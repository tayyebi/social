# Deferred roadmap

This file records work intentionally excluded from the Rust CLI/TUI v0.1.
The current implementation must not add native GUI, browser, or alternative
transport clients before the core email workflows are stable.

## Android app (v0.3 shipped the companion)

- [x] Compose app over the shared engine through one JSON-over-JNI entry point.
- [x] Feed, threads, replies, compose, contacts, requests, problems, settings, sync.
- [x] Session-only app password; no backup, no device transfer of the identity.
- [x] `release.yml` builds the APK for arm64-v8a, armeabi-v7a and x86_64, signed when
      the `ANDROID_*` secrets exist and named `-unsigned` otherwise.
- [ ] Restore a desktop identity from a backup file (most important gap: today the
      phone can only create a new identity).
- [ ] Export a backup from the phone so an identity can move to the desktop.
- [ ] Store the app password in the Android Keystore instead of memory.
- [ ] WorkManager-driven background sync with a notification, respecting the
      provider's rate limits.
- [ ] Share targets: post from another app, "add contact" from a shared card.
- [ ] Persian UI strings and RTL layout review on device.
- [ ] Play Store listing (AAB, store assets, privacy policy) if it is ever wanted.
- [ ] Instrumented tests on an emulator in CI (the JNI bridge is unit-tested today).

## Mail transport (v0.2 shipped the basics)

- [x] IMAP receive (UID SEARCH/FETCH), SMTP send with STARTTLS and implicit TLS.
- [x] Provider-independent failure classification: auth, transient, permanent, too large.
- [x] MIME mapping: one carrier per message, UTF-8 base64 body.
- [x] Protocol folder moves (`SocialMail/`) with copy-then-delete.
- [x] 2 MB message ceiling with quarantine above it.
- [x] App-password credentials from the environment, redacted in Debug output.
- [ ] OAuth2 / XOAUTH2 sign-in (Gmail, Outlook) with per-provider client registration.
- [ ] OS keyring credential storage (Windows Credential Manager, Secret Service).
- [ ] `IDLE` and a reused session for long-running clients.
- [ ] UIDVALIDITY-aware resumable sync (v0.2 re-scans the mailbox each pass).
- [ ] Public-list reading from the mailbox; reflector parity for public lists.
- [ ] HTML-only senders: currently only `text/plain` and base64/quoted-printable parts.

## Shared-core preparation

- [ ] Keep protocol, verification, trust, and timeline logic independent of UI. *(done)*
- [ ] Define a stable, minimal application-engine API. *(started: `crates/engine`)*
- [ ] Evaluate UniFFI for Kotlin and Swift bindings.
- [ ] Evaluate a C ABI for platforms without UniFFI support.
- [ ] Extract a headless daemon (`social-maild`) that owns the spool and the queue.

## Android

- [ ] Kotlin client over the shared core.
- [ ] Background sync respecting Doze.
- [ ] Notification privacy: no post text on the lock screen by default.

## Apple platforms

- [ ] SwiftUI client over the shared core.
- [ ] Background fetch and share extension.
- [ ] Keychain storage for the identity file.

## Native desktop graphical clients

- [ ] Tauri or native toolkit over the same engine.
- [ ] Multi-window timeline and composer.

## Browser

- [ ] WebAssembly build of protocol + storage.
- [ ] IndexedDB-backed storage adapter.
- [ ] Explicit warning that browser builds have weaker key protection.

## Protocol and identity

- [ ] OpenPGP wire compatibility (or a documented bridge), if interoperability is
      ever required. See `docs/decisions/0002-crypto-backend.md`.
- [ ] Key rotation and migration (v0.1 deliberately has none).
- [ ] Key-loss recovery beyond an encrypted backup.
- [ ] Multi-device identity synchronisation.
- [ ] Deeper reply trees (storage caps recursion at depth 8 today).
- [ ] Reactions, reposts, edits, deletes.
- [ ] Attachments and large-media distribution.
- [ ] Group threads beyond list relay.

## Networking and discovery

- [ ] Address book import from vCard.
- [ ] Optional directory services (explicitly rejected for v0.1).
- [ ] Recommendation or ranking algorithms (explicitly rejected).

## Privacy and security

- [ ] Metadata analysis: what SPF/DKIM/DMARC leak about the author's mail provider.
- [ ] Optional padding for message length.
- [ ] Sealed-sender style reply paths for lists.
- [ ] Independent security review before any 1.0 claim.

## Terminal interface

- [ ] Visual RTL cursor editing inside the composer (v0.1 edits logical text).
- [ ] Persian glyph shaping for terminals that do not shape Arabic script.
- [ ] Mouse support.
- [ ] Theme files beyond colour on/off.
- [ ] Localisation beyond English and Persian (infrastructure is in place).
