# social-mail

A decentralised, email-based social app: one binary with a scriptable CLI and an
interactive terminal UI, plus a headless list reflector.

Your identity is a key. Your posts travel as email. Nobody runs a server that
decides who you can follow.

Status: **v0.2, working reference implementation.** Signed events, encrypted
private posts, two-tier trust, the reflector and the terminal UI work end to end,
and messages now travel through a real mailbox: IMAP for receiving, SMTP for
sending, app passwords for both. A local spool directory is still available for
offline demos. See [`docs/IMPLEMENTATION-STATUS.md`](docs/IMPLEMENTATION-STATUS.md).

## What works now

| Area | State |
|---|---|
| Ed25519 + X25519 identities, PGP-shaped fingerprints | done |
| Signed events with canonical IDs, deterministic vectors | done |
| Contact cards, import with mismatch detection, explicit verification | done |
| Two-tier trust: unverified contacts readable, private posts only to verified followers | done |
| Follow requests, approvals, declines, removals | done |
| Private posts encrypted per recipient (ChaCha20-Poly1305) | done |
| Public list posts: signed, delivered in the clear to the list operator | done |
| Headless reflector with approved members, retries and per-member state | done |
| SQLite storage, sticky timeline positions, dedupe, orphan replies | done |
| TUI: four source tabs, three densities, conversation view, queue, problems | done |
| RTL text pipeline with Persian fixtures | done |
| IMAP receive + SMTP send with app passwords (TLS via rustls) | done |
| MIME envelope, protocol-folder moves, 2 MB ceiling | done |
| OAuth2 / XOAUTH2 sign-in | not yet (app passwords only) |
| Attachments, reactions, edits, search, multi-device | out of scope (see `TODO.md`) |

## Install

Download the archive for your platform from the releases page:

```text
social-mail-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
social-mail-v0.1.0-x86_64-pc-windows-msvc.zip
```

Each archive contains `social-mail` (`social-mail.exe` on Windows),
`social-mail-reflector`, this README and `PLAN.md`. Verify the download against
`SHA256SUMS.txt` before running it.

v0.2 artefacts are named `social-mail-v0.2.0-...`; the older `v0.1.0` artefacts use
the spool transport only.

From source (Rust 1.75 or newer):

```bash
cargo build --release
# binaries land in target/release/
```

## First run

```bash
social-mail init --name "Alice" --email alice@example.org
social-mail identity contact-card --out alice.socialmail
```

The wizard asks three questions: language, display name, address. Everything it
writes stays on your machine. There is no account to create and no service to
trust.

Exchange cards out of band, then confirm fingerprints:

```bash
social-mail contacts import bob.socialmail
social-mail contacts verify ABCD1234EFGH5678...   # or a 6-character suffix
social-mail follow request bob@example.org         # addresses and suffixes both work
```

Following is a request: nothing is shared until the other side approves. Private
posts go only to verified followers; unverified ones are marked `withheld`, never
silently dropped.

Posting and reading:

```bash
social-mail post --text "Hello from a keypair"
social-mail post --text "Public list post" --public-list list@example.org
social-mail timeline --limit 20
social-mail timeline --thread <EVENT_ID>
social-mail sync
social-mail queue status
```

Everything machine-readable:

```bash
social-mail timeline --json
social-mail diagnostics --json
```

## Connect a mailbox

v0.2 talks to any IMAP/SMTP provider that accepts app passwords — Fastmail, Proton
Bridge, self-hosted Dovecot, most VPS mail servers, and plenty more.

```bash
social-mail settings mail \
  --transport mail \
  --account alice@example.org \
  --imap-host imap.example.org --imap-port 993 --imap-tls implicit \
  --smtp-host smtp.example.org --smtp-port 587 --smtp-tls starttls

export SOCIAL_MAIL_PASSWORD='app-password-from-your-provider'
social-mail mail test          # signs in to IMAP and SMTP, sends nothing
social-mail mail test --send   # additionally posts a test message to yourself
social-mail sync
```

What you get, and what you do not:

- TLS is always verified (bundled Mozilla roots). There is no switch to turn it off.
- The password lives in an environment variable. It is never written to
  `social-mail.toml`, never stored in the database, never printed — and `Debug`
  output is redacted on purpose.
