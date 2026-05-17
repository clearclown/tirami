# tirami-zkml-bench-guest

Phase 25 B / Wave 5.2.1 — risc0 guest program for the `Risc0HostBackend`
verifier wired in Phase 24 Wave 5.2 (PR #115).

## Status

This crate ships as a **skeleton**. Until the operator runs
`cargo-risczero build`, this code compiles as a host-side stub that
prints "real ELF not built" rather than emitting a real Risc-V binary.

The skeleton is intentionally separate from the host workspace
(`crates/tirami-zkml-bench-guest` is NOT in the root `members =`) so
contributors without the Risc-V toolchain can still build the
host workspace.

## What it commits to (when built as a guest)

The guest reads five public inputs from the host's `ExecutorEnv`:

- `model_hash: [u8; 32]`
- `prompt_hash: [u8; 32]`
- `output_hash: [u8; 32]`
- `token_count: u64`
- `flops: u64`

It writes a single 32-byte commitment to the public journal:

```
commit = SHA-256( "tirami-risc0-commit-v1"
                 || model_hash
                 || prompt_hash
                 || output_hash
                 || token_count.to_le_bytes()
                 || flops.to_le_bytes() )
```

The host verifier (`tirami_zkml_bench::risc0_host::Risc0HostBackend::verify`)
re-derives the same commitment via `expected_journal_commit(&BenchSpec)`
and rejects any receipt whose journal disagrees. The cryptographic
STARK verification is performed by the host-side `risc0-zkvm` crate
(v1.2.x — wired in PR #115).

## Build (operator-side)

```bash
cargo install cargo-risczero
rzup install
cd crates/tirami-zkml-bench-guest
cargo risczero build
# → target/riscv-guest/riscv32im-risc0-zkvm-elf/release/bench-commit
```

## What this PR does NOT do

- **Embed the ELF into the host crate.** Wave 5.2.2 (next bounded
  PR) `include_bytes!` the operator-built ELF + image ID into
  `tirami-zkml-bench` under a new `risc0-host-prove` feature. Until
  that lands, `Risc0HostBackend::prove` still returns
  `BackendUnavailable("guest ELF required")`.
- **Add the guest crate to CI.** The Risc-V toolchain prebuild is
  multi-GB and would dominate CI time; operators who care about
  proving locally bootstrap the toolchain themselves following the
  steps above.

## Why the guest source is commented-out today

`risc0_zkvm::guest::env` and `risc0_zkvm::entry!` macros are only
defined when the `risc0-zkvm` crate is compiled for the
`riscv32im-risc0-zkvm-elf` target. Importing them under the host
target would fail at link time. Keeping the guest body as comments
+ a host-stub `main()` lets:

1. `cargo build` in this crate succeed on developer machines.
2. The full guest source still live next to the host code, so
   readers learn the protocol contract by reading the file.
3. Wave 5.2.2 swap-in is a literal "uncomment + delete stub" diff.

## Issue references

- Wave 5.2 (host-side verifier): PR #115
- Wave 5 design doc: `docs/phase-24-wave-5-zk-backends.md`
- This skeleton: closes #130 (the guest binary path is unblocked;
  the actual ELF build is the operator's problem until 5.2.2).
