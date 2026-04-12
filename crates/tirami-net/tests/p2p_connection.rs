//! Integration test: Two Iroh nodes connect and exchange Forge protocol messages.

use tirami_core::{DType, TensorMeta};
use tirami_net::ForgeTransport;
use tirami_proto::{Envelope, Forward, Hello, InferenceRequest, Payload, TokenStreamMsg};

#[tokio::test]
async fn two_nodes_connect_and_exchange_hello() {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

    // Create two transport instances (two Iroh endpoints)
    let transport_a = ForgeTransport::new().await.expect("transport A");
    let transport_b = ForgeTransport::new().await.expect("transport B");

    // Start accepting on node B
    let _accept_handle = transport_b.start_accepting();

    // Node A connects to Node B
    let addr_b = transport_b.endpoint_addr();
    let peer_b = transport_a.connect(addr_b).await.expect("connect to B");

    // Give the connection a moment to establish
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Node A sends a Hello message to Node B
    let hello = Envelope {
        msg_id: 1,
        sender: transport_a.tirami_node_id(),
        timestamp: 0,
        payload: Payload::Hello(Hello {
            version: 1,
            capability: tirami_core::PeerCapability {
                node_id: transport_a.tirami_node_id(),
                cpu_cores: 8,
                memory_gb: 16.0,
                metal_available: true,
                bandwidth_mbps: 100.0,
                battery_pct: None,
                available_memory_gb: 12.0,
                region: "test".to_string(),
            },
        }),
    };

    transport_a
        .send_to(peer_b.peer_id(), &hello)
        .await
        .expect("send hello");

    // Node B receives the Hello
    let (sender_id, received) = transport_b.recv().await.expect("receive message");

    match received.payload {
        Payload::Hello(h) => {
            assert_eq!(h.version, 1);
            assert_eq!(h.capability.cpu_cores, 8);
            assert!(h.capability.metal_available);
            println!("Node B received Hello from {}", sender_id);
        }
        other => panic!("Expected Hello, got {:?}", std::mem::discriminant(&other)),
    }

    // Cleanup
    transport_a.close().await;
    transport_b.close().await;
}