- `--imap-tls starttls` aborts if the server refuses the upgrade; there is no
  silent downgrade to plaintext.
- Processed protocol mail is moved to `SocialMail/` (set `mail.move_protocol_messages
  = false` to keep it in place). Quarantined mail stays put, flagged read, with the
  reason in `social-mail problems list`.
- Delivery language stays honest: "Accepted for delivery" means your mail server
  took it, nothing more. Sign-in failures pause sending and say so.
- OAuth2 (Gmail/Outlook) is not implemented; use an app password or a provider that
  offers one. See `TODO.md`.

Staying offline is still one flag: `social-mail init --offline` uses the spool
transport, and `social-mail settings mail --transport spool` switches back.

## Terminal UI

```bash
social-mail --tui      # or: social-mail tui
```

| Key | Action |
|---|---|
| `1` `2` `3` `4` | Following, Lists, You, All |
| `Z` | cycle density: compact, cozy, full (remembered per tab) |
| `j` `k` | move |
| `Enter` | open conversation |
| `n` / `R` | new post / reply |
| `r` | sync now (IMAP + SMTP when a mailbox is configured) |
| `g c` | contacts (then `v` verify, `b` block, `m` mute, `f` follow) |
| `g f` | follow requests (`v` verify & approve, `a` approve, `d` decline) |
| `g q` | queue (retry, cancel) |
| `g r` | replies to you |
| `g p` | problems (quarantined messages, with reasons) |
| `?` | help, `q` back or quit |

The feed shows root posts only, newest first, and positions are sticky: nothing
re-sorts under your cursor when mail arrives. Delivery wording is honest —
"Accepted for delivery", never "Read".

## Reflector (lists)

A reflector is a mailbox you control that relays a list. It never holds member
keys and never signs for an author.

```bash
social-mail-reflector init --list list@example.org
social-mail-reflector run --once          # or `run` to stay in the foreground
social-mail-reflector list-members --pending
social-mail-reflector approve-member 3F2A91C4 --force
social-mail-reflector remove-member 3F2A91C4
social-mail-reflector show-queue
social-mail-reflector retry-failed
```

Members ask to join from their own client; the operator approves. Approved
members are verified by definition — an unverified member is never relayed to.
The operator can read list traffic while relaying it; say so to your members.

## Security notes

- Backups are encrypted with PBKDF2-HMAC-SHA256 (200k rounds); the passphrase is
  read from `SOCIAL_MAIL_PASSPHRASE` or prompted for.
- Quarantined messages are stored with their reason; they are never shown in the
  feed.
- Key loss is documented, not solved: see `docs/security.md` and
  `docs/decisions/0002-crypto-backend.md` (OpenPGP was deferred for v0.1).
- Mail transport choices (rustls, hand-written clients, app passwords, folder
  moves) are recorded in `docs/decisions/0003-mail-transport.md`.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

Layered by design:

```text
crates/crypto    keys, signatures, encryption, backups
crates/protocol  envelopes, canonical JSON, carriers, contact cards
crates/storage   SQLite: events, timeline, contacts, deliveries, problems
crates/mail      IMAP/SMTP clients, MIME envelope, failure classification
crates/engine    application logic shared by CLI and TUI, transport abstraction
apps/cli         `social-mail` (CLI + TUI)
apps/social-mail-reflector
```

The engine is the only place that decides anything; both surfaces are thin
shells over it, which is what keeps CLI/TUI parity honest.

Releases are built by `.github/workflows/release.yml` on `v*` tags for Windows
and Linux only.

## Documents

- [`PLAN.md`](PLAN.md) — v0.1 requirements baseline
- [`docs/user-journey.md`](docs/user-journey.md) — product decisions D1–D36, journeys, copy
- [`docs/IMPLEMENTATION-STATUS.md`](docs/IMPLEMENTATION-STATUS.md) — what is done, what is left
- [`docs/protocol.md`](docs/protocol.md) — event envelope, canonicalisation, carriers
- [`docs/decisions/0003-mail-transport.md`](docs/decisions/0003-mail-transport.md) — IMAP/SMTP, TLS, credentials
- [`docs/security.md`](docs/security.md) — threat model and non-goals
- [`TODO.md`](TODO.md) — deferred work

Licence: AGPL-3.0-or-later.
