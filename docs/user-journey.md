# User Journey Baseline: `social-mail` v0.1

Status: **decided** (v0.1 baseline, revised 2026-11-03 after product-owner choices).
Companion to [`../PLAN.md`](../PLAN.md).

PLAN.md specifies the machine: crypto, protocol, transport, storage, CLI surface,
TUI screens. This document specifies the **human** side of the same binary: who
the users are, what they do on day one, what they do on day thirty, what the
terminal shows them at each step, what every failure state means, and which
product decisions are now closed so implementation does not re-litigate them.

Where this document and PLAN.md disagree about **what the software must do**,
PLAN.md wins. Where they disagree about **how it feels, what it says, and in what
order the user is asked for things**, this document wins.

---

## 0. Confirmed product choices

These five were chosen explicitly by the maintainer (2026-11-03) and override any
earlier framing in this file. They are the assumptions behind every journey below.

| Choice | Selected | Consequence |
|---|---|---|
| Surface balance | **TUI and CLI as equals** | Every action is designed twice, with a parity test (§19, J14). No feature may be reachable from only one surface. |
| Onboarding | **Guided wizard** | `social-mail init` asks the questions in a fixed order; local (file) mode is an advanced `--offline` path, not an onboarding option. |
| Trust model | **Two-tier** | Unverified contacts can be read and followed; private posts go only to verified followers; withheld recipients are shown as `Not sent — unverified`. |
| Timeline | **Feed of cards** | Header (time · author · fingerprint · trust · audience), body, footer; grouped by day; no split pane. |
| Visual style | **Color-coded status** | Trust and delivery states get color and always keep their word; `NO_COLOR` loses nothing. |

---

## 1. Product promise, in user language

> Your identity is a key you hold. Your posts travel as ordinary email. Nobody
> runs a server that decides who you can follow or what you see.

What the user is promised:

1. **You are reachable by anyone whose key you have verified, and you can read
   anyone you choose to follow** — no account approval by a platform, no invite
   code, no signup form.
2. **Nobody can post as you.** Every item in the timeline verifies against the
   author's key, and the app tells you when it does not.
3. **Private means encrypted before it leaves your machine**, one copy per
   recipient.
4. **Nothing is lost because your laptop died mid-send.** Posts and deliveries
   survive restarts.

What the user is *not* promised, and must be told once, in plain words, during
onboarding:

- Your email provider still sees **who you talk to and when**.
- **Recipients can copy anything** you send them.
- **Lose the key, lose the identity.** There is no password reset, no account
  recovery, no support desk. (Key migration is deferred; see `TODO.md`.)
- Delivery feedback means *"your mail server accepted it"*, never *"they read
  it"*.

---

## 2. Who we are building for

| Persona | Description | Primary interface | Success in v0.1 |
|---|---|---|---|
| **Alice / Bob** (primary) | Technically comfortable, one laptop, one existing mailbox. Wants a small trusted circle, not an audience. | TUI, or CLI at equal fidelity | Two people exchange a verified, encrypted post in under 10 minutes, and can repeat it a week later without re-reading docs. |
| **Carol** (list operator) | Runs a private list for 5–50 people on her own mailbox. Comfortable with a terminal and a config file. | `social-mail-reflector` CLI | Approves a member from a queue, redistributes a post, and can explain a failed delivery. |
| **Dave** (public-list reader) | Reads a public list that already exists (Mailman/mlmmj). Does not want to run anything. | TUI or `timeline --json` | Sees verified authors and forged messages visibly rejected. |
| **Evaluator** (contributor/security reviewer) | Wants to reproduce the acceptance demo from a clean checkout without real mail accounts. | CLI | Runs the offline two-profile demo end to end with no credentials. |

**Degrees of sophistication we assume:** the primary user can copy a path,
paste an email address, and read a fingerprint aloud. They cannot be assumed to
know what a subkey is, what `UID VALIDITY` means, or why a signature failed.

---

## 3. The journey map

```text
        ┌──────────────┐
        │  J0 First run│  binary with no config
        └──────┬───────┘
               ▼
   ┌───────────────────────┐        ┌────────────────────────┐
   │ J1 Create identity    │───────►│ J2 Backup ceremony     │
   │ (+ passphrase)        │        │ (mandatory, skippable  │
   └──────────┬────────────┘        │  only with warning)    │
              ▼                     └────────────────────────┘
   ┌───────────────────────┐
   │ J3 Connect mailbox    │◄──── "local mode" shortcut (no mail)
   │ (folder decision)     │
   └──────────┬────────────┘
              ▼
   ┌───────────────────────┐        ┌────────────────────────┐
   │ J4 Pair & verify      │───────►│ J5 Follow / approve    │
   │ (fingerprint ceremony)│        │ (both sides)           │
   └──────────┬────────────┘        └───────────┬────────────┘
              ▼                                 ▼
   ┌───────────────────────┐        ┌────────────────────────┐
   │ J6 Post privately     │───────►│ J7 Read, thread, reply │
   └──────────┬────────────┘        └────────────────────────┘
              ▼
   ┌───────────────────────┐        ┌────────────────────────┐
   │ J8 Public list post   │        │ J9 Private list (Carol)│
   └───────────────────────┘        └────────────────────────┘
              ▼
   ┌───────────────┬───────────────┬───────────────┬───────────────┐
   │ J10 Offline / │ J11 Block &   │ J12 Restart & │ J13 Key lost  │
   │ delivery pain │ mute / unfollow│ recovery      │ (dead end)    │
   └───────────────┴───────────────┴───────────────┴───────────────┘
```

Every journey below lists: **trigger → steps → what the user sees → what must
never happen → acceptance check**. Journey IDs (`J0`–`J13`, plus the cross-cutting checks J14–J15) are the vocabulary
for UI tests, release notes, and issue titles.

---

## 4. Closed decisions

These are decided. Rationale is included so a later reader can tell a decision
from an accident. Reversal requires editing this table.

