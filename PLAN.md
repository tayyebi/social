# Implementation Brief: `social-mail`

## 1. Objective

Build **v0.1 of a decentralized, email-based social application**, implemented entirely in **Rust**.

The application should:

- Use OpenPGP fingerprints as user identities.
- Sign every application event.
- Encrypt private posts and subscription messages.
- Transport events through ordinary email.
- Synchronize using IMAP and send using SMTP.
- Build timelines locally.
- Support direct follower delivery and mailing lists.
- Provide CLI commands and an interactive TUI enabled with `--tui`.
- Correctly display Persian text and mixed Persian/English RTL content in the TUI.
- Require no application-operated central server.

Email providers still provide transport and storage. “Decentralized” means that no application service owns identities, subscriptions, timelines, or message verification.

This phase is a working reference implementation, not a production-ready anonymity system.

This document specifies the machine. `docs/user-journey.md` specifies the human side of the same binary — personas, first-run setup, trust ceremonies, screen flows, copy, failure states, and closed product decisions (D1–D28). It is part of the v0.1 baseline: where it disagrees about *what the software must do*, this document wins; where it disagrees about *how the product feels and in what order the user is asked for things*, it wins. See section 24.

---

## 2. Deliverables

The agent must produce:

1. A compilable Rust workspace.
2. A CLI/TUI binary named `social-mail`.
3. A shared Rust application engine used by both CLI and TUI.
4. OpenPGP identity generation, import, backup, and verification.
5. Direct-email and mailing-list workflows.
6. IMAP synchronization and SMTP sending.
7. SQLite-backed local storage.
8. Delta Chat-style protocol-mail folder handling.
9. Persian/RTL TUI support with automated tests.
10. Unit, integration, and malformed-input tests.
11. `README.md` with build, configuration, and usage instructions.
12. `TODO.md` containing deferred native, browser, and protocol work.
13. `.github/workflows/release.yml` that builds and uploads artifacts when a release is published.

The agent should implement this incrementally, committing after each milestone. It must not begin native GUI or browser applications.

---

## 3. Scope for v0.1

### Included

- One local profile with one identity and one email account.
- Additional identities and mail accounts may be supported internally, but multi-account UX is not required.
- Identity generation and OpenPGP certificate import.
- Contact import and explicit fingerprint confirmation.
- Signed profile events.
- Signed follow requests, approvals, and removals.
- Encrypted private posts and replies.
- Signed public posts for compatible public mailing lists.
- A local chronological timeline.
- Duplicate suppression and out-of-order reply handling.
- Persistent outgoing queues and retries.
- Local block and mute controls.
- Dedicated protocol-mail folder support.
- Headless mailing-list reflector commands.
- CLI and TUI interfaces.
- English and Persian UI text, with mixed-direction text support.
- Configuration, structured logging, and diagnostics that do not expose secrets.

### Explicitly deferred

- Native Android, Apple, Windows, or graphical Linux applications.
- Browser or WebAssembly clients.
- Global discovery and search.
- Recommendations and algorithmic feeds.
- Reactions, reposts, edits, and rich media.
- Large-attachment distribution.
- Automatic multi-device identity synchronization.
- Anonymous transport or metadata hiding.
- Guaranteed deletion or message recall.
- Celebrity-scale follower fan-out.
- Production security certification.

---

## 4. Repository Layout

Use this structure unless a concrete implementation constraint requires a small adjustment:

```text
/
├── Cargo.toml
├── rust-toolchain.toml
├── README.md
├── TODO.md
├── LICENSE
├── .github/
│   └── workflows/
│       └── release.yml
├── crates/
│   ├── protocol/
│   ├── crypto/
│   ├── storage/
│   ├── mail-format/
│   ├── mail-transport/
│   ├── engine/
│   └── test-support/
├── apps/
│   ├── cli/
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/
│   │       └── tui/
│   └── list-reflector/
│       └── src/
│           └── main.rs
├── protocol/
│   ├── schemas/
│   └── fixtures/
├── tests/
│   ├── fixtures/
│   └── integration/
└── docs/
    ├── architecture.md
    ├── protocol.md
    ├── security.md
    ├── user-journey.md
    └── decisions/
```

Keep binary names explicit in workspace configuration:

- `social-mail`: CLI and TUI.
- `social-mail-reflector`: headless mailing-list reflector.

The main package must have `src/main.rs`; do not create an ambiguous package containing both `src/main.rs` and `src/lib.rs` without clear responsibilities.

---

## 5. Technology Choices

Use maintained, compatible versions and document all selections.

