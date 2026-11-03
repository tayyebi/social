# 0002 — Cryptography backend for v0.1

- Status: accepted
- Date: 2026-11-03
- Supersedes: the OpenPGP assumption in `PLAN.md` §1, §5 and §9 for this phase

## Context

`PLAN.md` asks for OpenPGP fingerprints as identities and OpenPGP operations for
signing and encryption. The phase had to ship working Windows and Linux binaries
with tests that run in CI, without a network-dependent, unverifiable dependency
choice.

At the time of writing:

- The Rust OpenPGP ecosystem (`rpgp`, `sequoia-openpgp`) could not be evaluated in
  the build environment: no Rust toolchain and no crates.io access in the agent
  sandbox, and the CI runner is the only place compilation happens.
- `sequoia-openpgp` pulls a C library (`nettle`) by default; its pure-Rust backend
  adds build complexity that could not be validated before shipping this phase.
- OpenPGP's feature surface (key flags, subkeys, certifications, AEAD options,
  compression) is much larger than v0.1 needs, and every part of it is a place to
  get the trust model wrong.

Shipping an OpenPGP implementation that was never compiled or tested against
vectors would be worse than shipping a smaller, well-tested scheme and saying so.

## Decision

v0.1 uses a purpose-built cryptographic envelope:

- **Identity**: Ed25519 signing key + X25519 encryption key, both generated
  locally and stored in a single `identity.json` (mode 0600 on Unix).
- **Fingerprint**: uppercase hex of the first 20 bytes of
  `SHA-256(ed25519_pub ‖ x25519_pub)`, rendered in groups of four like a PGP
  fingerprint. It is a 160-bit identifier with the same ergonomics, not a PGP
  fingerprint and not claimable as one.
- **Signatures**: Ed25519 over the canonical JSON bytes of the event.
- **Encryption**: X25519 ephemeral-static ECDH, key derived as
  `SHA-256("social-mail/v1/enc" ‖ shared ‖ eph_pub ‖ recipient)`, payload encrypted
  with ChaCha20-Poly1305. Wire format: `SMENC1` ‖ ephemeral public key (32) ‖
  nonce (12) ‖ ciphertext+tag.
- **Backups**: pretty JSON, PBKDF2-HMAC-SHA256 with 200k rounds.

Consequences that are stated to users rather than hidden:

- `social-mail` does not interoperate with OpenPGP mail clients, and a
  `social-mail` message cannot be verified by GnuPG.
- Fingerprints look like PGP fingerprints but are not the same thing.
- Key rotation is absent; key loss means publishing an advisory statement and
  starting a new identity.

## Alternatives considered

| Option | Why not now |
|---|---|
| `rpgp` (pure Rust) | Could not be compiled or vector-tested in this phase; its API surface is still moving. Revisit when the mail transport lands and CI can run its test suite. |
| `sequoia-openpgp` (nettle) | C dependency and system-library variance on Windows; heavier build than the phase budget allows. |
| `sequoia-openpgp` (crypto-rust) | Closest fit, but unverifiable here; a failure would have left the branch without binaries. |
| libsodium/sealed boxes | No signature scheme bundled; would still need Ed25519 separately. |

## Reversal criteria

Revisit this decision when **all** of the following hold:

1. A Rust OpenPGP implementation compiles reproducibly on Windows and Linux in CI.
2. It exposes detached signatures and per-recipient encryption with a stable API.
3. A migration path exists for existing identities (at minimum: sign a new key
   with the old one, publish both, keep old events verifiable).

The protocol envelope already supports versioned carriers (`version: 1`), so a
future `pgp-v1` carrier can coexist with `SMENC1` rather than replacing it.
