# Contributing to Forge

Thank you for considering a contribution. Forge is a compute-as-currency
protocol where the unit of account is 10^9 FLOPs of verified inference.
The code is in Rust (5-layer workspace, 12 crates) with Python SDK + MCP
clients. This document covers how to build, test, and submit changes.

## Quick start for contributors

```bash
git clone https://github.com/clearclown/forge
cd forge

# Build + run all tests (~2-3 min cold, ~30s incremental)
cargo build --release
cargo test --workspace

# Run the conformance suite (95 assertions against parameters.md)
bash scripts/verify-impl.sh

# Run the one-command end-to-end demo (downloads SmolLM2-135M, ~30s cold)
bash scripts/demo-e2e.sh
```

## Before you start

Open a GitHub issue first if you are planning a non-trivial change. Small
fixes (typos, tiny bugs, docs) can go straight to a PR. Large changes
(new crates, new protocol messages, new economic primitives) should be
discussed in an issue before you write the code — it's cheaper to align
on design than to rewrite a big diff after review.

## Philosophy

Three invariants that every PR must respect:

1. **Theory ↔ implementation parity.** Every numeric constant in the
   protocol lives in one place: `forge-economics/spec/parameters.md`.
   Rust code references those constants via named `pub const` values in
   `crates/tirami-ledger/src/lending.rs` or equivalent. Drift is caught
   by `scripts/verify-impl.sh` (95/95 GREEN today) and audited in
   `docs/THEORY-AUDIT.md`. If you add or change a constant, update the
   spec first, then the code, then add a `verify-impl.sh` assertion.

2. **Tests first.** We use TDD. For new handlers, copy the pattern in
   `crates/tirami-node/src/handlers/bank.rs` — handler function +
   `#[cfg(test)]` block + a `verify-impl.sh` grep assertion. Never
   merge a PR without tests for the new code path.

3. **No panics in library code.** Use `Result<T, ForgeError>` or a
   crate-local error enum. `unwrap()` and `expect()` are acceptable in
   tests and in initialization-only code paths where failure means the
   program should abort anyway. Everywhere else, return an error.

## How to build

Rust edition 2024, resolver v2, workspace of 12 crates. Apple Metal
acceleration is enabled by default; CUDA and ROCm work via llama.cpp
build flags (inherited through `llama-cpp-2`).

```bash
cargo build --release              # full workspace
cargo build --release -p tirami-cli  # just the CLI binary
cargo check --workspace             # fast type-check
cargo clippy --workspace            # lint (71 baseline warnings, don't add new ones)
```

## How to test

Three test suites, all must be green before merge:

```bash
cargo test --workspace --no-fail-fast    # 426 unit + integration tests
bash scripts/verify-impl.sh              # 95 grep-based spec assertions
bash scripts/demo-e2e.sh                 # real end-to-end with SmolLM2-135M
```

The `verify-impl.sh` script is a conformance suite. It uses `grep` to
confirm that specific constants, struct names, and function signatures
exist in specific files. This is how we prevent theory-implementation
drift — if you change a constant name, you must update both the code
and the assertion in the script.

## Adding a new HTTP endpoint

Follow `crates/tirami-node/src/handlers/bank.rs` exactly:

1. Write the handler function in `crates/tirami-node/src/handlers/<layer>.rs`
2. Register it in `crates/tirami-node/src/api.rs` inside
   `create_router_with_services()`, in the protected router section
3. Add at least one test using `test_router_default()` from the same file
4. Add a `verify-impl.sh` assertion if the endpoint is spec-relevant

## Adding a new economic primitive

Follow `crates/tirami-ledger/src/lending.rs` exactly:

1. **First**, send a PR against `clearclown/forge-economics` that adds
   the new constant to `spec/parameters.md` under an appropriate section
2. After that lands, create a `pub const NAME: f64 = VALUE;` in
   `lending.rs` (or the appropriate ledger submodule) with a doc comment
   citing `parameters.md §N`
3. Add at least two unit tests per new function (happy path + edge case)
4. Add a `verify-impl.sh` grep assertion that the constant + value exist

Never hardcode a magic number inline — always lift it to a named constant.

## Code style

- Default `rustfmt` (no custom `rustfmt.toml`)
- `cargo clippy --workspace` — don't add new warnings beyond the 71 baseline
- No `unsafe` unless there's a documented reason
- No `panic!` / `unwrap()` / `expect()` in library code (tests are fine)
- Imports grouped: `std`, third-party, `crate::`, `super::`, `self::`
- Doc comments (`///`) on every public item, with a one-sentence summary

## Commit messages

Conventional Commits style:

```
feat: ...       new feature
fix: ...        bug fix
docs: ...       documentation only
test: ...       tests only
chore: ...      housekeeping
refactor: ...   no behavior change
perf: ...       performance improvement
```

First line ≤ 72 characters. Body explains the *why*, not the *what*
(the diff is the what). Reference issues with `#N` where relevant.

## Pull requests

One logical change per PR. Tests included. CHANGELOG entry under
`[Unreleased]`. All 3 test suites green. Docs updated if API changed.

Reviewers look at:

1. Does it add tests?
2. Does it break any existing test?
3. Does it drift from `parameters.md`?
4. Is the commit message honest about the change?
5. Is the code style consistent with the rest of the repo?

## Code of Conduct

Be kind, be technical, critique decisions not people. See
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for the full Contributor Covenant.

## Security issues

Do NOT open public issues for security vulnerabilities. See
[SECURITY.md](SECURITY.md) for the disclosure policy.

## Help

- Read [CLAUDE.md](CLAUDE.md) for the architecture overview
- Read `docs/developer-guide.md` for a longer walkthrough
- Open a `question` issue if anything is unclear