| # | Decision | Rationale |
|---|---|---|
| **D1** | **Two equal surfaces: interactive and scriptable.** Every user action exists both in the TUI and as a CLI command; neither is ever the only way to do something. Parity is enforced by a test (see §19, *Parity: CLI ↔ TUI*), not by intention. The TUI is what a person opens; the CLI is what a script, the demo, and a headless host uses — both are first-class. | Nothing may be reachable only by memorising a flag, and nothing may be reachable only by pressing keys (CI, the reflector, and the acceptance demo must never need a TTY). This costs a documented parity matrix and doubles some journey tests; it is worth it because it makes the engine boundary honest. |
| **D2** | **One command gets you to a usable app: `social-mail init`.** It is an interactive wizard with flag equivalents for every question (`--email`, `--name`, `--locale`, `--offline`, `--no-auto-sync`). Re-running is safe and resumes a partial setup. | Setup is the highest-risk funnel step. One command with resumable state beats five ordered commands the user can get wrong. |
| **D3** | **First launch is always the wizard.** No implicit config creation from defaults, no "empty timeline with a cryptic status bar". If `--config` points at nothing, the wizard offers to create it there. | An empty app that cannot explain itself is the most common failure mode of terminal software. |
| **D4** | **Identity creation comes before mailbox configuration.** | The key is the identity; the mailbox is a delivery route. Doing keys first means the app is already meaningful (and exportable) if SMTP/IMAP fails. |
| **D5** | **Local (file-based) mode exists but is not offered in the wizard.** The wizard is a mail wizard. Local mode is reachable only via `social-mail init --offline`, used for credential-free demos and CI, and marked in the TUI with a persistent `LOCAL MODE` label. | It is needed for the Milestone 2 gate and for tests without credentials, but presenting "try it without email" as an equal first-run choice splits the onboarding story and buries the product's actual value. Advanced path, loud label, no prominent invitation. |
| **D6** | **Terminology in the UI: Follow / Followers / Requests / Contacts / Lists.** "Subscription" and "subscriber" never appear in user-facing strings, even though the protocol and storage use those words. | Email-era "subscribe" reads as newsletters and mailing lists, which are a *different* feature we also have. One word per concept. |
| **D7** | **Verification is a deliberate two-step ceremony, never a checkbox.** Step 1: compare the grouped fingerprint out of band. Step 2: confirm in-app with the full grouped fingerprint visible and the consequence spelled out ("You will trust posts signed by this key"). | A signature proves key control, not personhood. The ceremony is the only defense against a swapped contact card, so it must be visible and slightly inconvenient. |
| **D8** | **Short fingerprints and safety phrases are display conveniences only.** The UI shows the last 8 hex characters next to names, and the full grouped fingerprint in every verification dialog. No word-list "safety phrase" in v0.1. | Word lists add translation surface and invite people to compare 32 bits instead of 160. Convenience for scanning, full string for deciding. |
| **D9** | **Two-tier trust: reading is permissive, private publishing is not.** Unverified contacts can be imported, followed, and read (their items are permanently tagged `unverified`). **Private posts are delivered only to verified followers.** Approving an unverified follower is allowed, with the consequence on the button: *"Approve — Alice won't receive private posts until you verify her."* | Verify-before-anything made the first ten minutes a fingerprint exercise; fully soft trust quietly leaks encrypted posts to a key nobody checked. The split puts friction exactly where the harm is: outbound confidential content, not inbound reading. |
| **D10** | **Following is never automatic and never mutual by default.** No follow-back, no suggestions, no auto-accept even when a request comes from a verified contact. | Never manufacture consent on the user's behalf. |
| **D11** | **Private is the default audience.** The composer opens with `Audience: Followers (encrypted)` and requires an explicit move to public. | Opt-in disclosure only. |
| **D12** | **Public posting is a two-gate action:** the composer shows a bordered warning block (*"Anyone on this list and its operators can read this in plain text"*), and the send action requires a typed confirmation only on the first public post per list, then a single explicit keypress (`Ctrl+S` on the warning panel) thereafter. | One-time friction teaches; permanent friction gets bypassed by muscle memory. |
| **D13** | **The home screen is the Timeline, and requests/queue are persistent badges, not screens you must remember.** Status bar always shows: identity, sync freshness, pending requests, queued deliveries. | A social app whose social surface is hidden is a mail client. |
| **D14** | **Sync runs automatically every 5 minutes while the TUI is idle**, plus on launch, plus on `r`. It is disableable in settings and via `--no-auto-sync`. Delivery of outgoing mail is attempted immediately on send and retried by the queue. | A timeline frozen until you press a key feels broken. 5 minutes is slow enough not to burn provider quota, fast enough to feel alive. |
| **D15** | **Never block the interface on the network.** Sync and sends run in the engine's worker; the TUI shows state transitions (`Syncing…`, `Retrying in 2m`) and stays navigable. Modals only appear for actions the user just took. | Terminal apps that freeze look crashed. |
| **D16** | **Delivery language is literal and never exceeds SMTP truth**: `Queued` → `Sending` → `Accepted by your mail server` → `Retrying in 2m (attempt 3)` → `Failed — needs action`. "Delivered" is banned; "Read" is banned. A private copy withheld by policy reads **`Not sent — unverified`** — never queued, never failed. | PLAN.md §13. The withheld state is a policy outcome decided before the queue, so presenting it as a queue state would mislead twice: it is not coming, and nothing is broken. |
| **D17** | **Public/private/list audience is visible on every timeline item and every queue row**, as a compact tag (`private`, `public`, `list:name`). | The user must never wonder whether something went out encrypted. |
| **D18** | **Threads open on demand.** Timeline is flat and chronological (local receipt order); `Enter` opens a thread view with replies, capped at 3 indentation levels. Orphaned replies appear under a `Pending (parent not received)` heading in both the timeline and the thread. | PLAN.md §8's ordering rules and orphan handling need a concrete presentation. |
| **D19** | **Read/unread is local and never synced.** A `New` divider marks items received since the timeline was last opened in this session; `u` toggles unread. No read receipts exist anywhere in the protocol. | Prevents an implied receipt channel and matches the "no server" model. |
| **D20** | **Blocking is local and silent.** v0.1 has no protocol `block` event: blocked events are dropped at ingest, blocked contacts disappear from the timeline, and no message is sent to the blocked party. **Mute** keeps delivery and subscription but hides items from the timeline. Both are reversible from Contacts. | A block event would be a notification. Silent local control is also what protects the user in the harassment case. |
| **D21** | **Key change is never silent.** If mail arrives signed by a key that claims a known contact's address but a different fingerprint, it is quarantined, the contact is flagged `key mismatch`, and no timeline item is created. The user resolves it from the Requests/Diagnostics screen. | PLAN.md §7 forbids silent replacement; the UI needs a place for the case to land. |
| **D22** | **Protocol mail folder moves are always the default, and the setting is a journey with a preview**: the settings screen states what will move (*"3 messages currently in INBOX are SocialMail messages"*) before the first move. Failures produce a persistent, dismissible-once banner; the app never quietly flips the setting off. | PLAN.md §12. Silence here looks like data loss. |
| **D23** | **The composer is plain text with `Enter` = newline, `Ctrl+S` = send.** `Esc` closes and **keeps the draft**. One autosaved draft per audience target, restored with a prompt. | Multi-line posting is the norm; `Enter`-to-send causes half-written posts in public. |
| **D24** | **UI locale is a first-run question with a live preview**, defaulted from `$LANG` (`fa*` → Persian, else English). Identifiers (fingerprints, event IDs, addresses, URLs) are always Latin/US-ASCII digits and stay LTR even inside Persian text. Persian digits are used for dates and counts when the locale is `fa`. | RTL is a headline feature (PLAN.md §17); asking once with a preview is cheaper than a settings hunt, and identifiers must stay copy-pasteable. |
| **D25** | **Every error state names an action.** No dead-end screens, no codes without a sentence, no "something went wrong". If there is genuinely nothing to do, the screen says so and offers export/log-path. | The support model is a README; the UI is the only help channel. |
| **D26** | **Reflector administration is CLI-only in v0.1.** No reflector TUI. | PLAN.md §11 lists CLI commands; a second TUI doubles the RTL/snapshot test surface for a small-operator audience already using a terminal. |
| **D27** | **No OS notifications, no sounds, no badges outside the app.** The status bar and the `Requests` badge are the notification system. | Cross-platform terminal notification support is unreliable; false promises are worse than quiet. |
| **D28** | **The first public post is the only place we interrupt the user uninvited**, and that interruption is educational, not blocking. | One earned interruption; everything else stays out of the way. |
| **D29** | **The timeline is a feed of cards, not an inbox of rows.** Each item is a bordered block: a header line (local time · author · short fingerprint · trust · audience), the body (up to a configurable line budget, with `…` + `Enter` for the rest), and a footer line (`2 replies`, delivery summary for own posts). Cards are grouped by day with a thin `Today` / `Yesterday` / date divider. | Chosen over an inbox list: posts are multi-paragraph and often mixed-direction, and a header/body/footer block is the only layout that keeps author, trust, and audience attached to the text they describe without a second column. Chosen over a split pane: split panes waste horizontal space at 80 columns, which is the size we must look good at. |
| **D30** | **Color carries status, words carry meaning.** Trust and delivery states get color (verified green, unverified/retrying amber, mismatch/failed red, private blue-cyan, public yellow, list magenta), and every one of them also renders as a word. `NO_COLOR`, `TERM=dumb`, and a `settings.color = never` option remove color entirely with no loss of information; `settings.color = always` is available for high-contrast terminals. | Chosen over monochrome: scanning a feed for "is this verified / did it send" is the app's most repeated task, and color makes it a glance instead of a read. The word must stay, because ssh sessions, screenshots, and color-blind users all exist. |
| **D31** | **One trust vocabulary, two renderings.** The engine returns trust (`verified`, `unverified`, `mismatch`, `invalid`) and delivery (`queued`, `sending`, `accepted`, `retrying`, `failed`, `cancelled`, `withheld`); the TUI maps these to color + word, the CLI maps them to words and `--json` enums. Neither surface invents a state, and a test asserts the mapping is total. | Two surfaces means two chances to describe the same fact differently — the fastest way to lose a user's trust is for the TUI to say "Sending" while the CLI says "queued". |

