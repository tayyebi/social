# 0003 — Mail transport: hand-written IMAP/SMTP over rustls, app passwords

- Status: accepted
- Date: 2026-11-05
- Applies to: v0.2

## Context

v0.1 shipped a file-spool transport: honest, testable, and useless for talking to
a real mailbox. v0.2 had to send and receive through ordinary email using
IMAP/SMTP, with credentials a normal person can obtain from their provider.

Constraints that shaped the decision:

1. CI must pass on `windows-latest` and `ubuntu-latest` with no system packages.
2. The build sandbox has no crates.io access, so every dependency choice has to be
   right the first time; a compile error costs a full CI round trip.
3. Credentials are the most dangerous thing this project touches. They must not end
   up in the config file, the database, diagnostics output, or logs.
4. Tests must not depend on the network or on a real mailbox.

## Decision

### Clients

Write the clients ourselves: `crates/mail` contains a small SMTP submission client
(EHLO, STARTTLS, AUTH PLAIN/LOGIN, MAIL/RCPT/DATA) and a small IMAP client (LOGIN,
SELECT, UID SEARCH, UID FETCH with literals, COPY, STORE, EXPUNGE, CREATE). Both are
blocking, use `std::net::TcpStream` with 30-second timeouts, and understand only
the responses their own commands produce.

### TLS

`rustls` with the `ring` backend and `webpki-roots` (bundled Mozilla roots).
Consequences: no OpenSSL, no schannel, no `cmake`, no system trust store to
misconfigure, identical behaviour on both target platforms. Certificate
verification cannot be turned off: there is no option for it.

### Credentials

App passwords, read from an environment variable (`SOCIAL_MAIL_PASSWORD`, or the
name in `mail.password_env`). The password is held only in memory for the duration
of one operation. It is never written to the config file, never persisted in
SQLite, never formatted into an error message, and never included in `Debug` output
(explicit redacting `Debug` impls exist for exactly that reason).

OAuth2/XOAUTH2 is out of scope for v0.2 and tracked in `TODO.md`.

### Message format

One carrier per message, `Content-Type: text/plain; charset=utf-8`,
`Content-Transfer-Encoding: base64`, `Subject: SocialMail/v1 carrier`,
`X-SocialMail: v1`. Base64 keeps every line short, avoids dot-stuffing ambiguity,
and survives providers that mangle 8-bit bodies. Incoming mail is classified by
the subject prefix or the header; anything else is left untouched in the mailbox.

### Folder behaviour

Processed protocol mail (accepted or already known) is marked `\Seen`, copied to
`SocialMail/`, marked `\Deleted`, and expunged — the Delta Chat pattern. Quarantined
mail stays where it is, marked read, with the reason recorded in the client's
Problems list. Nothing is deleted that we did not understand.

### Connections

One connection per operation. v0.2 queues are small, and a re-opened session is far
easier to reason about than a connection pool with retry semantics. The cost is
visible and bounded: N queued messages mean N SMTP sessions. If that becomes a
problem, IDLE plus a pooled session is the next step, not a rewrite.

## Alternatives considered

| Option | Why not |
|---|---|
| `lettre` + `imap` crates | Attractive (battle-tested, more features) but their TLS backends want `native-tls` (OpenSSL/schannel) or a `rustls` feature combination that could not be compiled or verified in this phase. A build failure would have cost the whole milestone. |
| `async-imap` / Tokio stack | Correct for a long-lived daemon, heavy for a CLI whose longest operation is one fetch. |
| OAuth2 first | Most personal providers now prefer it, but XOAUTH2 needs client registration per provider; app passwords still work for Fastmail, Proton Bridge, self-hosted Dovecot, and many others. Generic support first, OAuth2 as its own decision. |
| Storing the password in the OS keyring | Nice-to-have; adds platform-specific crates and an interactive prompt in CI. Environment variable is the documented path; the keyring is deferred. |

## Consequences

- Full control over the wire format, the error taxonomy and the credential path.
- No automatic feature coverage of exotic servers: no IDLE, no CONDSTORE, no
  `MOVE`, no pipelining. Folders are moved with COPY/STORE/EXPUNGE, which every
  server supports.
- STARTTLS is opportunistic only in the sense that the mode is explicit; there is
  no silent downgrade. If `starttls` is configured and the server refuses, the
  connection fails rather than continuing in the clear.
- Two-round-trip latency per message. Acceptable for a personal mailbox.

## Reversal criteria

Switch to maintained crates when **all** of the following hold:

1. A `rustls`-only configuration compiles on Windows and Linux in CI without
   system packages.
2. It exposes UID-based fetch with literals and flag manipulation (or the
   equivalents we rely on).
3. The credential story is at least as strict as this one — no plaintext password
   in a config file or a Debug print.
