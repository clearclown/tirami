# Changelog

## Unreleased

- Start `0.2.0-dev.0` development from the `v0.1.0` public MVP baseline.
- Focus shifts to stabilization and closing the split-inference/runtime gap before a stable release.

## v0.1.0 - 2026-04-02

Initial public MVP prerelease of Forge.

- Encrypted seed/worker inference over Iroh QUIC
- Loopback-first HTTP API with optional bearer token protection
- Local CU-native ledger, persisted snapshots, and settlement export
- Capability handshake, topology planning groundwork, and protocol hardening

Known boundary at `v0.1.0`:

- Split inference is still target architecture, not the active runtime path
- `Forward` messages and topology planning exist, but real multi-stage execution is not shipped yet
- Stable release work starts from this baseline