---

## 5. J0 — First run

**Trigger:** `social-mail` or `social-mail --tui` with no profile, or `social-mail init`.

```text
┌ social-mail ────────────────────────────────────────────────┐
│                                                              │
│   Your identity is a key. Your posts travel as email.        │
│                                                              │
│   Nobody runs a server that decides who you can follow.      │
│                                                              │
│   Let's set up this machine. You'll need about two minutes.  │
│                                                              │
│   [ Enter ]  Start setup        [ q ]  Quit                  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

**Wizard order (fixed):**

1. **Language** — `English` / `فارسی`, with a live preview line rendered in the
   chosen direction. Defaults from `$LANG`.
2. **Display name** — free text; can be changed later; explicitly *not* an
   identity claim ("This is only a label. Others verify your key, not your
   name.").
3. **Passphrase for the secret key** — hidden input, confirmation, minimum
   length enforced, and a one-line refusal to continue if empty:
   *"You can leave this blank, but then anyone with this laptop can post as
   you."* Blank requires an extra confirmation keystroke.
4. **Mail account** — address, then IMAP/SMTP host/port/user/password
   (hidden), with `[ t ] Test`. No "skip and try it without email" invitation:
   local mode is an advanced path (`init --offline`, D5), documented in the
   README's evaluator demo.
5. **Protocol folder** — `[x] Move SocialMail messages out of INBOX` (default
   checked) with a two-line explanation and the preview from D22 once a first
   scan exists (the wizard accepts "decide after first sync").
6. **Backup** — see J2.
7. **Done screen** — shows the full grouped fingerprint, the config path, and
   exactly three next actions: *"Copy my contact card"*, *"Add someone"*, and
   *"Open the app"*.

**Rules**

- A failed SMTP or IMAP connection at step 4 does **not** restart the wizard; it
  saves what exists and offers `Retry` / `Continue without email` / `Quit`.
- `Ctrl+C` at any point leaves a resumable partial profile; the next launch says
  *"Setup was not finished. Resume?"*.
- The wizard never prints the passphrase, the password, or key material to the
  terminal, the log, or the config file (passphrase is prompted, not stored, per
  PLAN.md §13).
- Every wizard question has a flag equivalent so CI and scripts never need a TTY.

**Must never happen:** creating a profile silently; storing a password in a
shell-visible flag by default; requiring IMAP to finish setup.

**Acceptance:** from a clean `$HOME`, `social-mail init` reaches the done screen
with outputs only in the chosen config/data directory; `Ctrl+C` mid-wizard then
re-running resumes at the interrupted question.

---

## 6. J1 — Creating the identity (the moment we must be honest)

Immediately after key generation, one screen, before any success animation:

```text
┌ Your identity ───────────────────────────────────────────────┐
│                                                              │
│   Fingerprint                                                │
│   3F2A 91C4 8D77 0B5E  6C1D 44A0 9E38 77B2  5A6F 0C21  D8E4  │
│                                                              │
│   This key is you. Anyone who has it can post as you.        │
│   If you lose it, there is no reset and no recovery —        │
│   you become a new person to everyone who knew you.         │
│                                                              │
│   We generated a revocation certificate. Keep a backup in    │
│   a place you control.                                       │
│                                                              │
│   [ b ] Back up now     [ c ] Copy fingerprint     [ Enter ] │
└──────────────────────────────────────────────────────────────┘
```

**Decisions applied:** D4, D6, D8, D24.

**Deliberate omission:** we do not explain subkeys, curves, or OpenPGP packet
structure here. A `?` on the fingerprint opens a short explainer (*"Why a long
string?"*) that mentions signing subkeys in one sentence and links to
`docs/security.md`.

**Acceptance:** the full fingerprint is readable at 80 columns without wrapping
mid-group (wraps only between groups); copying yields the ungrouped uppercase hex
plus the grouped form; the revocation certificate is written where the user
chose, or the backup screen refuses to close silently.

---

## 7. J2 — Backup ceremony

**Trigger:** wizard step 6, `b` from J1, or `social-mail identity export-backup`
at any time.

1. Choose destination path.
2. Choose protection: **passphrase-protected** (default and recommended) or
   **unprotected** (allowed, with a red warning and a typed `unprotected`).
3. Confirm the file was written by *reading it back* and verifying it decrypts
   with the just-entered passphrase. Show: *"Backup verified — it restores on
   this machine."*
4. Offer immediate **restore rehearsal** into a temporary directory (PLAN.md §18
   requires restoration testing). This is a real test, cheap and repeatable.

**Skipping:** allowed once, at the wizard level, with the warning *"No backup
means no second chance if this disk dies."* and a `Backup` reminder in the status
bar until it is done.

**Acceptance:** exporting twice produces two valid backups; a wrong passphrase on
restore fails with an actionable message and leaves the existing profile
untouched; restore rehearsal performs no writes to the live database.

---

## 8. J3 — Connecting the mailbox

**Trigger:** wizard step 4, or Settings → Mail account after local mode.

```text
┌ Mail account ────────────────────────────────────────────────┐
│  Address      alice@example.org                              │
│  IMAP         imap.example.org : 993   [TLS]                 │
│  SMTP         smtp.example.org : 465   [TLS]                 │
│  Password     **********                                     │
│                                                              │
│  [ t ] Test connection                                       │
└──────────────────────────────────────────────────────────────┘
```

**Test result must report three separate facts**, never one green light:

```text
  IMAP  connected · 3 folders · UID MOVE supported
  SMTP  connected · submitted a test message to yourself
  Folder SocialMail  will be created on first sync
