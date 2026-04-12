# Forge — Developer Guide

- [Repo layout](#repo-layout)
- [Build](#build)
- [Test](#test)
- [Code style](#code-style)
- [Adding a new HTTP endpoint](#adding-a-new-http-endpoint)
- [Adding a new economic primitive](#adding-a-new-economic-primitive)
- [Tests first](#tests-first)
- [Commit messages](#commit-messages)
- [PR guidelines](#pr-guidelines)

---

## Repo layout

```
forge/
├── crates/                     (12 Rust crates)
│   ├── tirami-core              shared types (NodeId, CU, Config)
│   ├── tirami-ledger            L1 economy — THE highest-value code
│   ├── tirami-proto             wire protocol (bincode, 27+ message types)
│   ├── tirami-net               P2P transport (iroh QUIC + Noise)
│   ├── tirami-infer             llama.cpp integration (GGUF, Metal/CUDA)
│   ├── forge-shard             topology planner (layer assignment)
│   ├── tirami-node              HTTP API + node orchestrator (all 5 layers)
│   ├── tirami-cli               reference CLI (chat, seed, worker, settle)
│   ├── tirami-lightning         CU↔BTC bridge (LDK wallet, Lightning)
│   ├── tirami-bank              L2 finance (strategies, portfolios, futures, insurance)
│   ├── tirami-mind              L3 self-improvement (harness, CuBudget, MetaOptimizer)
│   └── tirami-agora             L4 marketplace (AgentRegistry, ReputationCalculator)
├── sdk/python/                 tirami-sdk (PyPI, 20 methods)
├── mcp/                        forge-cu-mcp (PyPI, MCP server, 20 tools)
├── docs/                       technical documentation
└── scripts/
    ├── demo-e2e.sh             one-command end-to-end demo
    └── verify-impl.sh          95-assertion conformance suite
```

The crate dependency order is: `tirami-core` → `tirami-ledger` → `tirami-lightning` → `tirami-node` → `tirami-cli`. `tirami-net`, `tirami-proto`, `tirami-infer`, `forge-shard` are independent from the economic stack and feed into `tirami-node`.

When choosing where to make a change:
- Economic logic always lives in `tirami-ledger`.
- HTTP routing always lives in `tirami-node/src/api.rs` and `tirami-node/src/handlers/`.
- Shared types (NodeId, TRM amounts, Config) always live in `tirami-core/src/types.rs`.
- Wire protocol additions always go in `tirami-proto/src/messages.rs`.

---

## Build

```bash
cargo build --release          # full workspace, ~2-3 min cold on M-series
cargo build --release -p tirami-cli   # CLI only (faster if you don't need tests)
cargo check --workspace        # fast type check, no codegen (~15s)
cargo clippy --workspace       # lint (71 baseline warnings accepted for now)
```

Rust edition 2024, resolver v2. Apple Metal is ON by default on macOS builds. CUDA and ROCm require explicit feature flags:

```bash
cargo build --release --features cuda    # NVIDIA
cargo build --release --features rocm    # AMD
cargo build --release --features vulkan  # cross-vendor GPU
```

For CPU-only builds, omit all GPU features.

---

## Test

Three distinct test suites, all expected to be green before merging:

**1. Unit + integration tests (426 tests)**:

```bash
cargo test --workspace
# or for a single crate:
cargo test --package tirami-ledger
```

Tests live in `#[cfg(test)] mod tests` blocks in each source file, plus integration tests in `crates/*/tests/` directories. TDD is expected: write the test before the implementation.

**2. Conformance assertions (95 assertions)**:

```bash
bash scripts/verify-impl.sh
```

This script greps the source tree for specific constant values, struct names, and function names that must match `forge-economics/spec/parameters.md`. All 95 assertions must be GREEN. This is the primary mechanism to prevent theory-implementation drift. Current state: 95/95 GREEN.

**3. End-to-end demo (full stack)**:

```bash
bash scripts/demo-e2e.sh
```

Downloads SmolLM2-135M (~100 MB), starts a real forge node, runs 3 real chat completions through llama.cpp, then exercises every Phase 1-12 endpoint with live data. Takes about 30 seconds after the binary is built. Verified on Apple Silicon Metal GPU.

---

## Code style

- **Formatting**: default `rustfmt` (no `rustfmt.toml`). Run `cargo fmt --all` before committing.
- **Linting**: `cargo clippy --workspace`. There are 71 baseline warnings in the current codebase that are accepted; do not introduce new clippy errors.
- **Error handling**: `ForgeError` enum for all library-level errors (defined in `tirami-core/src/lib.rs`). Use `anyhow` only in `tirami-cli`. Never use `.unwrap()` or `.expect()` in library code — propagate with `?`.
- **Async**: tokio runtime everywhere. `Arc<Mutex<T>>` for shared state accessed across tasks.
- **Logging**: `tracing` crate. INFO for user-visible events, DEBUG for protocol details. Do not log sensitive data (bearer tokens, prompt content) at INFO or above.
- **Serialization**: `serde` with `#[derive(Serialize, Deserialize)]` for JSON and config. `bincode` for wire protocol. Do not mix the two on the same type.
- **No unsafe**: unless absolutely necessary and documented with a safety comment explaining why the invariant holds.
- **No panics in library code**: `Result<T, ForgeError>` everywhere in `tirami-ledger`, `tirami-bank`, `tirami-mind`, `tirami-agora`. Panics are acceptable only in tests and in CLI `main()`.

---

## Adding a new HTTP endpoint

Follow the pattern in `crates/tirami-node/src/handlers/bank.rs`:

1. Create the handler function in a file under `crates/tirami-node/src/handlers/`. The signature is:

   ```rust
   pub async fn my_endpoint(
       State(state): State<Arc<AppState>>,
       // optionally: Json(body): Json<MyRequest>,
   ) -> Result<Json<MyResponse>, StatusCode>
   ```

2. Register it in `crates/tirami-node/src/api.rs` inside `create_router_with_services()`:

   ```rust
   .route("/v1/tirami/my-endpoint", get(handlers::my_module::my_endpoint))
   ```

3. Add it to the `protected` router block (routes that require the bearer token), not the public router, unless it's specifically meant to be unauthenticated (like `/metrics` or `/health`).

4. Add at least one test using the `test_router_default` helper defined in `crates/tirami-node/src/api.rs`:

   ```rust
   #[tokio::test]
   async fn test_my_endpoint() {
       let app = test_router_default();
       let response = app
           .oneshot(Request::builder().uri("/v1/tirami/my-endpoint").body(Body::empty()).unwrap())
           .await
           .unwrap();
       assert_eq!(response.status(), StatusCode::OK);
   }
   ```

5. If the endpoint is spec-relevant (involves a constant from parameters.md), add a `verify-impl.sh` assertion — see the section below.

---

## Adding a new economic primitive

Follow the pattern in `crates/tirami-ledger/src/lending.rs`:

1. **Update the spec first.** Add the numeric constants to `forge-economics/spec/parameters.md` with a PR against the `clearclown/forge-economics` repo. Do not hard-code values in Rust before they exist in the spec.

2. **Implement with matching constants.** In `tirami-ledger` (or the appropriate L2/L3/L4 crate), declare:

   ```rust
   /// Minimum reserve ratio for the lending pool.
   /// Source: parameters.md §5
   pub const MIN_RESERVE_RATIO: f64 = 0.30;
   ```

   The value must exactly match the spec. The verify-impl.sh script greps for the literal value — mismatches fail the conformance suite.

3. **Reference the spec section in doc comments.** Every constant and function that derives from parameters.md should cite the section:

   ```rust
   /// Credit score weight for repayment history.
   /// Source: parameters.md §4 `weight_repayment`
   pub const WEIGHT_REPAYMENT: f64 = 0.40;
   ```

4. **Add tests.** At minimum:
   - One test that verifies the happy path (valid input, expected output).
   - One test that verifies the error path (invalid input, correct error variant).
   - For anything involving circuit breakers or thresholds, add a boundary test at the threshold value.

5. **Integrate into the API.** New ledger primitives should be exposed via a `tirami-node` HTTP endpoint. See "Adding a new HTTP endpoint" above.

---

## Tests first

The `verify-impl.sh` script is how the spec is enforced in code. When you add a new constant from parameters.md to the Rust codebase, you **must** add a corresponding `verify-impl.sh` grep assertion. The pattern is:

```bash
check "CONSTANT_NAME exists" grep -r "pub const CONSTANT_NAME" crates/
check "CONSTANT_NAME value matches spec" grep -r "CONSTANT_NAME.*=.*0.40" crates/
```

Current state: 95/95 GREEN. Any PR that turns a previously-green assertion red will be blocked.

The `docs/THEORY-AUDIT.md` file tracks the current match count between `forge-economics/spec/parameters.md` and the implementation: **43 match / 0 drift** as of Phase 12. Keep this file updated when you add spec-relevant constants.

---

## Commit messages

Follow the conventional-commits style already used in this repo:

```
feat: add /v1/tirami/anchor endpoint for Bitcoin OP_RETURN
fix: prevent reputation underflow below 0.0 on penalty accumulation
docs: update operator-guide with Phase 12 metric series
chore: bump llama-cpp-2 to 0.1.1
test: add boundary test for MIN_RESERVE_RATIO at 0.30
refactor: extract collusion detector into tirami-ledger::collusion module
```

Rules:
- First line ≤ 72 characters.
- Body explains the "why", not the "what" (the diff is the what).
- Reference the Phase or batch ID where relevant: `(Phase 12 A3)`.
- Do not use `git commit --amend` to rewrite merged history. Create a new commit.

---

## PR guidelines

- One logical change per PR. Split large features into sequential PRs if needed.
- All three test suites green: `cargo test --workspace`, `bash scripts/verify-impl.sh`, `bash scripts/demo-e2e.sh`.
- Tests included. No exception. If you're fixing a bug, add a test that would have caught the bug.
- Docs updated if the public API changed (endpoint added, request/response shape changed, constant value changed).
- Add a CHANGELOG entry under `[Unreleased]` for any user-visible change.
- For large changes (new crate, new layer, protocol change), open an issue first to align on design before writing code.

---

See also: [forge-economics/papers/compute-standard.md](https://github.com/clearclown/forge-economics) for the theoretical underpinnings, [CLAUDE.md](../CLAUDE.md) for current implementation status, and [docs/operator-guide.md](operator-guide.md) for deployment concerns.
