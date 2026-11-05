# Implementation status

Last updated: 2026-11-05, branch `arena/beca6cfd-social`, phase "v0.2 mail
transport: generic IMAP/SMTP with app passwords".

This file is the honest scoreboard. `PLAN.md` says what v0.1 must do;
`docs/user-journey.md` says how it must feel; this page says what the code does
right now.

## Done

### Protocol and cryptography

- Identity: Ed25519 signing key + X25519 encryption key, generated locally;
  fingerprint = uppercase hex of the first 20 bytes of
  `SHA-256(ed25519_pub ‖ x25519_pub)`, formatted in PGP-style groups.
- Canonical JSON (sorted by UTF-16 code units, floats rejected), SHA-256 event
  ids, deterministic golden vectors checked in tests
  (`crates/protocol/tests/fixtures.rs`, `crates/protocol/fixtures/*.json`).
- Carriers: plain or one-per-recipient encrypted, optional embedded contact card,
  routing metadata re-derived on receipt.
- Encryption: X25519 ephemeral-static, ChaCha20-Poly1305, `SMENC1` wire prefix.
- Encrypted identity backups: PBKDF2-HMAC-SHA256, 200k rounds.
- Contact cards: self-signed, `deny_unknown_fields`, mismatch detection on import.

### Application

- Contacts, trust (`unverified` → `verified`, `blocked`), mute, follow requests,
  approvals, declines, removals, unfollows.
- Posts and replies; replies to an unseen parent are kept and re-linked.
- Two-tier trust enforced at delivery: private posts are queued only for verified
  followers; unverified followers get `Withheld` with the reason `unverified`.
  Handshakes (follow, accept, remove) do reach unverified contacts — otherwise
  onboarding would be impossible.
- Public list posts are signed, never encrypted, and delivered in the clear to the
  list operator, card attached so the operator can verify the author.
- Reflector: approved-member database, strict verification, per-member encrypted
  fan-out preserving the author signature, retry and permanent-failure states,
  member removal effective immediately.
- Sticky timeline positions assigned once inside a transaction (never re-sorted),
  duplicate suppression by event id, quarantine with readable reasons, mute and
  block applied to every tab.

### Mail transport (v0.2)

- `crates/mail` implements SMTP submission (EHLO, STARTTLS, AUTH PLAIN/LOGIN,
  MAIL/RCPT/DATA, dot-stuffing) and IMAP (LOGIN, SELECT, UID SEARCH, UID FETCH with
  literals, COPY, STORE, EXPUNGE, CREATE) as small blocking clients.
- TLS through `rustls` with bundled Mozilla roots: identical on Windows and Linux,
  no OpenSSL, no way to disable verification, no silent STARTTLS downgrade.
- Carriers travel as `text/plain; charset=utf-8`, base64, subject
  `SocialMail/v1 carrier`, header `X-SocialMail: v1`.
- Processed mail is marked read, copied into `SocialMail/`, marked deleted and
  expunged; quarantined mail is left in place, flagged read, with the reason in
  Problems.
- Failures are classified — `auth`, `transient`, `permanent`, `too large` — and map
  onto the delivery states: auth pauses the queue and records a banner error,
  transient retries with backoff, permanent and oversized fail with a reason.
- The app password comes from `SOCIAL_MAIL_PASSWORD` (or `mail.password_env`). It is
  never written to the config file, the database, diagnostics or `Debug` output.
- `social-mail mail test [--send]`, `social-mail mail status`,
  `social-mail settings mail ...`, and `Diagnostics` report the effective settings.
- Fake in-process SMTP/IMAP servers cover send, fetch, acknowledge, quarantine and
  auth failure; no test touches the network.

### Android app (v0.3)

- `crates/android-jni` exports one function, `Core.call(request_json)`, and answers
  with `{"ok": true, "data": …}` or `{"ok": false, "error": …}`. All commands —
  status, create, feed, conversation, post, reply, follow, contacts, requests,
  problems, sync, queue, lists, settings, mail probe — go through it. Unit tests
  cover request parsing, the tab/audience mapping and TLS/port validation.
- `apps/android` is a Kotlin + Jetpack Compose app (minSdk 26, target 34,
  `tld.android.social`) with onboarding, feed, thread, compose, contacts, follow
  requests, problems and settings screens. It compiled and packaged
  (`assembleRelease`) in CI.
- The identity lives in app-private storage; `allowBackup=false` plus explicit
  data-extraction rules keep it out of cloud backup and device transfer.