| Area | Preferred implementation |
|---|---|
| Language | Rust, stable toolchain |
| Async runtime | Tokio |
| CLI | `clap` derive |
| TUI | `ratatui` and `crossterm` |
| Bidirectional text | `unicode-bidi` |
| Persian/Arabic shaping | `rustybuzz` or another maintained shaping library evaluated during implementation |
| Text width | `unicode-width`, used after terminal presentation is decided |
| Serialization | `serde` and `serde_json` |
| Canonical JSON | RFC 8785-compatible implementation |
| Hashing | SHA-256 |
| OpenPGP | Prefer `rpgp`; evaluate Sequoia if necessary |
| SQLite | `sqlx` or `rusqlite`, with migrations |
| IMAP | `async-imap` or a maintained equivalent |
| SMTP | `lettre` or a maintained equivalent |
| MIME | `mail-builder`, `mail-parser`, or equivalents |
| Logging | `tracing` |
| Errors | `thiserror` in libraries, `anyhow` in binaries |
| Testing | Rust test framework, `tempfile`, fixtures, and local fake services where practical |

### Cryptography decision gate

Before building the full protocol, demonstrate:

- Identity generation.
- Export and import of public certificates.
- Detached signing and verification.
- Encryption and decryption.
- Revocation certificate generation.
- Signing-subkey handling.
- Cross-language interoperability using GnuPG where available.

Do not implement cryptographic primitives or OpenPGP packet handling manually.

Record the selected library and any limitations in `docs/decisions/`.

---

## 6. Core Architecture

```text
CLI ───────────────┐
                   │
TUI ───────────────┼──► Engine
                   │       ├── Protocol validation
Reflector ─────────┘       ├── OpenPGP operations
                           ├── Subscription policy
                           ├── Timeline construction
                           └── Outgoing queue
                                    │
                    ┌───────────────┴───────────────┐
                    │                               │
              Storage adapter                Mail adapter
              SQLite                         MIME / IMAP / SMTP
```

The engine must not depend on Ratatui, Crossterm, terminal dimensions, or CLI argument parsing.

The CLI and TUI must call the same engine operations. They must not independently implement signature verification, subscription decisions, or timeline assembly.

---

## 7. Identity and Contact Model

### Identity

Use the full OpenPGP primary-key fingerprint as the canonical identity.

```text
Identity
  fingerprint
  public_certificate
  private_key_material, when locally owned
  display_profile
  delivery_routes
  revocation_state
```

Requirements:

- Never use short key IDs as identities.
- Email addresses and display names are mutable attributes.
- Display the full fingerprint during verification.
- Provide a grouped, human-readable fingerprint format for copying and comparison.
- Verify that signing subkeys are validly bound to the claimed primary key.
- Reject expired, revoked, or otherwise invalid signing keys according to documented policy.
- Do not silently replace a contact’s key.
- Generate a revocation certificate during identity creation.
- Support passphrase-protected secret-key export and import.

If secret-key passphrase handling cannot be implemented safely in v0.1, require protected exported backups and document the local-storage limitation rather than pretending the profile is encrypted.

### Contacts

Support:

- Import from an ASCII-armored public certificate or file.
- Optional initial contact bundle containing a certificate and signed delivery route.
- Explicit trust states: `unverified`, `verified`, `blocked`.
- Local fingerprint confirmation.
- Multiple known delivery routes, with one active route initially.

A valid signature proves control of a key; it does not prove a real-world name.

---

## 8. Protocol

Create a versioned specification in `docs/protocol.md` and stable JSON fixtures in `protocol/fixtures/`.

### Initial event types

```text
profile
follow-request
follow-accept
follow-remove
post
reply
route-update
```

Do not implement key migration in v0.1; reserve it in `TODO.md`.

### Event envelope

Each event must contain at least:

```json
{
  "protocol": "social-mail",
  "version": 1,
  "type": "post",
  "author": "FULL_PRIMARY_KEY_FINGERPRINT",
  "nonce": "CRYPTOGRAPHICALLY_RANDOM_UNIQUE_VALUE",
  "created_at": "RFC3339_TIMESTAMP",
  "payload": {}
}
```

Define strict schemas for every event type.

### Canonical event ID

```text
canonical_bytes = RFC8785_canonicalize(unsigned_event)
event_id        = lowercase_hex(SHA256(canonical_bytes))
signature       = OpenPGP_detached_signature(canonical_bytes)
```

The event ID must not be included in the bytes being hashed.

The transport object must carry:

- Canonical event bytes or their exact JSON representation.
- Event ID.
- Signature.
- Author certificate when needed for initial exchange.
- Protocol version.

Recipients must:

1. Parse within size and nesting limits.
2. Canonicalize independently.
3. Recompute the event ID.
4. Verify the OpenPGP signature.
5. Confirm that the signing key belongs to the declared author.
6. Validate event-specific authorization.
7. Apply local block, mute, and subscription policies.
8. Store the event idempotently.

An SMTP sender address, `From` header, or `Message-ID` never establishes event authorship.

### Posts and replies

A post payload should support:

```text
text
optional language tag
optional audience
optional list identifier
```

A reply references the signed application `event_id` of its parent.

Email `In-Reply-To` and `References` headers are transport hints only.

If a reply arrives before its parent:

- Store the verified reply.
- Mark its parent as missing.
- Display it as an orphaned reply or pending item.
- Resolve it automatically when the parent arrives.

