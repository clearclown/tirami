# PGP Key Setup for the Tirami Security Contact

The placeholder in `SECURITY.md` must be replaced with a real PGP
public key before the bug bounty program goes live. The key should
be owned by the same party that operates the `security@` email
address, and its private half must live exclusively on infrastructure
that party controls — not in this repository, not in any CI system,
and ideally not on any always-online server.

## One-time setup (operator runs this locally)

```bash
# 1. Install GPG (macOS: already present; Linux: `apt install gnupg`).

# 2. Generate a dedicated Ed25519 signing + encryption key pair.
#    Ed25519 is preferred over RSA for new keys (smaller, faster,
#    wider tooling support). Replace the email with your real one.
gpg --quick-generate-key "Tirami Security <security@YOUR-DOMAIN.example>" \
    ed25519 sign,cert,encr 2y

# 3. Capture the fingerprint for the SECURITY.md replacement.
gpg --fingerprint security@YOUR-DOMAIN.example

# 4. Export the ASCII-armored public key.
gpg --armor --export security@YOUR-DOMAIN.example > tirami-security.pub.asc

# 5. Upload to at least two independent keyservers so reporters
#    can verify out-of-band.
gpg --keyserver hkps://keys.openpgp.org \
    --send-keys $(gpg --fingerprint security@YOUR-DOMAIN.example | \
                  awk '/^\s+[A-F0-9 ]+$/ { gsub(/ /, ""); print; exit }')
gpg --keyserver hkps://keyserver.ubuntu.com \
    --send-keys $(gpg --fingerprint security@YOUR-DOMAIN.example | \
                  awk '/^\s+[A-F0-9 ]+$/ { gsub(/ /, ""); print; exit }')

# 6. Back up the private key to an offline medium.
gpg --armor --export-secret-keys security@YOUR-DOMAIN.example > \
    tirami-security.key.asc
# Move `tirami-security.key.asc` to your hardware security module,
# paper backup, or encrypted offline disk. Delete it from any
# temporary file locations afterward.
```

## Updating SECURITY.md

1. Open `SECURITY.md` at the repo root.
2. Replace the `-----BEGIN PGP PUBLIC KEY BLOCK-----` placeholder
   block with the contents of `tirami-security.pub.asc`.
3. Replace the fingerprint placeholder with the output of
   `gpg --fingerprint security@YOUR-DOMAIN.example`.
4. Commit on a separate branch and open a PR. The fingerprint
   change is part of the normal code review flow; reviewers should
   verify the fingerprint matches what they can independently
   fetch from the keyservers.

## Rotation policy

- Key validity: 2 years (set above via the `2y` TTL).
- Rotate at least 6 months before expiry.
- On rotation: publish a signed rotation notice encrypted to the
  old key, move the public key update through the same PR process.

## What happens if the private key is lost

Issue a revocation certificate using your key's offline revocation
key (generated separately — ask the operator to follow the GnuPG
handbook chapter on subkeys). Publish the revocation to the same
keyservers. Draft a replacement key, follow the normal SECURITY.md
update flow.

## Why not just an X.509 / S/MIME cert

S/MIME certs bind identity to an email address via a CA, which
adds a trust dependency we don't need and doesn't help with
out-of-band verification. PGP's web-of-trust is lightweight,
independent of any CA, and interoperable with every security
researcher's existing workflow. The cost is manual fingerprint
verification on first contact — which is the behavior we want
anyway for high-stakes reports.