- The mail app password is session-only: never in preferences, files, the database
  or logs.
- `release.yml` builds the Rust core with `cargo-ndk` for arm64-v8a, armeabi-v7a and
  x86_64, signs the APK when the `ANDROID_*` secrets exist, verifies the signature
  with `apksigner`, and publishes `social-mail-vX.Y.Z-android.apk` with a checksum.
  Without the secrets the artefact is named `…-unsigned.apk`.

### Surfaces

- `social-mail` CLI: `init`, `identity` (show/export-public/contact-card/
  export-backup/import-backup/revocation), `contacts` (import/list/show/verify/
  block/unblock/mute/unmute), `follow` (request/approve/decline/remove/unfollow/
  list/requests), `post`, `reply`, `timeline` (tab/thread/replies), `sync`,
  `queue` (status/retry/cancel), `lists` (add/remove/request-membership/list),
  `settings`, `problems`, `bundle` (export/import), `diagnostics`, `tui`.
  `--json` on every command; `--config` global; nonzero exit codes.
- TUI: four tabs with unseen counts, three densities remembered per tab, card feed
  with day dividers and status chips, conversation view, contacts, follow
  requests, queue, replies to you, problems, settings, composer, banner, status
  bar, help overlay, `NO_COLOR`-safe (words always carry the meaning).
- RTL: logical text → wrap by display width → bidi reorder per line, with Persian
  fixtures asserting nothing is lost, nothing exceeds the width, and identifiers
  stay readable.
- `social-mail-reflector`: `init`, `run` (`--once`, `--interval`), `list-members`,
  `approve-member`, `remove-member`, `block-member`, `show-queue`, `retry-failed`,
  `--json`.

### Engineering

- Workspace: `crates/crypto`, `crates/protocol`, `crates/storage`,
  `crates/engine`, `apps/cli`, `apps/social-mail-reflector`.
- 60+ tests: golden vectors, tamper rejection, storage invariants, engine flows,
  reflector relay end to end, bidi fixtures.
- `.github/workflows/release.yml`: Windows + Linux archives with SHA-256 checksums
  and a release job that publishes them for `v*` tags.

## Not done (deferred, tracked in `TODO.md`)

| Item | Why it is not here yet |
|---|---|
| OAuth2 / XOAUTH2 | App passwords only in v0.2; provider client registration belongs in its own decision record. |
| Android identity restore/export | The phone creates a new identity only; backup import/export is the next piece of work. |
| Android background sync, share targets, Play Store listing | Deferred; the app syncs on demand and ships as an APK. |
| IDLE / connection pooling | One connection per operation is simpler to reason about; queues are small. |
| OS keyring storage | Environment variable is documented; keyring is deferred. |
| Attachments and large media | Out of scope; the 2 MB ceiling quarantines oversized messages. |
| OpenPGP wire compatibility | Deliberately replaced by Ed25519+X25519 for v0.1 with PGP-shaped fingerprints. See `docs/decisions/0002-crypto-backend.md`. |
| Public-list ingestion at scale | Reading list mail from a mailbox needs IMAP. |
| Persian glyph shaping | Terminals that do not shape Arabic script still show disconnected letters; documented in `TODO.md`. |
| Attachments, reactions, edits, search, multi-device | Out of v0.1 scope. |

## Continuous integration

Branch CI was a development gate for this phase: it ran `cargo fmt`, `check`,
`test` and `clippy` on Linux and Windows and committed a readable report to the
branch. It passed on both platforms (sha `e4bf421`) and was then removed, so the
only workflow left is `release.yml`. Releases run the full test suite before they
package anything.

## How to verify this claim

```bash
cargo test --workspace           # protocol vectors, storage, engine, relay, bidi
cargo clippy --workspace --all-targets
cargo build --release --bins     # social-mail, social-mail-reflector
```

Then run the two-machine demo against one shared spool directory:

```bash
social-mail --config a.toml init --name Alice --email alice@example.org
social-mail --config b.toml init --name Bob   --email bob@example.org
# exchange cards, follow, approve, post --public-list, reflector run --once
```

## Next milestone (in order)

1. Android identity restore and export, so one identity works on desktop and phone.
2. OAuth2 (XOAUTH2) sign-in for Gmail and Outlook, as its own decision record.
2. Session reuse and IDLE so a long-running client does not reopen per operation.
3. Public-list reading from a mailbox and reflector parity for public lists.
4. Scripted J0–J18 acceptance transcripts against a fake mailbox.
5. Attachments with the documented size policy.