`created_at` is an author claim. Give every stored event an **immutable, monotonically increasing local feed position**, assigned at commit time inside the same transaction as the sync checkpoint. Within one commit batch, order by the claimed time clamped to `[commit_time - 30 days, commit_time]`; after commit, never re-sort and never move an event. Display author time separately and label arrivals that are far older than the newest committed item. Read state never changes position. Do not let an arbitrary timestamp reorder local storage destructively, and do not let a background sync rearrange the screen a user is reading (see `docs/user-journey.md` D32).

### Invalid events

Invalid events must not:

- Change contacts.
- Change subscriptions.
- Replace delivery routes.
- Alter trust state.
- Enter the normal timeline.

Store bounded diagnostic metadata where useful, but do not repeatedly process known failures.

---

## 9. Signing, Encryption, and MIME

Use this order:

```text
canonical event
    → detached OpenPGP signature
    → application payload containing event and signature
    → encryption for private recipients
    → MIME email
```

Private payloads must be encrypted before SMTP submission.

Use standard OpenPGP/MIME conventions where practical. Include:

- A harmless generic subject.
- A short human-readable fallback body where appropriate.
- An application payload with a versioned media type.
- A machine-readable header such as `X-Social-Mail-Protocol: 1`.
- Unique transport `Message-ID` values.

Do not place post content, follower lists, or sensitive profile data in plaintext headers.

Public mailing-list posts may be signed but unencrypted. This exception must be explicit in the composer and reflected in delivery previews.

Private posts must not be sent through a public list or legacy mailing-list server that cannot preserve the required confidentiality model.

---

## 10. Direct Subscription Workflow

Implement:

```text
Alice                             Bob
  │                                │
  ├── signed follow-request ──────►│
  │                                ├── user reviews fingerprint
  │◄──── signed follow-accept ─────┤
  │                                │
  │                     Bob publishes private post
  │                                │
  │◄── individually encrypted mail ┤
```

Requirements:

- Bob’s approved-follower list is local and authoritative for his client.
- A follow approval is not inferred merely because a request arrived.
- Two-tier trust (`docs/user-journey.md` D9): a follower who is approved without fingerprint verification is accepted into an **unverified tier** — readable and followable, but excluded from every private post. Private posts may only be delivered to followers whose key the user has verified. Seed deliveries are never backfilled after later verification.
- Requests from keys with no available certificate must be resolved by explicitly importing the contact first; approval never creates trust from an unverified claim.
- Support follow removal and blocking.
- Prevent delivery to blocked or removed followers.
- Previously delivered content cannot be recalled.

For private direct delivery, create an independently encrypted message per follower by default. This avoids exposing the entire recipient set through a shared encrypted payload.

Warn users that outbound traffic scales with follower count and remains subject to provider quotas.

---

## 11. Mailing Lists in This Phase

Implement two distinct modes.

### A. Public standard-list mode

For an existing public list using Mailman, mlmmj, or similar software:

- Configure the list address and expected `List-ID`.
- Send signed, unencrypted public posts to the list address.
- Do not encrypt posts to the list address unless the entire list architecture explicitly supports it.
- Match incoming messages by both protocol payload and configured list identity.
- Verify the original author’s signature.
- Do not trust `List-ID` alone.
- Ignore or quarantine list messages with invalid signatures.
- Display the author, list, and verification state in the timeline.

Document that list operators and subscribers can read public posts.

### B. Headless private-list reflector

Provide `social-mail-reflector` for a user-controlled list mailbox.

The reflector must:

1. Connect to its own IMAP/SMTP account.
2. Maintain a local approved-member database.
3. Process signed subscribe and unsubscribe requests.
4. Require administrative approval unless configured otherwise.
5. Verify every incoming post before redistribution.
6. Apply sender authorization and local moderation rules.
7. Enqueue delivery independently for each approved recipient.
8. Encrypt each private copy to that recipient.
9. Preserve the original signed event for end-to-end verification.
10. Never ask for members’ private keys.
11. Avoid forging an author signature or replacing it with a list signature.
12. Retry temporary failures and surface permanent failures.

A reflector can read private list traffic while processing it. It is optional user-controlled infrastructure, not a required central service.

Include reflector administration commands for:

```text
initialize
run
list-members
approve-member
remove-member
block-member
show-queue
retry-failed
```

---

## 12. IMAP Folder Behavior

Implement Delta Chat-style mailbox organization.

### Configuration

```toml
[mail]
move_protocol_messages = true
protocol_folder = "SocialMail"
```

Expose the setting in both CLI configuration and the TUI as:

```text
[x] Move SocialMail messages out of INBOX
```

### When checked

This is the default.

On startup and synchronization:

1. Discover the IMAP folder delimiter and capabilities.
2. Locate `SocialMail`.
3. Create and subscribe to it if missing.
4. Scan relevant new messages in `INBOX`.
5. Identify protocol messages conservatively.
6. Parse and store them before destructive mailbox operations where practical.
7. Move them to `SocialMail` using `UID MOVE` when supported.
8. Otherwise use copy, mark-deleted, and expunge carefully.
9. Continue subsequent protocol synchronization in `SocialMail`.

