# Forge — Arquitectura

## Descripción General

Forge es un sistema de dos capas: **inferencia** y **economía**.

La capa de inferencia gestiona la distribución del modelo, la red mesh y el servicio de la API. Está construida sobre [mesh-llm](https://github.com/Mesh-LLM/mesh-llm).

La capa económica gestiona la contabilidad de CU, el registro de transacciones, los precios y los presupuestos de los agentes. Esta es la contribución original de Forge.

```
┌─────────────────────────────────────────────────┐
│  SDK / Límite de Integración                    │
│  Cualquier cliente puede integrar forge-node    │
│  como una biblioteca. Agentes de terceros,      │
│  tableros, adaptadores.                         │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Capa Económica (Original de Forge)             │
│                                                  │
│  ┌──────────────┐ ┌──────────┐ ┌─────────────┐ │
│  │ forge-ledger │ │ precios  │ │ presupuestos│ │
│  │ transacc. CU │ │ oferta / │ │ de agentes  │ │
│  │ reputación   │ │ demanda  │ │ /v1/forge/* │ │
│  │ rendimiento  │ │          │ │             │ │
│  └──────────────┘ └──────────┘ └─────────────┘ │
│                                                  │
│  ┌──────────────┐ ┌──────────────────────────┐  │
│  │ forge-verify │ │ forge-bridge (opcional)  │  │
│  │ firma doble  │ │ CU ↔ BTC Lightning      │  │
│  │ sinc. gossip │ │ CU ↔ stablecoin         │  │
└──────────────────┴──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Capa de Inferencia (Derivada de mesh-llm)      │
│                                                  │
│  ┌────────────┐ ┌───────────┐ ┌──────────────┐ │
│  │ red iroh   │ │ llama.cpp │ │ API OpenAI   │ │
│  │ QUIC+Noise │ │ pipeline  │ │ /v1/chat/    │ │
│  │ desc. Nostr│ │ shard MoE │ │ completions  │ │
└───────────────┘ └───────────┘ └──────────────┘ │
└─────────────────────────────────────────────────┘
```

## Capa de Inferencia (mesh-llm)

La capa de inferencia es responsable de:

- **Red Mesh**: Conexiones QUIC basadas en iroh con cifrado Noise.
- **Descubrimiento de pares**: Relés de Nostr para redes públicas, mDNS para LAN.
- **Distribución del modelo**: Paralelismo de pipeline para modelos densos, sharding de expertos para MoE.
- **Ejecución de inferencia**: llama.cpp a través de los subprocesos llama-server y rpc-server.
- **Servicio de API**: `/v1/chat/completions` y `/v1/models` compatibles con OpenAI.

Forge hereda todo esto de mesh-llm. La capa de inferencia no conoce los CU, las transacciones ni los precios.

## Capa Económica (Forge)

La capa económica se sitúa por encima de la inferencia y es responsable de:

### forge-ledger — El Motor Económico

```rust
pub struct ComputeLedger {
    balances: HashMap<NodeId, NodeBalance>,
    work_log: Vec<WorkUnit>,
    trade_log: Vec<TradeRecord>,
    price: MarketPrice,
}
```

Responsabilidades principales:
- Rastrear el saldo de CU por nodo (contribuido, consumido, reservado).
- Registrar cada transacción de inferencia (proveedor, consumidor, monto de CU, tokens).
- Calcular precios de mercado dinámicos a partir de la oferta y la demanda.
- Aplicar rendimiento (yield) a los nodos contribuyentes.
- Exportar estados de liquidación para puentes fuera del protocolo.
- Persistir instantáneas (snapshots) en el disco con integridad HMAC-SHA256.

### forge-verify — Prueba de Trabajo Útil (objetivo)

Asegura que las reclamaciones de CU sean legítimas:
- Protocolo de firma doble: tanto el proveedor como el consumidor firman cada TradeRecord.
- Sincronización por chismes (gossip): las transacciones firmadas se propagan por la red.
- Verificación: cualquier nodo puede validar ambas firmas.
- Detección de fraude: las transacciones no coincidentes o no firmadas son rechazadas.

### forge-bridge — Liquidación Externa (opcional)

Convierte el valor de CU a valor externo para los operadores que lo necesiten:
- Bitcoin Lightning: CU → msats a través de un tipo de cambio configurable.
- Stablecoin: CU → USDC/USDT a través de un adaptador.
- Fiat: CU → transferencia bancaria a través del tablero del operador.

La capa de puente está fuera del protocolo principal. Diferentes operadores pueden usar diferentes puentes.

### Superficie de la API

| Ruta | Capa | Descripción |
|-------|-------|-------------|
| `POST /v1/chat/completions` | Inferencia + Economía | Ejecutar inferencia, registrar transacción de CU |
| `GET /v1/models` | Inferencia | Listar modelos cargados |
| `GET /v1/forge/balance` | Economía | Saldo de CU, reputación |
| `GET /v1/forge/pricing` | Economía | Precio de mercado, estimaciones de costos |
| `GET /status` | Economía | Precio de mercado, estadísticas de red, transacciones recientes |
| `GET /topology` | Inferencia | Manifiesto del modelo, pares, plan de shard |
| `GET /settlement` | Economía | Historial de transacciones exportable |
| `GET /health` | Inferencia | Verificación de estado básica |

## Flujo de Datos

### Inferencia con Contabilidad de CU

```
El consumidor envía la solicitud
    ↓
La API recibe POST /v1/chat/completions
    ↓
El Ledger verifica: ¿puede pagarlo(consumidor, costo_estimado)?
    ↓ sí
La capa de inferencia ejecuta (llama-server / rpc-server)
    ↓
Los tokens se transmiten de vuelta al consumidor
    ↓
El Ledger registra la transacción:
  - proveedor.contribuido += costo_cu
  - consumidor.consumido += costo_cu
  - trade_log.push(TradeRecord)
    ↓
La respuesta incluye x_forge: { costo_cu, saldo_efectivo }
```

### Exportación de Liquidación

```
El operador ejecuta: forge settle --hours 24
    ↓
La API lee el trade_log para la ventana de tiempo
    ↓
Agrega por nodo: ganado_bruto, gastado_bruto, cu_neto
    ↓
Exporta estado JSON con precio de referencia opcional
    ↓
El operador usa el adaptador de puente para convertir el CU neto a BTC/fiat
```

## Modelo de Seguridad

```
Capa 0: Cadena principal de Bitcoin ← Anclaje opcional (futuro)
Capa 1: Firmas dobles              ← El proveedor y el consumidor firman cada transacción
Capa 2: Ledger HMAC-SHA256         ← Protección de integridad local
Capa 3: iroh (QUIC + Noise)        ← Cifrado de transporte
Capa 4: Ejecución de inferencia    ← El modelo se ejecuta localmente en el proveedor
```

Cada capa protege contra diferentes amenazas:
- Capa 4: Integridad del modelo (verificación de hash GGUF).
- Capa 3: Confidencialidad del transporte (escuchas indiscretas).
- Capa 2: Manipulación local (modificación de archivos).
- Capa 1: Fraude en la red (reclamaciones falsas de CU).
- Capa 0: Inmutabilidad histórica (anclaje opcional a Bitcoin).

## Dependencias de Crates

```
forge-core ← tipos compartidos (NodeId, CU, Config)
    ↑
forge-ledger ← motor económico (transacciones, precios, rendimiento)
    ↑
forge-lightning ← puente externo (billetera LDK, CU↔sats)
    ↑
forge-node ← orquestador (API HTTP, pipeline, integración de ledger)
    ↑
forge-cli ← CLI de referencia (chat, seed, worker, liquidar)

forge-net ← transporte P2P (iroh, QUIC, Noise, mDNS)
forge-proto ← mensajes de cable (bincode, 14 tipos de carga útil)
forge-infer ← motor de inferencia (llama.cpp, cargador GGUF)
forge-shard ← planificador de topología (asignación de capas, reequilibrio)
```
