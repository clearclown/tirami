//! Phase 25 B / Wave 5.2.1 — guest program skeleton.
//!
//! When built with `cargo risczero build`, this becomes the
//! `bench-commit` ELF that `Risc0HostBackend::prove` (host side)
//! drives via `risc0_zkvm::default_prover`. The skeleton commits
//! to the canonical `BenchSpec` SHA-256 so the host can
//! cross-check the receipt's journal against
//! `tirami_zkml_bench::risc0_host::expected_journal_commit`.
//!
//! See `docs/phase-24-wave-5-zk-backends.md` for the full plan.

// When `risc0-zkvm` is uncommented in Cargo.toml, this `#![no_main]`
// attribute together with `risc0_zkvm::entry!(main)` makes the
// crate a guest ELF instead of a host binary. We keep the
// host-stub form here (a plain `fn main()`) so contributors who
// haven't installed cargo-risczero can still `cargo build` the
// crate locally and reason about the surface area.

fn main() {
    // ---- Guest version (build under cargo-risczero) ----
    //
    // #![no_main]
    // risc0_zkvm::entry!(main);
    // use risc0_zkvm::guest::env;
    // use sha2::{Digest, Sha256};
    //
    // let model_hash: [u8; 32] = env::read();
    // let prompt_hash: [u8; 32] = env::read();
    // let output_hash: [u8; 32] = env::read();
    // let token_count: u64 = env::read();
    // let flops: u64 = env::read();
    //
    // let mut h = Sha256::new();
    // h.update(b"tirami-risc0-commit-v1");
    // h.update(model_hash);
    // h.update(prompt_hash);
    // h.update(output_hash);
    // h.update(token_count.to_le_bytes());
    // h.update(flops.to_le_bytes());
    // let commit: [u8; 32] = h.finalize().into();
    //
    // env::commit(&commit);
    //
    // ---- Host-stub form (this file as currently shipped) ----
    eprintln!(
        "tirami-zkml-bench-guest: host-stub build. Run \
         `cargo risczero build` to produce the real guest ELF; \
         see docs/phase-24-wave-5-zk-backends.md."
    );
}