Handle the move operation idempotently so a crash does not create duplicate timeline entries.

### When unchecked

```toml
move_protocol_messages = false
```

- Leave protocol messages in `INBOX`.
- Do not move, delete, or expunge them.
- Process them in place.
- Use stored IMAP UID validity/UID state and local message IDs to avoid reprocessing.
- Do not assume that custom IMAP keywords are supported.

### Safety requirements

- Never move all INBOX mail indiscriminately.
- Do not classify a message solely by subject.
- Require a recognized protocol header plus a plausible payload, or another conservative rule documented in the code.
- Quarantine malformed messages instead of repeatedly moving them.
- Do not silently disable the folder setting after an IMAP error.
- Display failures in the UI and logs without exposing credentials or message bodies.
- Test servers both with and without `UID MOVE`.

The folder option controls mailbox organization only; it must not change event verification or local timeline behavior.

---

## 13. Mail Synchronization and Sending

### IMAP

Implement:

- TLS with certificate validation.
- Configurable host, port, and authentication settings.
- Capability detection.
- Incremental synchronization.
- UID validity handling.
- Reconnection and bounded backoff.
- Sync checkpoints saved transactionally.
- Limits on message size and processing work per cycle.

### SMTP

Implement:

- Authenticated submission.
- TLS according to provider configuration.
- Persistent outgoing queue.
- Per-recipient delivery records.
- Retry with exponential backoff and jitter.
- Clear permanent-failure reporting.
- Recovery after application restart.

Delivery states:

```text
queued
submitting
accepted-by-smtp-server
retrying
failed
cancelled
```

Never label SMTP acceptance as “read” or guaranteed delivery.

### Credentials

Support environment variables and interactive hidden input for initial development.

Do not:

- Commit credentials.
- Write passwords or private keys to logs.
- Place secrets in CLI arguments where they may appear in process listings.
- Include secrets in debug dumps.

A production keyring integration may be deferred, but the security limitation must be documented.

---

## 14. Local Storage

Use SQLite with migrations.

Suggested tables:

```text
profiles
identities
contacts
delivery_routes
events
event_payloads
subscriptions
timeline_entries
mail_messages
sync_state
outgoing_messages
outgoing_deliveries
processing_errors
settings
```

Processing pipeline:

```text
received
  → parsed
  → signature verified
  → authorization checked
  → local policy applied
  → indexed
```

Requirements:

- Event IDs have uniqueness constraints.
- Every timeline entry stores an immutable feed position (monotonic per profile), and per-filter seen cursors are stored separately from entries.
- Transport messages and application events are separate concepts.
- Duplicate deliveries do not duplicate timeline entries.
- Raw payloads and verification status remain available for diagnostics.
- Database writes and outgoing-state transitions are transactional.
- Restarting during sync or sending is safe.
- Block/mute state is local and does not invalidate cryptographic verification results.

---

## 15. CLI Interface

Implement at least:

```text
social-mail --config <PATH> <COMMAND>
social-mail --tui
social-mail tui

social-mail init
social-mail identity show
social-mail identity export-public
social-mail identity export-backup
social-mail identity import-backup

social-mail contacts import <FILE>
social-mail contacts list
social-mail contacts verify <FINGERPRINT>
social-mail contacts block <FINGERPRINT>

social-mail follow request <ADDRESS>
social-mail follow approve <FINGERPRINT>
social-mail follow remove <FINGERPRINT>

social-mail post --text <TEXT> [--private|--public-list <LIST>]
social-mail reply <EVENT_ID> --text <TEXT>
social-mail timeline [--limit <N>] [--json]
social-mail sync
social-mail queue status
social-mail queue retry

social-mail lists add <ADDRESS> --list-id <LIST_ID>
social-mail lists list
social-mail settings show
social-mail settings set-mail-folder <true|false>
```

Additional commands required by the user-journey baseline (`docs/user-journey.md`):

```text
social-mail init [--email <ADDR>] [--name <NAME>] [--locale <en|fa>] [--offline] [--no-auto-sync]
social-mail identity contact-card [--out <FILE>]
social-mail lists request-membership <LIST_ADDRESS>
```

`--tui` and `tui` may both launch the same interface, but only one needs to be the documented primary form.

All machine-oriented commands should support `--json` where practical.

Errors must produce nonzero exit codes and actionable messages.

---

## 16. TUI Requirements

Launch with:

```bash
social-mail --tui
```

### Screens

- Startup/profile unlock.
- Timeline.
- Post and reply composer.
- Contacts and fingerprint verification.
- Follow requests.
- Mailing lists.
- Outgoing queue and synchronization status.
- Settings, including the mail-folder checkbox.
- Help and key bindings.

### Interaction