```

**Failure states and their copy** (each with a next action):

| Condition | Message | Action offered |
|---|---|---|
| Bad password | "The server rejected the password. Some providers require an app password instead of your login password." | Edit, retry |
| TLS/cert failure | "Could not verify the server's certificate (`<reason>`). This may be a corporate proxy." | Retry, open docs, continue without mail |
| IMAP ok, SMTP refused | "Reading works; sending does not. You can receive posts but not publish." | Edit SMTP, continue read-only |
| No `UID MOVE` | "This server cannot move messages. SocialMail will copy and delete them instead." | Accept (informational, not an error) |
| Folder creation denied | "Could not create `SocialMail`. Messages will stay in INBOX." | Choose another folder name, keep in INBOX |

**Saving credentials:** environment variable or interactive prompt only
(PLAN.md §13). The UI says where the password came from and never echoes it.

**Acceptance:** each row above is reproducible with the fake services in
`crates/test-support`; a failure never discards an already-created identity.

---

## 9. J4 — Pairing and verification (the trust ceremony)

**Trigger:** `social-mail contacts import card.alice.socialmail`, the TUI
`i` key, or receiving a follow request that carries a certificate.

**Pairing is order-independent.** Both people must create an identity, then each
sends the other a **contact card**: a small signed file containing the public
certificate and the current delivery route.

```bash
social-mail identity contact-card --out alice.socialmail
```

The card is a normal file; it may travel by any channel the users trust
(already-verified email, USB, messenger). It is signed, so tampering is
detectable, but **a signature does not tell you who sent it** — that is what
verification is for.

**Verification screen:**

```text
┌ Verify contact ──────────────────────────────────────────────┐
│                                                              │
│   Alice                                      unverified       │
│   alice@example.org                                          │
│                                                              │
│   Fingerprint                                                │
│   3F2A 91C4 8D77 0B5E  6C1D 44A0 9E38 77B2  5A6F 0C21  D8E4  │
│                                                              │
│   Read these characters to Alice over a channel you already   │
│   trust — a phone call, in person, or a video chat. If they   │
│   match, this key is hers. If they don't, stop.              │
│                                                              │
│   [ v ] Mark verified        [ b ] Block        [ Esc ] Back  │
└──────────────────────────────────────────────────────────────┘
```

**Rules**

- The verification dialog always shows the **complete** fingerprint and the
  source of the key (imported file, follow request, or `profile` event).
- `Mark verified` requires a confirmation keypress after the full string has
  been on screen; the success state is explicit and states the tier change:
  *"Alice verified. Her posts are now trusted, and she can receive your private
  posts."*
- The dialog states what is *already* possible without verifying, so the user
  knows the stakes: *"Unverified, Alice can still be read, replied to, and
  followed — she just won't receive private posts."* (D9)
- Importing a card for a fingerprint we already know is a no-op (idempotent).
  Importing a **different** key claiming the same address triggers D21: it is
  quarantined, not merged, and the UI asks the user to resolve the mismatch.
- Blocking from this dialog is allowed (it is the natural moment to say no).

**Acceptance:** importing the same card twice produces one contact; importing a
card whose fingerprint differs from the stored contact opens the mismatch
resolution screen and mutates nothing; verification state is visible everywhere
the contact appears.

---

## 10. J5 — Following and approving

### Alice sends the request

```bash
social-mail follow request alice@…    # Bob's address
```

TUI: Contacts → `f` on a contact, or the done-screen action after J3.

Rules (two-tier trust, D9):

- Following works for **verified and unverified** contacts, because reading their
  posts is safe either way. If the contact is unverified, the confirmation line
  says: *"Bob isn't verified. You'll see his posts marked unverified until you
  check his fingerprint."*
- A known delivery route is still required; without one the action is disabled
  with the reason inline (*"Bob's card has no delivery route. Ask for a newer
  card."*).
- The request is signed and encrypted to Bob's key; it carries Alice's
  certificate and route so Bob can approve without another exchange.
- After sending, Alice sees a **Pending** row in Contacts (`waiting for Bob to
  approve`), not a "following" row. She does not receive Bob's private posts
  until approval arrives.

### Bob approves

Requests screen, badge in the status bar (`1 request`):

```text
┌ Follow requests ─────────────────────────────────────────────┐
│  Alice · alice@example.org                                   │
│  3F2A 91C4 … D8E4   unverified · received 2m ago             │
│                                                              │
│  Alice wants to follow you. Approving lets her reach you.    │
│  She won't receive your private posts until you verify her   │
│  fingerprint.                                                │
│                                                              │
│  [ v ] Verify and approve   [ a ] Approve unverified         │
│  [ d ] Decline              [ b ] Block                      │
└──────────────────────────────────────────────────────────────┘
```

- `v` runs the J4 fingerprint screen, then approves in one flow and sends
  `follow-accept`. Alice lands in the **verified tier**.
- `a` approves without verification. Alice lands in the **unverified tier**: she
  receives `follow-accept`, she can be replied to publicly, she can be followed,
  and **she is silently-excluded from every private post** — not queued, not
  failed, just absent from that post's delivery list.
- The consequence is stated at the moment of the decision, in the dialog, not
  afterwards: the dialog shows a persistent `unverified — no private posts` line
  for the contact, and the composer's audience line counts only verified
  followers (*"Followers (3 verified, 1 unverified — unverified followers won't
  receive this)"*).
- `Decline` removes the request and remembers the fingerprint so a duplicate
  request is not shown again (no notification is sent).
- Promoting a follower from unverified to verified later is a single action in
  Contacts and takes effect on the *next* post; `TODO`-style backlog delivery is
  not attempted (no backfill).
- Bob can later `follow remove` from Contacts; delivery stops immediately for
  future posts. **The UI says so**: *"Bob will stop receiving new posts. What he
  already has, he keeps."*

**Acceptance:** an unverified follower's delivery list for a private post is
empty and the post's delivery summary says so in words; a verified follower
receives exactly one encrypted copy; promoting a follower changes the next
post's recipients and nothing else; a declined request does not reappear;
removing a follower produces no new deliveries and no error surfaced to the
removed party.

---

## 11. J6 — Publishing a post

```text
┌ New post ────────────────────────────────────────────────────┐
│  To:  Followers · encrypted · 3 will receive      [ a ] change│
│       ⚠ 1 unverified follower won't receive this             │
│                                                              │
│  سلام، این یک آزمایش است.                                    │
│  Hello world                                                  │
│  ▌                                                            │
│                                                              │
│  Ctrl+S send   Esc save draft & close   ? help                │
└──────────────────────────────────────────────────────────────┘
```

**Audience chooser (`a`)** — exactly three options, each one line:

```text
  Followers (3 verified, 1 unverified)   encrypted per verified person
  Public list…                           signed, unencrypted, readable by all
  Save as draft                          keep it local for now