#[tokio::test]
async fn multiple_messages_in_sequence() {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

    let transport_a = ForgeTransport::new().await.expect("transport A");
    let transport_b = ForgeTransport::new().await.expect("transport B");

    let _accept_b = transport_b.start_accepting();

    // A connects to B
    let addr_b = transport_b.endpoint_addr();
    let peer_b = transport_a.connect(addr_b).await.expect("connect");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Send 3 messages from A to B
    for i in 0..3 {
        let msg = Envelope {
            msg_id: i,
            sender: transport_a.tirami_node_id(),
            timestamp: i,
            payload: Payload::Heartbeat(tirami_proto::Heartbeat {
                uptime_sec: i * 100,
                load: 0.5,
                memory_free_gb: 8.0,
                battery_pct: None,
            }),
        };
        transport_a
            .send_to(peer_b.peer_id(), &msg)
            .await
            .expect("send message");
    }

    // B receives all 3
    for i in 0..3 {
        let (_peer, received) = transport_b.recv().await.expect("receive message");

        match received.payload {
            Payload::Heartbeat(hb) => {
                assert_eq!(hb.uptime_sec, i * 100);
                println!("Received heartbeat #{}", i);
            }
            other => panic!(
                "Expected Heartbeat, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    transport_a.close().await;
    transport_b.close().await;
}

#[tokio::test]
async fn forward_activation_tensor_over_p2p() {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

    let transport_a = ForgeTransport::new().await.expect("transport A");
    let transport_b = ForgeTransport::new().await.expect("transport B");
    let _accept_b = transport_b.start_accepting();

    let addr_b = transport_b.endpoint_addr();
    let peer_b = transport_a.connect(addr_b).await.expect("connect");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Simulate activation tensor: 4096 floats (typical hidden_dim for 7B model)
    let activation: Vec<f32> = (0..4096).map(|i| i as f32 * 0.001).collect();
    let byte_len = activation.len() * 4;
    let mut tensor_data = Vec::with_capacity(byte_len);
    for &val in &activation {
        tensor_data.extend_from_slice(&val.to_le_bytes());
    }

    let forward_msg = Envelope {
        msg_id: 100,
        sender: transport_a.tirami_node_id(),
        timestamp: 0,
        payload: Payload::Forward(Forward {
            request_id: 42,
            sequence_pos: 0,
            tensor_meta: TensorMeta {
                shape: vec![1, 4096],
                dtype: DType::F32,
                byte_len: byte_len as u32,
            },
            tensor_data,
        }),
    };

    transport_a
        .send_to(peer_b.peer_id(), &forward_msg)
        .await
        .expect("send forward");

    let (_peer, received) = transport_b.recv().await.expect("receive forward");

    match received.payload {
        Payload::Forward(fwd) => {
            assert_eq!(fwd.request_id, 42);
            assert_eq!(fwd.tensor_meta.shape, vec![1, 4096]);
            assert_eq!(fwd.tensor_meta.dtype, DType::F32);
            assert_eq!(fwd.tensor_data.len(), 4096 * 4);
            println!("Forward activation received: {} floats OK", 4096);
        }
        other => panic!("Expected Forward, got {:?}", std::mem::discriminant(&other)),
    }

    transport_a.close().await;
    transport_b.close().await;
}

/// Full bidirectional: Worker sends InferenceRequest, Seed streams TokenStreamMsg back.
#[tokio::test]
async fn bidirectional_inference_request_and_token_stream() {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

    let transport_worker = ForgeTransport::new().await.expect("worker");
    let transport_seed = ForgeTransport::new().await.expect("seed");
    let _accept_seed = transport_seed.start_accepting();

    // Worker connects to seed (also starts read loop for responses)
    let addr_seed = transport_seed.endpoint_addr();
    let peer_seed = transport_worker
        .connect(addr_seed)
        .await
        .expect("connect to seed");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Worker → Seed: InferenceRequest
    let req = Envelope {
        msg_id: 200,
        sender: transport_worker.tirami_node_id(),
        timestamp: 0,
        payload: Payload::InferenceRequest(InferenceRequest {
            request_id: 200,
            prompt_text: "What is gravity?".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 0.9,
        }),
    };

    transport_worker
        .send_to(peer_seed.peer_id(), &req)
        .await
        .expect("send request");

    // Seed receives InferenceRequest
    let (worker_peer_id, received) =
        tokio::time::timeout(std::time::Duration::from_secs(5), transport_seed.recv())
            .await
            .expect("timeout")
            .expect("receive request");

    match &received.payload {
        Payload::InferenceRequest(r) => {
            assert_eq!(r.request_id, 200);
            assert_eq!(r.prompt_text, "What is gravity?");
        }
        other => panic!(
            "Expected InferenceRequest, got {:?}",
            std::mem::discriminant(other)
        ),
    }

    // Seed → Worker: stream 4 token fragments
    let fragments = ["Gravity", " is", " a", " force."];
    for (i, text) in fragments.iter().enumerate() {
        let is_final = i == fragments.len() - 1;
        let msg = Envelope {
            msg_id: 200 * 10000 + i as u64,
            sender: transport_seed.tirami_node_id(),
            timestamp: 0,
            payload: Payload::TokenStream(TokenStreamMsg {
                request_id: 200,
                text: text.to_string(),
                is_final,
            }),
        };
        transport_seed
            .send_to(&worker_peer_id, &msg)
            .await
            .expect("send token");
    }

    // Worker collects streamed tokens
    let mut result = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("Timed out. Got: '{}'", result);
        }
        match tokio::time::timeout(remaining, transport_worker.recv()).await {
            Ok(Some((_peer, resp))) => {
                if let Payload::TokenStream(ts) = resp.payload {
                    if ts.request_id == 200 {
                        result.push_str(&ts.text);
                        if ts.is_final {
                            break;
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(_) => panic!("Timeout. Got: '{}'", result),
        }
    }

    assert_eq!(result, "Gravity is a force.");
    println!("Bidirectional inference flow OK: \"{}\"", result);

    transport_worker.close().await;
    transport_seed.close().await;
}

#[tokio::test]
async fn transport_rejects_spoofed_sender_identity() {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

    let transport_a = ForgeTransport::new().await.expect("transport A");
    let transport_b = ForgeTransport::new().await.expect("transport B");
    let _accept_b = transport_b.start_accepting();

    let addr_b = transport_b.endpoint_addr();
    let peer_b = transport_a.connect(addr_b).await.expect("connect");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let spoofed = Envelope {
        msg_id: 999,
        sender: tirami_core::NodeId([9u8; 32]),
        timestamp: 0,
        payload: Payload::Heartbeat(tirami_proto::Heartbeat {
            uptime_sec: 1,
            load: 0.1,
            memory_free_gb: 1.0,
            battery_pct: None,
        }),
    };

    transport_a
        .send_to(peer_b.peer_id(), &spoofed)
        .await
        .expect("send spoofed");

    let result =
        tokio::time::timeout(std::time::Duration::from_millis(300), transport_b.recv()).await;

    assert!(result.is_err(), "spoofed envelope should be dropped");

    transport_a.close().await;
    transport_b.close().await;
}

#[tokio::test]
async fn transport_drops_duplicate_message_ids_from_same_peer() {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

    let transport_a = ForgeTransport::new().await.expect("transport A");
    let transport_b = ForgeTransport::new().await.expect("transport B");
    let _accept_b = transport_b.start_accepting();

    let addr_b = transport_b.endpoint_addr();
    let peer_b = transport_a.connect(addr_b).await.expect("connect");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let heartbeat = Envelope {
        msg_id: 1_234,
        sender: transport_a.tirami_node_id(),
        timestamp: 0,
        payload: Payload::Heartbeat(tirami_proto::Heartbeat {
            uptime_sec: 1,
            load: 0.25,
            memory_free_gb: 4.0,
            battery_pct: None,
        }),
    };

    transport_a
        .send_to(peer_b.peer_id(), &heartbeat)
        .await
        .expect("send first heartbeat");
    transport_a
        .send_to(peer_b.peer_id(), &heartbeat)
        .await
        .expect("send duplicate heartbeat");

    let (_peer_id, received) = transport_b.recv().await.expect("first heartbeat");
    match received.payload {
        Payload::Heartbeat(hb) => assert_eq!(hb.uptime_sec, 1),
        other => panic!(
            "Expected Heartbeat, got {:?}",
            std::mem::discriminant(&other)
        ),
    }

    let result =
        tokio::time::timeout(std::time::Duration::from_millis(300), transport_b.recv()).await;
    assert!(result.is_err(), "duplicate message should be dropped");

    transport_a.close().await;
    transport_b.close().await;
}