- Vim-style and arrow-key navigation where practical.
- `q` exits or returns according to context.
- `?` opens help.
- `r` synchronizes.
- `n` opens a new-post composer.
- Enter opens or confirms the focused item.
- Escape cancels the current modal operation.
- Explicit confirmation is required for key import, fingerprint verification, blocking, and public posting.

### Timeline display

Show:

- Display name when available.
- Short fingerprint only as a display convenience, never as the sole verification mechanism.
- Signature and trust status.
- Local receipt time and author time where useful.
- Public/private/list audience.
- Delivery state for outgoing items.
- Persian and English content without corrupting adjacent lines.

The TUI must remain usable at narrow terminal sizes and must handle resize events.

---

## 17. Persian and RTL Support

This is a required feature, not a cosmetic enhancement.

Support:

- Persian UI labels.
- Persian post bodies.
- Persian and English mixed in the same paragraph.
- Persian digits and Latin digits.
- Right-to-left and left-to-right alignment as appropriate.
- Multiline posts.
- Composer input and editing.

### Rendering pipeline

Use a dedicated terminal-text module:

```text
logical Unicode text
  → paragraph and line segmentation
  → script-aware shaping where required
  → Unicode bidirectional analysis
  → terminal visual ordering
  → display-width calculation
  → clipping and alignment
  → Ratatui rendering
```

Do not shape, reverse, or truncate text independently in unrelated widgets.

### Implementation cautions

- `unicode-bidi` solves ordering, not necessarily Persian glyph shaping.
- Simple string reversal is not an acceptable BiDi implementation.
- Byte slicing must never split UTF-8 code points.
- Grapheme clusters, combining marks, emoji, and zero-width characters must not cause panics.
- Width must be calculated for the text actually sent to the terminal.
- Persian/Arabic presentation forms may be necessary for some terminal rendering paths; document the trade-off.
- LTR metadata, fingerprints, URLs, and event IDs should remain readable inside RTL paragraphs.
- Keep original logical text in storage; transformed terminal text is presentation only.

### Input caution

Bidirectional editing in a terminal is substantially harder than static rendering.

If fully correct visual RTL cursor editing cannot be delivered reliably:

- Provide correct RTL display in the timeline and previews.
- Use a documented logical-order composer editing mode.
- Do not claim full visual RTL editing support.
- Add visual RTL cursor editing to `TODO.md`.

### Required test fixtures

Include at least:

```text
سلام دنیا
سلام، این یک آزمایش است.
Hello سلام world
این پیام از social-mail است.
شناسه: ABCD1234
متن فارسی، با نیم‌فاصله و اعداد ۱۲۳۴۵.
```

Also test:

- Empty strings.
- Very long unbroken text.
- Combining marks.
- Newlines.
- Zero-width joiner and zero-width non-joiner.
- Mixed RTL/LTR punctuation.
- Narrow and wide terminal widths.

Add fixture-based tests and document any terminals known to render incorrectly.

---

## 18. Security Requirements

Before calling v0.1 complete, document the threat model and enforce:

- Signature verification before state mutation.
- Strict event authorization.
- No remote image loading or HTML execution.
- Plaintext-safe timeline rendering.
- Message, event, payload, and decompression limits.
- No secret values in logs.
- No credentials in test fixtures.
- Rejection of revoked or invalid keys according to policy.
- Explicit public-post confirmation.
- Encrypted private content over the application payload.
- Safe behavior on malformed MIME and JSON.
- Idempotent processing after crashes.
- Backup restoration testing.

Add parser fuzz targets for:

- MIME input.
- Event envelopes.
- Canonical JSON.
- Terminal text formatting where feasible.

### Documented non-goals

The application does not inherently hide:

- Sender and recipient email metadata.
- Communication timing.
- Approximate message sizes.
- Relationships visible to email providers.
- Content from intended recipients.
- Data on a compromised unlocked device.

Encryption does not prevent recipients from copying content. Decentralization does not prevent providers from blocking email.

---

## 19. Testing and Acceptance Criteria

### Unit tests

Cover:

- Canonicalization.
- Event ID generation.
- Signature verification.
- Wrong-author and wrong-key rejection.
- Revoked-key handling.
- Duplicate events.
- Out-of-order replies.
- Follow authorization.
- Block/mute policy.
- MIME generation and parsing.
- SQLite migrations.
- Outgoing queue recovery.
- IMAP folder classification logic.
- Persian and mixed-direction TUI formatting.

### Integration tests

Use local fake or containerized services where feasible.

Do not require real user accounts in CI.

Test:

1. Two local identities exchanging signed events through files.
2. Two profiles exchanging private posts through test IMAP/SMTP.
3. A three-profile follow and private-post workflow.
4. Duplicate delivery producing one timeline item.
5. Reply-before-parent resolution.
6. Restart with a partially completed outgoing queue.
7. Folder-enabled sync moving only protocol mail.
8. Folder-disabled sync leaving protocol mail in INBOX.
9. IMAP servers with and without `UID MOVE`.
10. Public-list ingestion rejecting forged content.
11. Reflector redistribution preserving the original author signature.
12. Persian text remaining valid in storage and predictable in TUI snapshots.