```

**After send:** the composer closes, the item appears in the timeline pinned into
the local chronology, and the status bar shows `1 queued`. The post's card footer
carries a one-line delivery summary (`sent 2 of 3 · 1 retrying · 1 withheld`),
and `Queue` expands it to one row per recipient (D16 vocabulary):

```text
  Bob        Accepted by your mail server   12:04
  Carol      Retrying in 2m (attempt 2)     12:04   [ R ] retry now
  Dave       Failed — needs action          12:04   [ i ] details
  Erin       Not sent — unverified          12:04   [ v ] verify her
```

**Rules**

- Sending an empty or whitespace-only post is refused inline, not with a modal.
- Length is capped (documented limit, enforced with a counter that appears near
  the limit); over-limit blocks send with the exact overflow.
- A private post with **zero verified followers** is allowed but warned before
  sending: *"None of your verified followers can receive this. It will be saved
  and sent to no one."* If there are unverified followers, the warning names the
  fix: *"3 followers won't receive this until you verify them."*
- Unverified followers are **withheld, not queued** (D9, D16). Withheld rows never
  enter the outgoing queue, never retry, and never fail; they resolve the moment
  the contact is verified — for future posts only, with no backfill.
- Recipient addresses may be shown on this local-only screen but are never
  placed in outbound headers (PLAN.md §9).

**Acceptance:** an accepted private post produces exactly one delivery record per
verified follower and zero for unverified, blocked, or removed followers;
promoting a follower to verified changes the next post only; blocked followers
cannot be re-added to a delivery by re-queuing; the outgoing item survives an
immediate kill -9 and resumes on restart.

---

## 12. J7 — Reading, threading, replying

**Timeline cards** (feed layout, D29; must fit 80×24 — roughly three cards):

```text
─────────────────── Today ────────────────────
┌ 12:04 · Bob · b7c3 · verified · private ─────────────────────┐
│ سلام، این یک آزمایش است.                                    │
│ Hello world                                                  │
│                                                              │
│ 2 replies                                                    │
└──────────────────────────────────────────────────────────────┘
┌ 11:47 · Carol · 4a19 · verified · list:rust-users ───────────┐
│ Does anybody still have that mail-sync bug from last week?   │
│ I couldn't reproduce it on 0.1.0.                            │
│                                                              │
│ 5 replies · unread                                           │
└──────────────────────────────────────────────────────────────┘
```

Color (D30) applies to the header's status words, never to the body text:
`verified` green, `unverified` amber, `mismatch`/`invalid` red, `private`
blue-cyan, `public` yellow, `list:…` magenta. Without color the words, the
`private`/`public` tag, and the position of the tag are unchanged, so the card is
identically informative.

Card rules:

- **Header line:** local receipt time · author · short fingerprint · trust word ·
  audience tag. Header is the only thing that is ever truncated (and it
  truncates from the right, keeping author and trust).
- **Body:** authored text, up to `settings.feed.body_lines` (default `6`) with a
  `… +4 lines` marker; `Enter` opens the full item. Persian and mixed-direction
  bodies render through the same text pipeline as everywhere else.
- **Footer:** reply count, `unread` marker, and for your own posts the delivery
  summary from §11. No footer if there is nothing to say.
- Day dividers (`Today`, `Yesterday`, `2026-11-01`) keep local chronology legible
  without timestamps competing with the body.
- Author display name falls back to address, then short fingerprint. The short
  fingerprint is a *scan* aid only; `Enter` → contact card shows the full one.
- Trust and delivery never render as bare icons; the word is always present
  (D30, PLAN.md §16).
- Own posts are visually distinguishable (a dim left rule) but not differently
  colored, so "mine" never competes with "trusted".

**Invalid events never look like normal events.** A quarantined item renders in a
separate `Problems` section with one sentence of cause and an action:

```text
  ✗ Message from alice@example.org          signature invalid
     This message claims to be from Alice but the signature does not
     match her key. It was not added to your timeline.
     [ i ] details   [ x ] delete
