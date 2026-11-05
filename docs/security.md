# Security notes

Scope: the v0.1 reference implementation in this repository. It is not audited,
not anonymity software, and not a replacement for a messenger with forward
secrecy.

## What the design protects

| Property | How |
|---|---|
| Message integrity and authorship | Every event is signed with the author's Ed25519 key; the event id is the SHA-256 of the canonical bytes, so any edit changes the id and breaks the signature. |
| Private post confidentiality | One X25519 + ChaCha20-Poly1305 copy per verified follower. No shared group key, no list key. |
| Contact authenticity | Contact cards are self-signed; a fingerprint that does not match the card is a hard error, never a silent merge. |
| Trust decisions are local | Verification, mute and block live in your database. No server can mark someone verified for you. |
| Replay protection | Duplicate event ids are ignored; encrypted carriers re-derive author, id and signature from the decrypted event. |
| Secret hygiene | The identity file is 0600 on Unix; backups are encrypted; diagnostics never print key material. |

## Credentials

v0.2 signs in with an app password read from `SOCIAL_MAIL_PASSWORD` (or the variable
named in `mail.password_env`). The password is kept in memory for one operation and
is never written to `social-mail.toml`, never stored in SQLite, never printed by
`diagnostics`, and never included in error text or `Debug` output.

Use an app password, not your account password: it can be revoked on its own, and
it usually bypasses second-factor prompts. If a provider only offers OAuth2, this
build cannot talk to it yet (`TODO.md`).

TLS is mandatory in practice: `implicit` and `starttls` are verified against
bundled Mozilla roots, and a refused STARTTLS upgrade aborts the connection instead
of continuing in the clear. `none` exists for a local relay and is labelled as such.

## What it does not protect

- **Metadata.** Your mail provider sees who you write to, when, and how often.
  Subject lines and timestamps leak the same things email always leaks. Our subject
  is always `SocialMail/v1 carrier`, so the envelope still shows the two addresses
  and the time.
- **Server-side copies.** Processed mail is moved to `SocialMail/` and quarantined
  mail is left in place: both remain readable by whoever runs the mailbox.
- **Traffic analysis and anonymity.** There is no onion routing, no padding, and
  no cover traffic.
- **A compromised endpoint.** Malware, a shared account, or a keylogger defeats
  everything above.
- **Forward secrecy.** Losing a long-term encryption key exposes past ciphertexts
  that were captured.
- **List privacy from the operator.** A reflector decrypts list traffic in order
  to re-encrypt it per member. Operators can read it. Say so before people join.
- **Denial of service.** Anyone who learns your address can send you mail; the
  client quarantines what it cannot verify but cannot stop it arriving.

## Threat model in one paragraph

Assume a curious mail provider and a passive network observer: they learn
metadata, not content. Assume an active attacker who can send arbitrary messages:
they cannot forge a signature, cannot make you display a quarantined message, and
cannot make a different key claim a contact you already know. Assume an attacker
who can read your disk: they get everything the app has, so use disk encryption.

## Key loss

There is no recovery service and no key escrow. If you lose your identity file
and your encrypted backup, you can:

1. Generate a new identity.
2. Publish a revocation statement (`social-mail identity revocation --out loss.txt`)
   that says the old fingerprint is dead, signed by the new key.
3. Ask contacts to re-import your new card and re-verify.

The statement is advisory: nothing in v0.1 can force a peer to stop trusting an
old key. This is documented rather than papered over; see `TODO.md` for rotation.

## Reporting

This is a reference implementation without a security response process. Do not
deploy it to protect people at risk.