### v0.1 acceptance demonstration

A clean checkout must support the following demonstration:

1. Build with `cargo build`.
2. Run tests with `cargo test --workspace`.
3. Create Alice and Bob profiles.
4. Export and import their public certificates.
5. Verify fingerprints.
6. Send and approve a follow request.
7. Publish an encrypted post from Bob.
8. Synchronize Alice and display the post in CLI and `--tui`.
9. Reply from Alice and resolve the thread on Bob’s client.
10. Repeat with the protocol folder enabled and disabled.
11. Display a Persian post with mixed English text.
12. Publish a signed public post through a test list.
13. Run the reflector and deliver a private list post to two members.
14. Restart both applications without duplicate posts or lost queued deliveries.

---

## 20. Build Milestones

### Milestone 1: Foundation

- Initialize the workspace.
- Add formatting, linting, logging, configuration, and CI-friendly tests.
- Write architecture and threat-model drafts.
- Prove the selected OpenPGP library supports required operations.
- Establish protocol fixtures.

### Milestone 2: Offline protocol

- Implement identities and contacts.
- Implement event canonicalization, IDs, signatures, and encryption.
- Implement SQLite storage.
- Exchange events using files.
- Build a basic CLI timeline.

**Gate:** Two profiles can exchange private posts and replies without email.

### Milestone 3: Email transport

- Implement MIME packaging.
- Implement SMTP and IMAP.
- Add persistent queues and sync checkpoints.
- Add direct follow workflows.
- Implement folder behavior.

**Gate:** Two test mailboxes can follow, post, reply, restart, and recover safely.

### Milestone 4: Mailing lists

- Add public-list configuration and ingestion.
- Add signed public-list posting.
- Implement the headless reflector and member administration.

**Gate:** Public-list signatures and private reflector delivery pass integration tests.

### Milestone 5: TUI

- Build screens using the shared engine.
- Add settings, queue status, contacts, lists, and composer.
- Add Persian UI strings and mixed-direction post rendering.
- Add terminal text tests.

**Gate:** The complete acceptance demonstration works in `--tui`.

### Milestone 6: Hardening and release

- Run formatting, Clippy, tests, and fuzz smoke tests.
- Complete README and security documentation.
- Test clean builds on all release targets.
- Verify release artifacts.
- Clearly label remaining limitations.

---

## 21. GitHub Actions

Create `.github/workflows/release.yml`.

### Trigger

The workflow must run when a GitHub release is published:

```yaml
on:
  release:
    types: [published]
```

Do not trigger artifact publication on every push or pull request.

A separate optional CI workflow may run tests on pushes and pull requests.

### Release matrix

Build:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Package and upload:

- `social-mail`
- `social-mail-reflector`
- License and basic usage documentation where practical.

Use archives named with the project version and target, for example:

```text
social-mail-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
social-mail-v0.1.0-x86_64-pc-windows-msvc.zip
```

### Workflow requirements

- Use `actions/checkout`.
- Use a pinned stable Rust setup action.
- Install targets explicitly.
- Cache Cargo dependencies without caching secrets.
- Run tests before packaging where practical.
- Package from `target/<target>/release/`.
- Preserve `.exe` on Windows.
- Upload only the archives for the current matrix job.
- Grant `contents: write` to the release job.
- Do not print credentials or use custom secrets for ordinary public releases.
- If Linux cross-linking is unreliable, use a supported cross-compilation tool or build Linux ARM on an appropriate runner rather than silently uploading the wrong architecture.

Also add a basic CI workflow for:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

---

## 22. Required `TODO.md`

Create `TODO.md` in the project root. Do not implement these items during this phase.