```

**Thread view:** `Enter` on an item. Parent at top, replies indented to a maximum
of 3 levels, orphaned replies grouped under `Pending (parent not received)` with
the note *"This reply arrived before the post it answers. It'll join the thread
when that post arrives."* When the parent arrives, the orphan moves automatically
and, if the user is viewing that thread, a one-line notice appears.

**Reply:** `R` in the thread or timeline. The composer is prefilled with
`Replying to Bob` and the audience is derived from the parent (private reply to a
private post; a public-list reply requires re-confirming the list target).
**A private reply to a public post is not allowed** — the thread's audience is
public, and escaping it would silently de-anonymize nothing but confuse the
privacy model. The composer says: *"This thread is public. Replies are posted to
the list."*

**Acceptance:** duplicate delivery yields one timeline item; reply-before-parent
shows under `Pending` then merges without duplication; a tampered payload is
listed in `Problems` and never in the timeline.

---

## 13. J8 — Public list posting

**Journey:** `social-mail lists add list@example.org --list-id <LIST_ID>` (or
Lists → `a`).

- Reading requires no approval: after the first successful sync, matching
  messages appear with `list:<name>` tags and verified authors.
- The list is displayed with its address and `List-ID`; a mismatch between the
  configured `List-ID` and the incoming header quarantines the message.
- **Posting to the list is the one place we interrupt** (D12):

```text
┌ Public post ─────────────────────────────────────────────────┐
│ ⚠  This will be sent to rust-users@example.org in plain text │
│    and cannot be recalled. The list operator and every       │
│    subscriber can read it, and it may be archived publicly.  │
│                                                              │
│    Ctrl+S  send anyway        Esc  cancel                    │
└──────────────────────────────────────────────────────────────┘
```

- The first public post to a given list additionally requires a typed `yes`
  (once per list, remembered).
- Private posts can never target a public list; the option does not exist in
  that audience's chooser.
- Invalid-signature list traffic is quarantined and counted: *"8 messages from
  this list could not be verified."* with a details view.

**Acceptance:** a forged list message never appears in the timeline; the warning
screen appears on the first post per list and on every public post in condensed
form; unsubscribing from a list stops ingestion without deleting history.

---

## 14. J9 — Private list via reflector (Carol, CLI only)

**Member journey**

1. Carol publishes the list address and fingerprint out of band (README page,
   email signature) — there is no directory.
2. Member runs `social-mail lists request-membership <list-address>` (encrypted
   signed request carrying the member's certificate).
3. Member waits; when approved, the CLI/UI says *"Carol approved you. You'll
   receive posts from this list."*
4. Member posts with audience `List: <name> (encrypted per member)`; the
   reflector re-encrypts copies, so delivery time varies per member and is shown
   per recipient where the member's client can see it.

**Operator journey (Carol)**

```bash
social-mail-reflector init --config reflector.toml   # mailbox + list identity
social-mail-reflector run                            # foreground worker
social-mail-reflector list-members
social-mail-reflector approve-member <FINGERPRINT>
social-mail-reflector remove-member <FINGERPRINT>
social-mail-reflector block-member <FINGERPRINT>
social-mail-reflector show-queue
social-mail-reflector retry-failed
```

- `run` prints a live one-line status per cycle (`cycle 42: 3 in, 12 out, 0
  failed`) and never prompts.
- Pending memberships are a queue that `list-members --pending` shows; approving
  prints the fingerprint and requires `--force` to approve an unverified key.
  The reflector is deliberately *stricter* than personal two-tier trust (D9):
  a list has one operator, no in-app fingerprint ceremony, and a fan-out blast
  radius, so unverified members stay out of redistribution entirely.
- A member's post that fails verification is **not** redistributed and is
  reported with the reason and the sender.
- The reflector never holds member keys; the admin UI/CLI states that the
  operator can read private list traffic (PLAN.md §11) — in `init` and in
  `list-members`.

**Acceptance:** redistribution preserves the original author signature; a
revoked/removed member receives nothing after removal; permanent failures are
listed with a reason and are retryable individually.

---

## 15. J10 — Offline, slow, and failing delivery

This is not an edge case; it is Tuesday. Every one of these states is reachable in
the TUI without leaving the current screen.

| Situation | User sees | Available action |
|---|---|---|
| No network at launch | `Offline — showing stored timeline` in the status bar; composing still works | Queue fills; `r` retries |
| Sync in progress | `Syncing… 12 of 40` | Stay navigable; `Esc` cancels politely |
| Sync failed (auth) | Banner: *"Mailbox sign-in failed. Sending and receiving are paused."* | Fix settings, retry |
| Sync failed (transient) | `Last sync 14m ago — retrying in 30s` | `r` for now |
| Message too large | Quarantined with *"Message exceeds the 2 MB limit."* | Delete, view headers |
| Server refused sender | `Failed — the server rejected this sender` | Edit config, retry, cancel |
| Queue retrying | `Retrying in 2m (attempt 3 of 6)` | `R` retry now, `c` cancel |
| Permanent failure | `Failed — needs action` + reason in plain words | `i` details, retry, cancel |
| Private post withheld | `Not sent — unverified` on the recipient row and in the post footer | `v` verify now (applies to future posts) |
| Folder move failed | Persistent banner: `Could not move 3 messages to SocialMail. They were still read from INBOX.` | Retry, change folder, disable moving (explicitly, not silently) |
| Clock skew | Warning on items whose author time is >10 min in the future: *"Bob's clock may be wrong."* | None needed; never reorders storage |

**Non-negotiable:** a user with a broken mailbox can still read the stored
timeline, compose, and export; the app never becomes unusable because the network
is down (D15, D25).

---

## 16. J11 — Discomfort: blocking, muting, leaving

Harassment on a decentralized system arrives as email; the user's controls must
be one keystroke away and described honestly.

| Action | Where | Effect | What we tell the user |
|---|---|---|---|
| **Mute** | Item/contact `m` | Hidden from timeline; delivery continues; subscription unchanged | "Hidden from your timeline. You still receive posts and deliveries continue." |
| **Block** | Contact `b` | Ingest-dropped; hidden everywhere; future delivery refused | "Blocked. New messages from Bob are discarded and never shown. Bob is not told." |
| **Unfollow** (you follow them) | Contact `f` | Stop receiving their posts | "You no longer receive Bob's posts. Bob is not told." |
| **Remove follower** (they follow you) | Contact `F` | No future private deliveries | "Bob stops receiving new posts. He keeps what he already has." |
| **Leave a list** | Lists `d` | Stop ingesting that list | "You'll stop seeing posts from this list. Past items stay in your timeline." |

All five are reversible from Contacts/Lists, and the result is stated as a fact,
not a euphemism. **No block/mute action ever sends mail** (D20).

**Acceptance:** a blocked contact's events are dropped before timeline insertion;
unblocking does not replay the backlog; mute and block are independently
reversible and both survive restart.

---

## 17. J12 — Restart, crash, and restore

**Journeys**

1. **Crash during sync** → on restart: no duplicate timeline entries, sync
   resumes from the stored checkpoint, and the UI says `Resumed sync`.
2. **Crash mid-send** → the queue row returns to `Queued` (never left stuck in
   `Sending`); nothing is silently dropped; at worst a duplicate is possible and
   the recipient's dedupe by `event_id` absorbs it.
3. **Kill during folder move** → no message is lost; if it was already moved, it
   is not processed twice.
4. **Restore from backup** on a new machine → wizard offers `Restore profile`;
   the restored fingerprint is displayed and the user is told which contacts are
   verified, which were pending, and that **their mailbox settings must be
   re-entered** (secrets are not in the backup).
5. **Database corrupt** → the app refuses to write, offers `Export logs` and
   `Restore from backup`, and never silently starts a fresh database.

**Acceptance:** each of 1–5 is an integration test; none requires a manual SQL
edit to recover, and none can be mistaken for data loss without an explicit
message saying data was lost.

---

## 18. J13 — Losing the key (the dead end, stated plainly)

If the key is gone and no backup exists, v0.1 has **no recovery** — key migration
is deferred (PLAN.md §8, `TODO.md`).

The journey is therefore a documentation journey, delivered at exactly two
moments: J1 (creation) and J2 (backup skip). It says:

> A new key is a new person. People who verified your old fingerprint will see
> posts signed by an unknown key, and they will be asked to verify you again —
> many will not.

The app must not offer any button implying recovery. If a backup exists but the
passphrase is lost, the message is the same, plus the path of the backup file and
the date it was written.

---

## 19. TUI information architecture

### Screens and navigation

```text
                       ┌────────────┐
                       │  Timeline  │◄── home
                       └─┬───┬───┬──┘
        ┌────────────────┘   │   └───────────────┐
        ▼                    ▼                   ▼
   ┌─────────┐         ┌───────────┐       ┌───────────┐
   │ Thread  │         │ Composer  │       │  Problem  │
   └────┬────┘         └───────────┘       │  details  │
        ▼                                 └───────────┘
   ┌───────────┐   ┌──────────┐   ┌────────┐   ┌─────────┐   ┌──────────┐
   │ Contacts  │   │ Requests │   │ Lists  │   │  Queue  │   │ Settings │
   └────┬──────┘   └────┬─────┘   └───┬────┘   └────┬────┘   └────┬─────┘
        ▼               ▼             ▼             ▼             ▼
   Verify dialog   Verify+approve  List detail   Delivery row   Folder,
   Import card     Decline/Block   Post/leave    Retry/cancel   locale, sync
