# Protocol

The wire format is JSON, versioned, and small on purpose. One message carries one
event. Encryption is per recipient; signatures are per event and never replaced.

## Event envelope

```json
{
  "author": "3F2A91C48D770B5E6C1D44A09E3877B25A6F0C21",
  "created_at": "2026-11-03T09:15:00Z",
  "nonce": "9f2c1d4e5a6b7c8d9e0f1a2b3c4d5e6f",
  "payload": { "audience": "followers", "lang": "fa", "text": "…" },
  "protocol": "social-mail",
  "type": "post",
  "version": 1
}
```

- `type` is one of `profile`, `post`, `reply`, `follow-request`, `follow-accept`,
  `follow-remove`, `route-update`.
- `nonce` makes otherwise identical events distinct.
- Unknown fields are rejected (`deny_unknown_fields`), unknown versions are
  rejected, and a mismatched `protocol` is rejected.

## Canonical bytes and event id

The canonical form is JSON with:

- keys sorted by UTF-16 code units,
- no insignificant whitespace,
- floats rejected (integers only),
- the same UTF-8 text as the original payload.

`event_id = hex(SHA-256(canonical_bytes))`. Signatures are Ed25519 over the same
bytes. Golden vectors live in `crates/protocol/fixtures/` and are asserted in
`crates/protocol/tests/fixtures.rs`, so a change in canonicalisation fails the
build rather than silently invalidating old messages.

## Carriers

A carrier is what actually travels:

```json
{
  "kind": "social-mail/carrier",
  "version": 1,
  "author": "…",
  "event_id": "…",
  "signature": "…",
  "event": { "…": "plain event, when unencrypted" },
  "encrypted": "SMENC1…",
  "card": { "…": "optional self-signed contact card" }
}
```

Exactly one of `event` and `encrypted` is present.

- **Plain carriers** are used for public list posts and whenever the recipient is
  the list operator. Anyone can read them; that is the point.
- **Encrypted carriers** contain one recipient's copy. Author, event id and
  signature are routing metadata only: the recipient decrypts, re-derives all
  three from the plaintext, and rejects any mismatch.
- **`card`** lets a recipient see who is writing before importing anything. A card
  whose fingerprint does not match the carrier author is ignored.

## Cryptographic primitives

- Identify: Ed25519 (signing) + X25519 (encryption).
- Fingerprint: `hex(SHA-256(ed25519_pub ‖ x25519_pub)[..20])`, uppercase, grouped.
- Encrypt: `SMENC1` ‖ eph_pub(32) ‖ nonce(12) ‖ ChaCha20-Poly1305 ciphertext,
  key = `SHA-256("social-mail/v1/enc" ‖ shared_secret ‖ eph_pub ‖ recipient)`.
- Backup: PBKDF2-HMAC-SHA256, 200k rounds, AES-256-GCM payload.

Rationale and the OpenPGP trade-off: `docs/decisions/0002-crypto-backend.md`.

## Payloads

| Type | Payload |
|---|---|
| `post` | `text`, optional `lang`, `audience` (`followers` or `public-list`), optional `list` |
| `reply` | `parent` (event id) + the same post fields |
| `follow-request` | optional `message`, the sender's `card` |
| `follow-accept` | `requester` fingerprint, the acceptor's `card` |
| `follow-remove` | optional reason |
| `profile` | `display_name`, optional `lang` |
| `route-update` | new `address` |

## Delivery semantics

- `followers` posts are queued only for verified followers. Others are recorded as
  `withheld` with the reason `unverified` — never silently lost.
- `public-list` posts are signed, not encrypted, and copied to the list operator if
  their address is a known contact.
- A reflector re-encrypts one copy per approved member, attaches the author's card,
  and leaves the author's signature untouched. Recipients verify the author, not
  the list.
- "Accepted for delivery" means the transport took the message. It never means the
  recipient read it.

## Invalid input

An event is quarantined, with a machine-readable reason, when:

- the signature does not verify, or the event id does not match the bytes;
- the author is unknown and no valid card travels with the message;
- a `follow-accept` names someone else;
- the payload does not parse or violates `deny_unknown_fields`;
- the message exceeds the size ceiling (once mail transport lands).

Blocked authors are dropped silently. Quarantined content is stored for
diagnostics, never displayed in the feed.