```markdown
# Deferred Roadmap

This file records features intentionally excluded from the Rust CLI/TUI v0.1.
The current implementation must not add native GUI, browser, or alternative
transport clients before the core email workflows are stable.

## Shared-core preparation

- [ ] Keep protocol, verification, trust, and timeline logic independent of UI.
- [ ] Define a stable, minimal application-engine API.
- [ ] Evaluate UniFFI for Kotlin and Swift bindings.
- [ ] Evaluate a C ABI for platforms without UniFFI support.
- [ ] Evaluate wasm-bindgen for browser-compatible protocol modules.
- [ ] Avoid exposing private keys or mailbox credentials unnecessarily across bindings.

## Android

- [ ] Add JNI or UniFFI bindings.
- [ ] Build a native Java or Kotlin interface without React.
- [ ] Integrate Android Keystore and encrypted credential storage.
- [ ] Add WorkManager-based synchronization within platform restrictions.
- [ ] Test battery use, offline composition, and queue recovery.

## Apple platforms

- [ ] Generate Swift bindings.
- [ ] Build SwiftUI interfaces for iOS and macOS.
- [ ] Integrate Apple Keychain.
- [ ] Add background synchronization using supported system tasks.

## Native desktop graphical clients

- [ ] Evaluate GTK for Linux.
- [ ] Evaluate WinUI and C ABI/P/Invoke for Windows.
- [ ] Evaluate a native macOS application.
- [ ] Keep graphical clients thin; do not duplicate protocol logic.
- [ ] Do not introduce Electron or React by default.

## Browser

- [ ] Document that browsers cannot directly open arbitrary IMAP/SMTP TCP connections.
- [ ] Design an authenticated local companion agent.
- [ ] Restrict the agent to loopback by default.
- [ ] Add pairing, strict origin checks, and CSRF protections.
- [ ] Evaluate provider-specific HTTPS APIs for standalone browser access.
- [ ] Evaluate IndexedDB-backed encrypted storage.
- [ ] Document risks from web-origin-delivered application code accessing unlocked keys.

## Protocol and identity

- [ ] Specify key migration signed by old and new identities.
- [ ] Add revocation propagation and Web Key Directory discovery.
- [ ] Design interoperable contact and key-discovery mechanisms.
- [ ] Add signed reactions and reposts.
- [ ] Specify edit and deletion tombstones without claiming guaranteed erasure.
- [ ] Add attachments and encrypted large-file distribution.
- [ ] Design multi-device identity and subscription synchronization.
- [ ] Add optional social recovery without weakening fingerprint-based identity.

## Networking and discovery

- [ ] Evaluate transport adapters beyond email.
- [ ] Preserve the same signed-event semantics across transports.
- [ ] Explore public signed discoverability without requiring a central directory.
- [ ] Investigate spam-resistant web-of-trust discovery.
- [ ] Measure and improve large-follower fan-out.

## Privacy and security

- [ ] Commission an independent security review.
- [ ] Expand continuous parser fuzzing.
- [ ] Improve metadata-protection documentation and optional transport research.
- [ ] Add reproducible release builds.
- [ ] Add hardware-backed or external OpenPGP key support where feasible.
- [ ] Implement production-grade OS keyring integration on supported platforms.

## Terminal interface

- [ ] Improve visual RTL cursor movement and selection in the composer.
- [ ] Test additional Persian/Arabic terminals and fonts.
- [ ] Add localization infrastructure beyond English and Persian.
- [ ] Improve accessibility and terminal-width behavior.
```

---

## 23. Agent Execution Rules

The implementing agent must:

1. Treat this document as the v0.1 requirements baseline.
2. Ask for clarification only when a requirement cannot be implemented safely; otherwise choose conventional defaults and document them.
3. Prefer a small working vertical slice over numerous unfinished modules.
4. Never claim encryption, verification, RTL editing, or delivery guarantees that tests do not support.
5. Never generate or commit real credentials, private keys, or personal email fixtures.
6. Keep all protocol state transitions deterministic and covered by fixtures.
7. Run formatting, linting, and tests before reporting completion.
8. Update `README.md`, `docs/protocol.md`, `docs/user-journey.md`, and `TODO.md` whenever behavior changes.
9. Mark unfinished functionality explicitly rather than leaving silently broken commands.
10. Finish with a report containing:
    - Implemented features.
    - Commands to build and run.
    - Tests executed and their results.
    - Security and interoperability limitations.
    - Known terminal/RTL limitations.
    - Any deviations from this plan.

The first implementation target remains:

> Two Rust profiles exchanging a verified, encrypted signed post through real or locally emulated email, with a persistent local timeline.

Everything else should build on that working foundation.

---

## 24. User Experience Baseline

The full plan lives in `docs/user-journey.md`. The non-negotiable parts that
constrain every implementation task:

1. **Two equal surfaces.** Every user action exists both in the TUI and as a CLI
   command; parity is a test (`docs/user-journey.md` §19), and neither surface may
   be the only way to do something. `social-mail init` is the single entry point
   and is resumable and fully flag-drivable.
2. **First launch is always the guided wizard.** No implicit profile creation.
   Local (file-based) mode exists as an advanced `init --offline` path for
   credential-free demos and CI, not as an onboarding choice.
3. **Identity before mailbox**, and a failed SMTP/IMAP test never discards the
   identity or restarts setup.
4. **Two-tier trust.** Verification is still a two-step ceremony with the full
   grouped fingerprint on screen, but reading is permissive: unverified contacts
   can be imported, followed, and read (permanently tagged `unverified`), while
   **private posts are delivered only to verified followers**. Withheld
   recipients are shown as `Not sent — unverified`, never as queued or failed.
5. **The timeline is a feed of cards** (header: time · author · short fingerprint
   · trust · audience; body; footer), grouped by day.
6. **Color carries status, words carry meaning.** Trust and delivery states are
   color-coded and always also rendered as words; `NO_COLOR` and
   `settings.color = never` lose no information.
7. **Private is the default audience**, and every card and queue row states its
   audience and trust status.