```

- Single status bar at the bottom, always visible: `identity · sync freshness ·
  N queued · N requests · mode/locale`.
- Banners occupy one line above the status bar; they never steal focus.
- Modals are only for actions the user initiated; the queue/sync run in the
  background (D15).
- Layout survives 80×24 and grows gracefully; below 60 columns, cards lose the
  short fingerprint first, then the audience tag becomes a one-letter tag, then
  the border collapses into a dim indent (still readable, still grouped by day).
- Scrolling is card-granular above 100 columns and row-granular below, so a tall
  Persian post never strands its header off-screen.

### Feed layout

The Timeline is a vertical list of cards (D29), each `header / body / footer`,
separated by day dividers, never by per-card blank lines (borders do that job).
Card width equals the frame width minus one column of breathing room; text is
wrapped by the terminal-text pipeline, not by character count. Long bodies are
clipped at `settings.feed.body_lines` (default `6`) with an explicit `… +N lines`
marker, never silently.

### Color palette (D30)

| Token | Meaning | Color | Fallback |
|---|---|---|---|
| `verified` | signature valid, fingerprint confirmed | green | word `verified` |
| `unverified` | signature valid, fingerprint not confirmed | amber | word `unverified` |
| `mismatch` | known address, unknown key | red, bold | word `key mismatch` |
| `invalid` | signature does not verify | red, reverse | word `signature invalid` |
| `private` | encrypted audience | blue-cyan | word `private` |
| `public` | unencrypted list | yellow | word `public` |
| `list:<name>` | mailing list | magenta | word `list:<name>` |
| `accepted` / `failed` / `retrying` | delivery states | green / red / amber | words |
| `withheld` | policy-excluded recipient | dim amber | `Not sent — unverified` |

Rules: status color never applies to body text; `NO_COLOR=1`, `TERM=dumb`,
`--no-color`, and `settings.color = never` disable the palette completely; a
monochrome snapshot test runs the full matrix to prove no information is lost.

### Keymap (global, then contextual)

| Key | Global | Timeline | Thread | Requests | Contacts | Queue | Composer |
|---|---|---|---|---|---|---|---|
| `?` | help (screen-aware) | · | · | · | · | · | · |
| `q` | back; quit from Timeline | · | back | back | back | back | · |
| `Esc` | cancel modal | · | back | back | back | back | save draft & close |
| `j` / `k`, `↓` / `↑` | move | move | move | move | move | move | · |
| `g` then `t/c/f/l/q/s` | jump to screen | · | · | · | · | · | · |
| `r` | sync now | · | · | · | · | · | · |
| `n` | new post | new post | · | · | · | · | · |
| `a` | · | · | · | approve unverified | follow | · | audience |
| `d` | · | · | · | decline | · | · | · |
| `b` | · | · | · | block | block | · | · |
| `v` | · | · | · | verify & approve | verify | · | · |
| `f` | · | · | · | · | follow | · | · |
| `F` | · | · | · | · | remove follower | · | · |
| `m` | · | mute | · | · | mute | · | · |
| `u` | · | toggle unread | · | · | · | · | · |
| `R` | · | reply | reply | · | · | retry | · |
| `c` | · | · | · | · | · | cancel delivery | · |
| `i` | · | item details | · | · | import card | failure details | · |
| `Enter` | open / confirm | open thread | open item | open dialog | contact card | delivery details | newline |
| `Ctrl+S` | · | · | · | · | · | · | send |
| `y` | copy focused identifier (OSC 52 where available, otherwise show a selectable line) | · | · | · | · | · | · |

`q` never destroys state. `Esc` in a dialog always returns without side effects.

### Parity: CLI ↔ TUI (D1)

Every journey step below is reachable from both surfaces, and the CLI form is the
scripted equivalent of the keypress — not a reduced version.

| Action | TUI | CLI |
|---|---|---|
| Create profile | wizard | `init` (+ flags, `--offline`) |
| Show/export identity | Contacts → own card, `y` | `identity show`, `identity export-public` |
| Export/import backup | Settings → Backup | `identity export-backup`, `identity import-backup` |
| Import contact card | `i` in Contacts | `contacts import <FILE>` |
| Verify contact | `v` | `contacts verify <FP>` |
| Block / unblock | `b` | `contacts block <FP>`, `contacts unblock <FP>` |
| Follow / unfollow | `f` / `F` (own follows) | `follow request <ADDR>`, `follow remove <FP>` |
| Approve follower | Requests `v` / `a` | `follow approve <FP>` (+ `--force` for unverified) |
| Decline request | Requests `d` | `follow decline <FP>` |
| Post (private/list) | `n`, `a`, `Ctrl+S` | `post --text … --private` / `--public-list <ID>` |
| Reply | `R` | `reply <EVENT_ID> --text …` |
| Read timeline | scroll / `Enter` | `timeline [--limit N] [--json] [--thread <ID>]` |
| Sync | `r`, auto | `sync`, `sync --status` |
| Delivery state / retry / cancel | Queue `R` / `c` | `queue status`, `queue retry <ID>`, `queue cancel <ID>` |
| Verify-a-recipient prompt | `v` on a withheld row | `queue show <POST_ID>` lists withheld recipients |
| Lists add/remove/post | Lists `a` / `d` | `lists add`, `lists remove`, `post --public-list` |
| Mute / block from an item | `m` / `b` | `contacts mute <FP>` |
| Folder setting | Settings | `settings show`, `settings set-mail-folder` |
| Locale / color / auto-sync | Settings | `settings set-locale`, `set-color`, `set-auto-sync` |
| Diagnostics | Problems `i` | `diagnostics [--json]`, `problems list` |

**Parity rule:** a UI action without a command is a bug in the CLI; a command
without a UI affordance is a bug in the TUI. The parity test walks this table,
fails if a row is missing from either `--help` or the keymap registry, and asserts
both surfaces print the same state words for the same row (D31).

### Empty states (each with the one action that fixes it)

| Screen | Copy | Action |
|---|---|---|
| Timeline | "Nothing here yet. When someone you follow posts, it arrives by email and appears here." | `n` post, `g c` import a contact |
| Timeline, local mode | "This profile is in local mode. Posts are exchanged as files, not email." | explain bundle exchange |
| Requests | "No follow requests. People who want your private posts will appear here first." | · |
| Contacts | "No contacts yet. Import someone's contact card, or ask them for their fingerprint." | `i` import |
| Lists | "No lists. Add a public list to read it, or set up a reflector for a private one." | `a` add |
| Queue | "Nothing queued. Everything you've sent has been accepted by your mail server." | · |

---

## 20. Copy deck (source of truth for strings)

Strings live in one table, keyed, English first, Persian second, so the TUI and
CLI cannot drift and translators have context. Identifiers stay Latin (D24).

| Key | English | فارسی |
|---|---|---|
| `nav.timeline` | Timeline | خط زمانی |
| `nav.requests` | Requests | درخواست‌ها |
| `nav.contacts` | Contacts | مخاطبان |
| `nav.lists` | Lists | فهرست‌ها |
| `nav.queue` | Queue | صف ارسال |
| `nav.settings` | Settings | تنظیمات |
| `compose.new` | New post | نوشتهٔ تازه |
| `compose.audience.followers` | Followers | دنبال‌کنندگان |
| `compose.send` | Send | ارسال |
| `compose.cancel` | Cancel | لغو |
| `status.offline` | Offline — showing stored timeline | بی‌اتصال — نمایش خط زمانی ذخیره‌شده |
| `status.syncing` | Syncing… | در حال همگام‌سازی… |
| `trust.verified` | verified | تأییدشده |
| `trust.unverified` | unverified | تأییدنشده |
| `trust.key_mismatch` | key mismatch | کلید ناسازگار |
| `trust.signature_invalid` | signature invalid | امضا نامعتبر |
| `delivery.queued` | Queued | در صف |
| `delivery.sending` | Sending | در حال ارسال |
| `delivery.accepted` | Accepted by your mail server | پذیرفته‌شده در سرور ایمیل |
| `delivery.retrying` | Retrying in {n} | تلاش دوباره تا {n} |
| `delivery.failed` | Failed — needs action | ناموفق — نیاز به اقدام |
| `folder.setting` | Move SocialMail messages out of INBOX | انتقال پیام‌های SocialMail از صندوق ورودی |
| `verify.title` | Verify contact | تأیید مخاطب |
| `verify.hint` | Read these characters over a channel you trust. | این نویسه‌ها را از راهی مطمئن بخوانید. |

Rules: sentence case; no exclamation marks; no blame ("The server rejected the
password", not "You entered a wrong password"); no jargon in primary text
(`UID MOVE`, `subkey`, `canonical` only behind `?`).

---

## 21. Accessibility and terminal reality

- **No color-only meaning.** Color is a fast path to status (D30); the word is
  always present, and `NO_COLOR` / `--no-color` / `settings.color = never` must
  leave the screen identically informative. Works over `ssh`, in `screen`, on a
  monochrome theme, and in a screenshot.
- **No mouse requirement.** Every action has a key; mouse support, if added, is
  additive.
- **Resize is a first-class event**: every screen re-renders safely at any size,
  never panics, never leaves stale glyphs.
- **Persian correctness is measured, not vibes**: the fixtures in PLAN.md §17 are
  rendered in snapshot tests at 40/80/120 columns, in both locales, and the
  composer is tested in logical-order editing mode. We do **not** claim visual
  RTL cursor editing (PLAN.md §17, `TODO.md`).
- **Screen readers / `TERM=dumb`**: not supported in v0.1; CLI `--json` output is
  the accessible path, documented in README.

---

## 22. Journey acceptance matrix

Every journey has an owner, a test, and a demonstrable artifact. Journeys marked
*manual* still get a scripted checklist entry so demos are repeatable.

| ID | Journey | Automated test | Acceptance evidence |
|---|---|---|---|
| J0 | First run | `tests/integration/journey_init.rs` (pty-less flag mode + resume) | clean-run log |
| J1 | Identity | unit: fingerprint grouping, copy form | snapshot |
| J2 | Backup | integration: export → restore rehearsal → wrong passphrase | test output |
| J3 | Mailbox | integration vs fake IMAP/SMTP: 6 failure rows | matrix table |
| J4 | Verify | integration: double import, mismatch quarantine | snapshot |
| J5 | Follow/approve | integration: 3-profile flow, unverified tier receives no private post, promotion applies to next post only, decline does not reappear | test output |
| J6 | Post | integration: one delivery per verified follower, withheld rows for unverified, zero-verified warning, kill -9 resume | test output |
| J7 | Read/reply | integration: duplicates, orphan-then-parent, tampered payload | snapshot + test |
| J8 | Public list | integration: forged rejection, warning gate, List-ID mismatch | snapshot |
| J9 | Reflector | integration: signature preserved, removal effective, retry | CLI transcript |
| J10 | Degraded | integration: every row of §15's failure table | matrix table |
| J11 | Block/mute | integration: drop before insert, unblock no replay | test output |
| J12 | Restart | integration: 5 recovery cases | test output |
| J13 | Key loss | manual checklist + doc assertion | docs link |
| J14 | Surface parity | `tests/parity.rs`: every row of the §19 parity table exists in `--help` and in the keymap registry, and both surfaces return the same state words (D31) | parity report |
| J15 | Color independence | snapshot matrix with and without color at 40/80/120 columns, both locales | diffed snapshots |

The v0.1 acceptance demonstration in PLAN.md §19 must pass **twice: once through
the TUI and once as a single scripted CLI transcript** (Milestone 5 gate). Either
one alone is an incomplete gate under D1.

---

## 23. Implementation consequences (what this changes in PLAN.md)

1. **New CLI commands** required by journeys (additive to §15):
   `identity contact-card`, `contacts import <FILE>` already exists,
   `lists request-membership`, and `init` with a full flag set
   (`--email --name --locale --offline --no-auto-sync --config`).
2. **New TUI screens/details** implied by §19: `Problems`/quarantine list,
   `Pending replies`, delivery detail view, withheld-recipient row, `LOCAL MODE`
   marker, key-mismatch resolution.
3. **New settings**: `ui.locale`, `ui.color` (`auto|always|never`),
   `sync.auto_interval_minutes` (default `5`, `0` = manual),
   `feed.body_lines` (default `6`), `compose.max_bytes`, and the existing
   `mail.move_protocol_messages`.
4. **New engine outputs**: per-recipient delivery summary for a post (including
   `withheld` reason), so both surfaces render from one struct (D31) — this is a
   small addition to PLAN.md §14's `outgoing_deliveries` semantics: a withheld
   recipient gets a row with reason `unverified` and never an SMTP attempt.
5. **New tests**: journey integration tests (J0–J13), parity test (J14), color
   independence snapshots (J15), string-table coverage (every UI key has both
   locales), TUI snapshot matrix at 40/80/120 columns.
6. **New docs**: this file; `docs/protocol.md` gains nothing (no protocol
   change); README gains a "Five-minute two-person demo" section that follows
   J0→J7 verbatim, in both its interactive and scripted forms.

---

## 24. Deliberately not in v0.1 UX

Onboarding tours · suggested contacts · follow-back prompts · OS notifications ·
themes · global search · rich text/Markdown · reactions · read receipts · visual
RTL cursor editing · multi-account switcher · in-app help beyond `?` · graphical
installers · auto-update.

Each has a home in `TODO.md` if it survives the first ten users.