8. **Delivery language never exceeds SMTP truth**: queued, sending, accepted by
   your mail server, retrying, failed, cancelled, withheld. Never "delivered",
   never "read".
9. **Key changes are never silent.** Mismatched or invalid signatures are
   quarantined into a visible `Problems` list and never enter the timeline.
10. **Every error names an action.** No dead-end screens and no unusable app when
    the network or mailbox is broken.
11. **Block and mute are local and silent**; no block or mute action sends mail.
12. **Nothing is lost to a crash**: drafts survive `Esc`, outgoing state survives
    kill, and restarting never duplicates a timeline entry.
13. **The feed is sticky and append-only.** Items get their position once, at
    commit; nothing is re-sorted, and syncing never rearranges what is on screen.
    Late arrivals are labelled `late`, not relocated.
14. **The feed is roots-only**, with replies in conversations and a visible
    `N new replies` counter plus a Replies screen so "someone answered you" is
    never invisible. List traffic is never merged into the Following tab.
15. **Density is per source and chrome is hybrid**: compact rows for lists, cards
    for people, borders only when the terminal has room (≥60 columns, not
    compact), and focus is never rendered as a status color.
16. **Persian is a first-class locale**: UI language is chosen at first run with
    a live preview, identifiers stay Latin and LTR inside RTL text, and RTL
    correctness is proven by snapshot tests, not asserted.
17. **Journeys J0–J18 in section 22 of `docs/user-journey.md` are acceptance
    tests**, not documentation. A journey without a test is unfinished work, and
    the release report must show which journeys pass. The acceptance demo must
    pass both interactively and as a scripted CLI transcript.

---

## 25. Implementation status (2026-11-05)

This section records where the code stands against the requirements above. It is
not a requirement; `docs/IMPLEMENTATION-STATUS.md` is the maintained version of
this table. Requirements above stay unchanged until a decision record says
otherwise.

### Shipped in v0.2 (Windows + Linux release)

- §9 MIME: one carrier per message, base64 body, `SocialMail/v1 carrier` subject.
- §12 folder behaviour: processed mail moves to `SocialMail/` (copy, delete, expunge);
  quarantined mail stays put, flagged read.
- §13 IMAP receive and SMTP send with app passwords, TLS via `rustls`, and failure
  classification that drives the existing delivery state machine.

### Shipped in v0.1 (release `v0.1.0`, Windows + Linux)

| Requirement | Where |
|---|---|
| §2 deliverables 1–4, 7, 9, 10, 11, 13 | workspace, CLI/TUI binary, engine, SQLite, RTL tests, README, `docs/`, `release.yml` |
| §7 identity and contact model | `crates/crypto`, `crates/protocol/src/card.rs`, `crates/engine` |
| §8 protocol envelope, canonical ids, invalid events | `crates/protocol`, golden vectors in `crates/protocol/tests/fixtures.rs` |
| §9 signing and encryption | Ed25519 + X25519 + ChaCha20-Poly1305; see `docs/decisions/0002-crypto-backend.md` |
| §10 direct subscription workflow | follow request → approval → encrypted posts, with `withheld` for unverified followers |
| §11 mailing lists (both modes) | public-list delivery to the operator; `social-mail-reflector` for private relay, strict verification, per-member states |
| §14 local storage | schema v1, sticky positions, dedupe, orphan re-link, problems, deliveries, settings |
| §15 CLI | full command surface with `--json`, nonzero exits, actionable errors |
| §16–17 TUI and RTL | four tabs, three densities, conversation view, composer, bidi pipeline with Persian fixtures |
| §18 security requirements | quarantine, block, mismatch rejection, encrypted backups, `docs/security.md` |
| §19 tests | unit + integration + relay end-to-end + bidi; `cargo test --workspace` green on ubuntu-latest and windows-latest |
| §21 Actions | `release.yml` on `v*` tags → two archives + SHA-256 + SHA256SUMS.txt; branch CI was temporary and is now removed |
| §22 `TODO.md` | present, covering deferred work |
| §24 non-negotiables 1–17 | implemented except those that depend on mail transport (see below) |

### Still open

| Requirement | Blocker |
|---|---|
| OAuth2 / XOAUTH2 sign-in | app passwords only in v0.2; see `TODO.md` |
| IDLE, session reuse, UIDVALIDITY cursors | one connection per operation re-scans the mailbox each pass |
| Attachments and large media | 2 MB ceiling quarantines oversized messages; attachments are out of scope |
| §19 acceptance demonstration J0–J18 as transcripts | journeys exist as engine tests and CLI commands; scripted transcripts against a fake mailbox are next |
| OpenPGP interoperability (§1) | replaced by decision 0002 |

### Crate layout that shipped

```text
crates/crypto    keys, signatures, encryption, backups
crates/protocol  envelopes, canonical JSON, carriers, contact cards
crates/storage   SQLite schema and queries
crates/engine    application logic, transport boundary, relay
apps/cli         social-mail (CLI + TUI)
apps/social-mail-reflector
```
