//! The application engine: identities, contacts, follows, posts, feed, queue and
//! transport flushing. The CLI and the TUI are thin shells over this type.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use social_mail_crypto::{base64, Identity, PublicIdentity};
use social_mail_protocol::{
    now_rfc3339, Audience, Carrier, ContactCard, Envelope, EventKind, FollowAcceptPayload,
    FollowRemovePayload, FollowRequestPayload, PostPayload, ProfilePayload, ReplyPayload,
    SignedEvent,
};
use social_mail_storage::{
    Contact, Counts, Database, DeliveryRow, DeliveryState, EventRow, FollowRow, FollowState,
    ProblemRow, ProfileRow, Tab, Trust,
};

use crate::config::Config;
use crate::transport::SpoolTransport;
use crate::EngineError;

pub const IDENTITY_FILE: &str = "identity.json";
pub const DB_FILE: &str = "social-mail.db";
/// What "accepted" means for the transport that v0.1 ships with.
pub const ACCEPTED_LABEL: &str = "Accepted for delivery";

#[derive(Clone, Debug, Default)]
pub struct InitOptions {
    pub display_name: String,
    pub address: String,
    pub locale: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportOutcome {
    Added,
    Updated,
    Unchanged,
    /// A different key claims an address we already know: never merged silently.
    Mismatch {
        stored: String,
    },
    OwnCard,
}

#[derive(Clone, Debug)]
pub struct FeedItem {
    pub event_id: String,
    pub position: i64,
    pub author: String,
    pub author_name: String,
    pub trust: Trust,
    pub audience: Audience,
    pub list_id: Option<String>,
    pub text: String,
    pub created_at: String,
    pub received_at: String,
    pub seen: bool,
    pub reply_count: i64,
    pub delivery_summary: Option<String>,
    pub is_mine: bool,
}

#[derive(Clone, Debug)]
pub struct ConversationItem {
    pub event_id: String,
    pub author: String,
    pub author_name: String,
    pub trust: Trust,
    pub text: String,
    pub created_at: String,
    pub depth: usize,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub post_event_id: String,
    pub preview: String,
    pub deliveries: Vec<DeliveryRow>,
}

#[derive(Clone, Debug, Default)]
pub struct SyncReport {
    pub received: usize,
    pub accepted: usize,
    pub duplicates: usize,
    pub dropped: usize,
    pub quarantined: usize,
    pub sent: usize,
    pub failed: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IngestOutcome {
    Accepted,
    Duplicate,
    Dropped,
    Quarantined(String),
}

#[derive(Clone, Debug)]
pub struct ContactView {
    pub contact: Contact,
    pub i_follow: Option<FollowState>,
    pub follows_me: Option<FollowState>,
    pub pending_request: bool,
}

pub struct Engine {
    config_path: PathBuf,
    pub config: Config,
    db: Database,
    identity: Identity,
    profile: ProfileRow,
}

impl Engine {
    // ------------------------------------------------------------ lifecycle

    pub fn create(config_path: &Path, options: InitOptions) -> Result<Self, EngineError> {
        let mut config = Config::load(config_path)?;
        if !options.display_name.trim().is_empty() {
            config.profile_name = options.display_name.trim().to_string();
        }
        if !options.address.trim().is_empty() {
            config.address = options.address.trim().to_string();
        }
        if !options.locale.trim().is_empty() {
            config.locale = options.locale.trim().to_string();
        }
        fs::create_dir_all(&config.data_dir)?;
        fs::create_dir_all(&config.spool_dir)?;
        config.save(config_path)?;

        let identity = Identity::generate();
        write_identity(&config.data_dir, &identity)?;
        let db = Database::open(config.data_dir.join(DB_FILE))?;
        let profile = ProfileRow {
            fingerprint: identity.fingerprint(),
            display_name: config.profile_name.clone(),
            address: config.address.clone(),
            locale: config.locale.clone(),
            created_at: now_rfc3339(),
        };
        db.set_profile(&profile)?;

        let mut engine = Engine {
            config_path: config_path.to_path_buf(),
            config,
            db,
            identity,
            profile,
        };
        engine.publish_profile()?;
        Ok(engine)
    }

    pub fn open(config_path: &Path) -> Result<Self, EngineError> {
        let config = Config::load(config_path)?;
        let identity = read_identity(&config.data_dir)
            .map_err(|_| EngineError::NotInitialised(config_path.display().to_string()))?;
        let db = Database::open(config.data_dir.join(DB_FILE))?;
        let profile = db
            .profile()?
            .ok_or_else(|| EngineError::NotInitialised(config_path.display().to_string()))?;
        if profile.fingerprint != identity.fingerprint() {
            return Err(EngineError::Config(
                "identity file does not match the stored profile".into(),
            ));
        }
        Ok(Engine {
            config_path: config_path.to_path_buf(),
            config,
            db,
            identity,
            profile,
        })
    }

    pub fn exists(config_path: &Path) -> bool {
        match Config::load(config_path) {
            Ok(config) => config.data_dir.join(IDENTITY_FILE).exists(),
            Err(_) => false,
        }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn fingerprint(&self) -> String {
        self.profile.fingerprint.clone()
    }

    pub fn profile(&self) -> &ProfileRow {
        &self.profile
    }

    pub fn save_config(&self) -> Result<(), EngineError> {
        self.config.save(&self.config_path)
    }

    pub fn data_dir(&self) -> &Path {
        &self.config.data_dir
    }

    // ------------------------------------------------------------- identity

    pub fn contact_card(&self) -> Result<ContactCard, EngineError> {
        Ok(ContactCard::create(
            &self.identity,
            &self.profile.display_name,
            &self.profile.address,
            &now_rfc3339(),
        )?)
    }

    pub fn export_card(&self, path: &Path) -> Result<ContactCard, EngineError> {
        let card = self.contact_card()?;
        write_private(path, card.to_pretty_json()?.as_bytes())?;
        Ok(card)
    }

    pub fn export_backup(&self, path: &Path, passphrase: &str) -> Result<String, EngineError> {
        let text = self.identity.export_backup(passphrase)?;
        write_private(path, text.as_bytes())?;
        Ok(self.fingerprint())
    }

    pub fn import_backup(&mut self, path: &Path, passphrase: &str) -> Result<String, EngineError> {
        let text = fs::read_to_string(path)?;
        let (identity, fingerprint) = Identity::import_backup(&text, passphrase)?;
        write_identity(&self.config.data_dir, &identity)?;
        self.identity = identity;
        self.profile.fingerprint = fingerprint.clone();
        self.db.set_profile(&self.profile)?;
        Ok(fingerprint)
    }

    /// A signed advisory statement about key loss. It cannot revoke anything on
    /// its own; see `docs/security.md`.
    pub fn revocation_statement(&self) -> Result<String, EngineError> {
        let mut text = String::new();
        text.push_str("social-mail key-loss statement\n\n");
        text.push_str(&format!("Fingerprint: {}\n", self.fingerprint()));
        text.push_str(&format!("Name: {}\n", self.profile.display_name));
        text.push_str(&format!("Address: {}\n", self.profile.address));
        text.push_str(&format!("Issued: {}\n\n", now_rfc3339()));
        text.push_str(
            "This statement is signed by the key it describes. Anyone holding this\n\
             file can confirm the signature while the key is still in use. It cannot\n\
             stop someone who holds the same key from posting, and it is not a\n\
             revocation certificate in the OpenPGP sense; key migration and\n\
             revocation propagation are deferred (see TODO.md).\n",
        );
        let signature = self.identity.sign(text.as_bytes());
        text.push_str(&format!("\nsignature: {}\n", signature.to_hex()));
        Ok(text)
    }

    // -------------------------------------------------------------- contacts

    pub fn import_card(&mut self, text: &str) -> Result<ImportOutcome, EngineError> {
        let card = ContactCard::from_json(text)?;
        self.import_card_struct(&card)
    }

    pub fn import_card_file(&mut self, path: &Path) -> Result<ImportOutcome, EngineError> {
        let text = fs::read_to_string(path)?;
        self.import_card(&text)
    }

    pub fn import_card_struct(&mut self, card: &ContactCard) -> Result<ImportOutcome, EngineError> {
        let public = card.verify()?;
        if public.fingerprint() == self.fingerprint() {
            return Ok(ImportOutcome::OwnCard);
        }
        let (ed25519, x25519) = public.to_b64();
        let now = now_rfc3339();
        match self.db.contact(&card.fingerprint)? {
            None => {
                self.db.upsert_contact(&Contact {
                    fingerprint: card.fingerprint.clone(),
                    display_name: card.display_name.clone(),
                    address: card.address.clone(),
                    ed25519,
                    x25519,
                    trust: Trust::Unverified,
                    muted: false,
                    added_at: now.clone(),
                    updated_at: now,
                })?;
                Ok(ImportOutcome::Added)
            }
            Some(existing) => {
                if existing.ed25519 != ed25519 || existing.x25519 != x25519 {
                    self.record_problem(
                        &card.fingerprint,
                        Some(&card.fingerprint),
                        None,
                        "key mismatch",
                        "A different key claims this contact's fingerprint. Nothing was changed.",
                    )?;
                    return Ok(ImportOutcome::Mismatch {
                        stored: existing.fingerprint,
                    });
                }
                let changed =
                    existing.display_name != card.display_name || existing.address != card.address;
                let mut updated = existing;
                updated.display_name = card.display_name.clone();
                updated.address = card.address.clone();
                updated.updated_at = now;
                self.db.upsert_contact(&updated)?;
                Ok(if changed {
                    ImportOutcome::Updated
                } else {
                    ImportOutcome::Unchanged
                })
            }
        }
    }

    pub fn contacts(&self) -> Result<Vec<Contact>, EngineError> {
        Ok(self.db.contacts()?)
    }

    pub fn contacts_view(&self) -> Result<Vec<ContactView>, EngineError> {
        let me = self.fingerprint();
        let mut out = Vec::new();
        for contact in self.db.contacts()? {
            let i_follow = self
                .db
                .follow(&me, &contact.fingerprint)?
                .map(|row| row.state);
            let follows_me = self
                .db
                .follow(&contact.fingerprint, &me)?
                .map(|row| row.state);
            out.push(ContactView {
                pending_request: matches!(follows_me, Some(FollowState::Pending)),
                contact,
                i_follow,
                follows_me,
            });
        }
        Ok(out)
    }

    pub fn contact(&self, fingerprint: &str) -> Result<Option<Contact>, EngineError> {
        Ok(self.db.contact(fingerprint)?)
    }

    pub fn resolve_fingerprint(&self, needle: &str) -> Result<String, EngineError> {
        let needle = needle.trim();
        if needle.is_empty() {
            return Err(EngineError::Invalid("empty fingerprint".into()));
        }
        let upper = needle.to_uppercase();
        if upper.len() == 40 && upper.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(upper);
        }
        if upper.len() < 4 {
            return Err(EngineError::Invalid(
                "fingerprints are 40 hex characters, or use at least 4 to match a suffix".into(),
            ));
        }
        let matches: Vec<Contact> = self
            .db
            .contacts()?
            .into_iter()
            .filter(|contact| contact.fingerprint.ends_with(&upper))
            .collect();
        match matches.len() {
            0 => Err(EngineError::NotFound(needle.to_string())),
            1 => Ok(matches[0].fingerprint.clone()),
            _ => Err(EngineError::Invalid(format!(
                "{needle} matches {} contacts; use more characters",
                matches.len()
            ))),
        }
    }

    pub fn verify_contact(&mut self, fingerprint: &str) -> Result<(), EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        self.db.set_trust(&fingerprint, Trust::Verified)?;
        // A verified follower starts receiving private posts from now on; there is
        // deliberately no backfill.
        let me = self.fingerprint();
        if let Some(row) = self.db.follow(&fingerprint, &me)? {
            if row.state == FollowState::Following {
                self.db
                    .upsert_follow(&fingerprint, &me, FollowState::Following, true)?;
            }
        }
        Ok(())
    }

    pub fn block_contact(&mut self, fingerprint: &str) -> Result<(), EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        self.db.set_trust(&fingerprint, Trust::Blocked)?;
        Ok(())
    }

    pub fn unblock_contact(&mut self, fingerprint: &str) -> Result<(), EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        self.db.set_trust(&fingerprint, Trust::Unverified)?;
        Ok(())
    }

    pub fn mute_contact(&mut self, fingerprint: &str, muted: bool) -> Result<(), EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        self.db.set_muted(&fingerprint, muted)?;
        Ok(())
    }

    // --------------------------------------------------------------- follows

    pub fn pending_requests(&self) -> Result<Vec<(FollowRow, Option<Contact>)>, EngineError> {
        let me = self.fingerprint();
        let mut out = Vec::new();
        for row in self.db.followers_of(&me)? {
            if row.state == FollowState::Pending {
                let contact = self.db.contact(&row.follower)?;
                out.push((row, contact));
            }
        }
        Ok(out)
    }

    pub fn my_follows(&self) -> Result<Vec<FollowRow>, EngineError> {
        Ok(self.db.follows_of(&self.fingerprint())?)
    }

    pub fn follow(&mut self, target: &str) -> Result<String, EngineError> {
        let target = self.resolve_fingerprint(target)?;
        if target == self.fingerprint() {
            return Err(EngineError::Invalid("cannot follow yourself".into()));
        }
        let contact = self
            .db
            .contact(&target)?
            .ok_or_else(|| EngineError::NotFound(format!("contact {target}")))?;
        if contact.trust.is_blocked() {
            return Err(EngineError::Invalid("this contact is blocked".into()));
        }
        let payload = FollowRequestPayload {
            message: None,
            card: self.contact_card()?,
        };
        let envelope = Envelope::new(
            EventKind::FollowRequest,
            self.fingerprint(),
            serde_json::to_value(&payload)?,
        );
        let signed = SignedEvent::sign(envelope, &self.identity)?;
        self.insert_signed(&signed, "accepted", None, None)?;
        self.db
            .upsert_follow(&self.fingerprint(), &target, FollowState::Pending, false)?;
        self.queue_delivery(&signed, &contact)?;
        self.flush_deliveries()?;
        Ok(signed.event_id)
    }

    pub fn approve_follower(
        &mut self,
        fingerprint: &str,
        verify: bool,
    ) -> Result<String, EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        let contact = self
            .db
            .contact(&fingerprint)?
            .ok_or_else(|| EngineError::NotFound(format!("contact {fingerprint}")))?;
        if verify {
            self.db.set_trust(&fingerprint, Trust::Verified)?;
        }
        let payload = FollowAcceptPayload {
            requester: fingerprint.clone(),
            card: self.contact_card()?,
        };
        let envelope = Envelope::new(
            EventKind::FollowAccept,
            self.fingerprint(),
            serde_json::to_value(&payload)?,
        );
        let signed = SignedEvent::sign(envelope, &self.identity)?;
        self.insert_signed(&signed, "accepted", None, None)?;
        self.db.upsert_follow(
            &fingerprint,
            &self.fingerprint(),
            FollowState::Following,
            verify,
        )?;
        self.queue_delivery(&signed, &contact)?;
        self.flush_deliveries()?;
        Ok(signed.event_id)
    }

    pub fn decline_follower(&mut self, fingerprint: &str) -> Result<(), EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        // Deliberately silent: declining sends no mail.
        self.db.upsert_follow(
            &fingerprint,
            &self.fingerprint(),
            FollowState::Declined,
            false,
        )?;
        Ok(())
    }

    pub fn remove_follower(&mut self, fingerprint: &str) -> Result<(), EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        self.db.upsert_follow(
            &fingerprint,
            &self.fingerprint(),
            FollowState::Removed,
            false,
        )?;
        if let Some(contact) = self.db.contact(&fingerprint)? {
            let envelope = Envelope::new(
                EventKind::FollowRemove,
                self.fingerprint(),
                serde_json::to_value(FollowRemovePayload::default())?,
            );
            let signed = SignedEvent::sign(envelope, &self.identity)?;
            self.insert_signed(&signed, "accepted", None, None)?;
            self.queue_delivery(&signed, &contact)?;
            self.flush_deliveries()?;
        }
        Ok(())
    }

    pub fn unfollow(&mut self, fingerprint: &str) -> Result<(), EngineError> {
        let fingerprint = self.resolve_fingerprint(fingerprint)?;
        self.db.upsert_follow(
            &self.fingerprint(),
            &fingerprint,
            FollowState::Removed,
            false,
        )?;
        if let Some(contact) = self.db.contact(&fingerprint)? {
            let envelope = Envelope::new(
                EventKind::FollowRemove,
                self.fingerprint(),
                serde_json::to_value(FollowRemovePayload::default())?,
            );
            let signed = SignedEvent::sign(envelope, &self.identity)?;
            self.insert_signed(&signed, "accepted", None, None)?;
            self.queue_delivery(&signed, &contact)?;
            self.flush_deliveries()?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- posting

    pub fn post(
        &mut self,
        text: &str,
        audience: Audience,
        list: Option<String>,
    ) -> Result<String, EngineError> {
        let payload = PostPayload {
            text: text.to_string(),
            lang: Some(self.config.locale.clone()).filter(|value| !value.is_empty()),
            audience,
            list: list.clone(),
        };
        let envelope = Envelope::new(
            EventKind::Post,
            self.fingerprint(),
            serde_json::to_value(&payload)?,
        );
        let signed = SignedEvent::sign(envelope, &self.identity)?;
        self.insert_signed(&signed, "accepted", None, None)?;
        self.db.append_root(
            &signed.event_id,
            &self.fingerprint(),
            "post",
            audience_str(audience),
            list.as_deref(),
        )?;
        self.deliver_to_followers(&signed)?;
        self.flush_deliveries()?;
        Ok(signed.event_id)
    }

    pub fn reply(&mut self, parent_event_id: &str, text: &str) -> Result<String, EngineError> {
        let parent = self
            .db
            .event(parent_event_id)?
            .ok_or_else(|| EngineError::NotFound(parent_event_id.to_string()))?;
        let parent_payload: PostPayload = serde_json::from_str(&parent.payload)
            .map_err(|error| EngineError::Invalid(format!("parent payload: {error}")))?;
        let payload = ReplyPayload {
            parent: parent_event_id.to_string(),
            post: PostPayload {
                text: text.to_string(),
                lang: Some(self.config.locale.clone()).filter(|value| !value.is_empty()),
                audience: parent_payload.audience,
                list: parent_payload.list.clone(),
            },
        };
        let envelope = Envelope::new(
            EventKind::Reply,
            self.fingerprint(),
            serde_json::to_value(&payload)?,
        );
        let signed = SignedEvent::sign(envelope, &self.identity)?;
        self.insert_signed(&signed, "accepted", None, Some(parent_event_id))?;
        self.db
            .set_parent_author(&signed.event_id, &parent.author)?;
        if parent.author != self.fingerprint() {
            if let Some(contact) = self.db.contact(&parent.author)? {
                self.queue_delivery(&signed, &contact)?;
            }
        }
        self.flush_deliveries()?;
        Ok(signed.event_id)
    }

    fn deliver_to_followers(&mut self, signed: &SignedEvent) -> Result<(), EngineError> {
        let me = self.fingerprint();
        let followers = self.db.followers_of(&me)?;
        for row in followers {
            if row.state != FollowState::Following {
                continue;
            }
            let Some(contact) = self.db.contact(&row.follower)? else {
                continue;
            };
            if contact.trust.is_blocked() {
                continue;
            }
            self.queue_delivery(signed, &contact)?;
        }
        Ok(())
    }

    fn queue_delivery(
        &mut self,
        signed: &SignedEvent,
        contact: &Contact,
    ) -> Result<(), EngineError> {
        let verified = contact.trust.is_verified();
        let state = if verified {
            DeliveryState::Queued
        } else {
            DeliveryState::Withheld
        };
        self.db.upsert_delivery(&DeliveryRow {
            post_event_id: signed.event_id.clone(),
            recipient: contact.fingerprint.clone(),
            state,
            reason: if verified {
                None
            } else {
                Some("unverified".to_string())
            },
            attempts: 0,
            next_attempt_at: None,
            updated_at: now_rfc3339(),
        })?;
        Ok(())
    }

    pub fn flush_deliveries(&mut self) -> Result<(usize, usize), EngineError> {
        let pending = self.db.pending_deliveries()?;
        let transport = self.transport()?;
        let mut sent = 0;
        let mut failed = 0;
        for row in pending {
            let Some(event_row) = self.db.event(&row.post_event_id)? else {
                self.db.set_delivery_state(
                    &row.post_event_id,
                    &row.recipient,
                    DeliveryState::Failed,
                    Some("event missing"),
                    None,
                    true,
                )?;
                failed += 1;
                continue;
            };
            let Some(contact) = self.db.contact(&row.recipient)? else {
                self.db.set_delivery_state(
                    &row.post_event_id,
                    &row.recipient,
                    DeliveryState::Failed,
                    Some("recipient is no longer a contact"),
                    None,
                    true,
                )?;
                failed += 1;
                continue;
            };
            if !contact.trust.is_verified() {
                self.db.set_delivery_state(
                    &row.post_event_id,
                    &row.recipient,
                    DeliveryState::Withheld,
                    Some("unverified"),
                    None,
                    false,
                )?;
                continue;
            }
            let signed = signed_from_row(&event_row)?;
            let public = PublicIdentity::from_b64(&contact.ed25519, &contact.x25519)?;
            let carrier = Carrier::for_recipient(&signed, &public)?;
            let body = carrier.to_json()?;
            match transport.send(&contact.fingerprint, &body) {
                Ok(_) => {
                    self.db.set_delivery_state(
                        &row.post_event_id,
                        &row.recipient,
                        DeliveryState::Accepted,
                        None,
                        None,
                        true,
                    )?;
                    sent += 1;
                }
                Err(error) => {
                    let next = chrono::Utc::now() + chrono::Duration::minutes(2);
                    self.db.set_delivery_state(
                        &row.post_event_id,
                        &row.recipient,
                        DeliveryState::Retrying,
                        Some(&error.to_string()),
                        Some(&next.to_rfc3339()),
                        true,
                    )?;
                    failed += 1;
                }
            }
        }
        Ok((sent, failed))
    }

    pub fn retry_delivery(&mut self, post: &str, recipient: &str) -> Result<(), EngineError> {
        self.db
            .set_delivery_state(post, recipient, DeliveryState::Queued, None, None, false)?;
        self.flush_deliveries()?;
        Ok(())
    }

    pub fn cancel_delivery(&mut self, post: &str, recipient: &str) -> Result<(), EngineError> {
        self.db.set_delivery_state(
            post,
            recipient,
            DeliveryState::Cancelled,
            Some("cancelled by user"),
            None,
            false,
        )?;
        Ok(())
    }

    // -------------------------------------------------------------- receiving

    pub fn sync(&mut self) -> Result<SyncReport, EngineError> {
        let transport = self.transport()?;
        let mut report = SyncReport::default();
        for incoming in transport.receive()? {
            report.received += 1;
            match self.ingest(&incoming.body) {
                Ok(IngestOutcome::Accepted) => {
                    report.accepted += 1;
                    transport.acknowledge(&incoming.path)?;
                }
                Ok(IngestOutcome::Duplicate) => {
                    report.duplicates += 1;
                    transport.acknowledge(&incoming.path)?;
                }
                Ok(IngestOutcome::Dropped) => {
                    report.dropped += 1;
                    transport.acknowledge(&incoming.path)?;
                }
                Ok(IngestOutcome::Quarantined(_)) => {
                    report.quarantined += 1;
                    transport.quarantine(&incoming.path)?;
                }
                Err(error) => {
                    report.quarantined += 1;
                    self.record_problem(
                        "unparsed",
                        None,
                        None,
                        "could not process message",
                        &error.to_string(),
                    )?;
                    transport.quarantine(&incoming.path)?;
                }
            }
        }
        let (sent, failed) = self.flush_deliveries()?;
        report.sent = sent;
        report.failed = failed;
        self.db.set_sync_state("last_sync", &now_rfc3339())?;
        Ok(report)
    }

    pub fn ingest(&mut self, body: &str) -> Result<IngestOutcome, EngineError> {
        let carrier = Carrier::from_json(body)?;
        let author = carrier.author.clone();
        let Some(contact) = self.db.contact(&author)? else {
            self.record_problem(
                &carrier.event_id,
                Some(&author),
                None,
                "unknown author",
                "No contact key for this author. Import their contact card first (verification is a separate, deliberate step).",
            )?;
            return Ok(IngestOutcome::Quarantined("unknown author".into()));
        };
        if contact.trust.is_blocked() {
            return Ok(IngestOutcome::Dropped);
        }
        let public = PublicIdentity::from_b64(&contact.ed25519, &contact.x25519)?;
        let signed = match carrier.verify_with(&public, Some(&self.identity)) {
            Ok(signed) => signed,
            Err(error) => {
                self.record_problem(
                    &carrier.event_id,
                    Some(&author),
                    None,
                    "signature invalid",
                    &error.to_string(),
                )?;
                return Ok(IngestOutcome::Quarantined("signature invalid".into()));
            }
        };
        if self.db.event(&signed.event_id)?.is_some() {
            return Ok(IngestOutcome::Duplicate);
        }
        let me = self.fingerprint();
        match signed.event.kind {
            EventKind::Profile => {
                let payload: ProfilePayload = signed.event.payload_as()?;
                let mut updated = contact.clone();
                updated.display_name = payload.display_name;
                updated.updated_at = now_rfc3339();
                self.db.upsert_contact(&updated)?;
                self.insert_signed(&signed, "accepted", None, None)?;
            }
            EventKind::FollowRequest => {
                let payload: FollowRequestPayload = signed.event.payload_as()?;
                self.import_card_struct(&payload.card)?;
                self.db
                    .upsert_follow(&author, &me, FollowState::Pending, false)?;
                self.insert_signed(&signed, "accepted", None, None)?;
            }
            EventKind::FollowAccept => {
                let payload: FollowAcceptPayload = signed.event.payload_as()?;
                if payload.requester != me {
                    self.record_problem(
                        &signed.event_id,
                        Some(&author),
                        Some("follow-accept"),
                        "approval for someone else",
                        "This approval names a different fingerprint.",
                    )?;
                    return Ok(IngestOutcome::Quarantined("not for me".into()));
                }
                self.import_card_struct(&payload.card)?;
                self.db
                    .upsert_follow(&me, &author, FollowState::Following, true)?;
                self.insert_signed(&signed, "accepted", None, None)?;
            }
            EventKind::FollowRemove => {
                self.db
                    .upsert_follow(&author, &me, FollowState::Removed, false)?;
                self.insert_signed(&signed, "accepted", None, None)?;
            }
            EventKind::RouteUpdate => {
                let payload: social_mail_protocol::RouteUpdatePayload =
                    signed.event.payload_as()?;
                let mut updated = contact.clone();
                updated.address = payload.address;
                updated.updated_at = now_rfc3339();
                self.db.upsert_contact(&updated)?;
                self.insert_signed(&signed, "accepted", None, None)?;
            }
            EventKind::Post => {
                let payload: PostPayload = signed.event.payload_as()?;
                self.insert_signed(&signed, "accepted", None, None)?;
                self.db.append_root(
                    &signed.event_id,
                    &author,
                    "post",
                    audience_str(payload.audience),
                    payload.list.as_deref(),
                )?;
                self.db.resolve_orphans(&signed.event_id, &author)?;
            }
            EventKind::Reply => {
                let payload: ReplyPayload = signed.event.payload_as()?;
                let parent_author = self.db.event(&payload.parent)?.map(|row| row.author);
                self.insert_signed(&signed, "accepted", None, Some(&payload.parent))?;
                if let Some(parent_author) = parent_author {
                    self.db
                        .set_parent_author(&signed.event_id, &parent_author)?;
                }
            }
        }
        Ok(IngestOutcome::Accepted)
    }

    fn publish_profile(&mut self) -> Result<(), EngineError> {
        let payload = ProfilePayload {
            display_name: self.profile.display_name.clone(),
            lang: Some(self.config.locale.clone()).filter(|value| !value.is_empty()),
        };
        let envelope = Envelope::new(
            EventKind::Profile,
            self.fingerprint(),
            serde_json::to_value(&payload)?,
        );
        let signed = SignedEvent::sign(envelope, &self.identity)?;
        self.insert_signed(&signed, "accepted", None, None)?;
        Ok(())
    }

    // ------------------------------------------------------------------ feed

    pub fn timeline(&self, tab: Tab, limit: usize) -> Result<Vec<FeedItem>, EngineError> {
        let me = self.fingerprint();
        let rows = self.db.timeline(tab, &me, limit, None)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(self.feed_item(&row)?);
        }
        Ok(out)
    }

    pub fn timeline_before(
        &self,
        tab: Tab,
        limit: usize,
        before: i64,
    ) -> Result<Vec<FeedItem>, EngineError> {
        let me = self.fingerprint();
        let rows = self.db.timeline(tab, &me, limit, Some(before))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(self.feed_item(&row)?);
        }
        Ok(out)
    }

    fn feed_item(&self, row: &social_mail_storage::TimelineRow) -> Result<FeedItem, EngineError> {
        let event = self
            .db
            .event(&row.event_id)?
            .ok_or_else(|| EngineError::NotFound(row.event_id.clone()))?;
        let payload: PostPayload = serde_json::from_str(&event.payload)
            .map_err(|error| EngineError::Invalid(format!("payload: {error}")))?;
        let (author_name, trust) = self.display_author(&row.author)?;
        Ok(FeedItem {
            event_id: row.event_id.clone(),
            position: row.position,
            author: row.author.clone(),
            author_name,
            trust,
            audience: payload.audience,
            list_id: payload.list.clone(),
            text: payload.text,
            created_at: event.created_at,
            received_at: event.received_at,
            seen: row.seen,
            reply_count: self.db.direct_reply_count(&row.event_id)?,
            delivery_summary: self.delivery_summary(&row.event_id)?,
            is_mine: row.author == self.fingerprint(),
        })
    }

    fn display_author(&self, fingerprint: &str) -> Result<(String, Trust), EngineError> {
        if fingerprint == self.fingerprint() {
            return Ok((self.profile.display_name.clone(), Trust::Verified));
        }
        match self.db.contact(fingerprint)? {
            Some(contact) => Ok((contact.display_name, contact.trust)),
            None => Ok((fingerprint.chars().take(8).collect(), Trust::Unverified)),
        }
    }

    fn delivery_summary(&self, event_id: &str) -> Result<Option<String>, EngineError> {
        let rows = self.db.deliveries_for(event_id)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let total = rows.len();
        let mut accepted = 0;
        let mut withheld = 0;
        let mut retrying = 0;
        let mut failed = 0;
        for row in &rows {
            match row.state {
                DeliveryState::Accepted => accepted += 1,
                DeliveryState::Withheld => withheld += 1,
                DeliveryState::Retrying | DeliveryState::Queued | DeliveryState::Sending => {
                    retrying += 1
                }
                DeliveryState::Failed => failed += 1,
                DeliveryState::Cancelled => {}
            }
        }
        let mut parts = vec![format!("{accepted}/{total} accepted")];
        if retrying > 0 {
            parts.push(format!("{retrying} pending"));
        }
        if failed > 0 {
            parts.push(format!("{failed} failed"));
        }
        if withheld > 0 {
            parts.push(format!("{withheld} withheld"));
        }
        Ok(Some(parts.join(" · ")))
    }

    pub fn conversation(&self, root: &str) -> Result<Vec<ConversationItem>, EngineError> {
        let rows = self.db.conversation(root)?;
        let mut depths: BTreeMap<String, usize> = BTreeMap::new();
        depths.insert(root.to_string(), 0);
        let mut out = Vec::new();
        for row in rows {
            let depth = row
                .parent
                .as_ref()
                .and_then(|parent| depths.get(parent).copied())
                .map(|parent_depth| (parent_depth + 1).min(3))
                .unwrap_or(0);
            depths.insert(row.event_id.clone(), depth);
            let (author_name, trust) = self.display_author(&row.author)?;
            let text = payload_text(&row.payload);
            out.push(ConversationItem {
                event_id: row.event_id,
                author: row.author,
                author_name,
                trust,
                text,
                created_at: row.created_at,
                depth,
                kind: row.kind,
            });
        }
        Ok(out)
    }

    pub fn replies_to_me(&self, limit: usize) -> Result<Vec<ConversationItem>, EngineError> {
        let me = self.fingerprint();
        let mut out = Vec::new();
        for row in self.db.replies_to(&me, limit)? {
            let (author_name, trust) = self.display_author(&row.author)?;
            out.push(ConversationItem {
                text: payload_text(&row.payload),
                event_id: row.event_id,
                author: row.author,
                author_name,
                trust,
                created_at: row.created_at,
                depth: 0,
                kind: row.kind,
            });
        }
        Ok(out)
    }

    pub fn orphan_replies(&self, limit: usize) -> Result<Vec<EventRow>, EngineError> {
        Ok(self.db.orphan_replies(limit)?)
    }

    pub fn unseen(&self, tab: Tab) -> Result<i64, EngineError> {
        Ok(self.db.unseen_count(tab, &self.fingerprint())?)
    }

    pub fn mark_seen(&mut self, tab: Tab) -> Result<(), EngineError> {
        let max = self.db.max_position()?;
        self.db.mark_seen(tab, &self.fingerprint(), max)?;
        Ok(())
    }

    pub fn set_unread(&mut self, event_id: &str, unread: bool) -> Result<(), EngineError> {
        self.db.set_unread(event_id, unread)?;
        Ok(())
    }

    // ----------------------------------------------------------------- queue

    pub fn queue(&self, limit: usize) -> Result<Vec<QueueItem>, EngineError> {
        let rows = self.db.all_deliveries(limit)?;
        let mut grouped: BTreeMap<String, Vec<DeliveryRow>> = BTreeMap::new();
        for row in rows {
            grouped
                .entry(row.post_event_id.clone())
                .or_default()
                .push(row);
        }
        let mut out = Vec::new();
        for (post_event_id, deliveries) in grouped {
            let preview = match self.db.event(&post_event_id)? {
                Some(event) => {
                    let text = payload_text(&event.payload);
                    text.chars().take(60).collect::<String>()
                }
                None => "(event missing)".to_string(),
            };
            out.push(QueueItem {
                post_event_id,
                preview,
                deliveries,
            });
        }
        Ok(out)
    }

    // --------------------------------------------------------------- bundles

    pub fn export_bundle(&self, path: &Path) -> Result<usize, EngineError> {
        let me = self.fingerprint();
        let mut carriers: Vec<String> = Vec::new();
        for id in self.own_event_ids(&me)? {
            if let Some(row) = self.db.event(&id)? {
                let signed = signed_from_row(&row)?;
                carriers.push(Carrier::plain(signed).to_json()?);
            }
        }
        let body = format!("{{\n  \"kind\": \"social-mail-bundle\",\n  \"version\": 1,\n  \"carriers\": [\n{}\n  ]\n}}", carriers.join(",\n"));
        write_private(path, body.as_bytes())?;
        Ok(carriers.len())
    }

    fn own_event_ids(&self, me: &str) -> Result<Vec<String>, EngineError> {
        Ok(self
            .db
            .events_by_author(me, 1000)?
            .into_iter()
            .map(|row| row.event_id)
            .collect())
    }

    pub fn import_bundle(&mut self, path: &Path) -> Result<Vec<IngestOutcome>, EngineError> {
        let text = fs::read_to_string(path)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let carriers = value
            .get("carriers")
            .and_then(|value| value.as_array())
            .ok_or_else(|| EngineError::Invalid("bundle has no carriers array".into()))?;
        let mut out = Vec::new();
        for carrier in carriers {
            let body = serde_json::to_string(carrier)?;
            out.push(self.ingest(&body)?);
        }
        Ok(out)
    }

    // ----------------------------------------------------------------- lists

    /// Lists the user has added, stored as a JSON array in settings.
    pub fn lists(&self) -> Result<Vec<String>, EngineError> {
        match self.db.setting("lists")? {
            None => Ok(Vec::new()),
            Some(text) => Ok(serde_json::from_str(&text)?),
        }
    }

    pub fn add_list(&mut self, address: &str) -> Result<(), EngineError> {
        let address = address.trim();
        if address.is_empty() || !address.contains('@') {
            return Err(EngineError::Invalid(
                "a list needs an address like name@example.org".into(),
            ));
        }
        let mut lists = self.lists()?;
        if !lists.iter().any(|existing| existing == address) {
            lists.push(address.to_string());
        }
        self.db.set_setting("lists", &serde_json::to_string(&lists)?)?;
        Ok(())
    }

    pub fn remove_list(&mut self, address: &str) -> Result<(), EngineError> {
        let mut lists = self.lists()?;
        lists.retain(|existing| existing != address.trim());
        self.db.set_setting("lists", &serde_json::to_string(&lists)?)?;
        Ok(())
    }

    // -------------------------------------------------------------- settings

    pub fn set_locale(&mut self, locale: &str) -> Result<(), EngineError> {
        if locale != "en" && locale != "fa" {
            return Err(EngineError::Invalid("locale must be en or fa".into()));
        }
        self.config.locale = locale.to_string();
        self.save_config()
    }

    pub fn set_color(&mut self, color: &str) -> Result<(), EngineError> {
        if !matches!(color, "auto" | "always" | "never") {
            return Err(EngineError::Invalid(
                "color must be auto, always or never".into(),
            ));
        }
        self.config.color = color.to_string();
        self.save_config()
    }

    pub fn set_density(&mut self, tab: Tab, density: &str) -> Result<(), EngineError> {
        if !matches!(density, "compact" | "cozy" | "full") {
            return Err(EngineError::Invalid(
                "density must be compact, cozy or full".into(),
            ));
        }
        self.config.set_density(tab.as_str(), density);
        self.save_config()
    }

    pub fn set_mail_folder(&mut self, enabled: bool) -> Result<(), EngineError> {
        self.config.move_protocol_messages = enabled;
        self.save_config()
    }

    pub fn counts(&self) -> Result<Counts, EngineError> {
        Ok(self.db.counts(&self.fingerprint())?)
    }

    pub fn problems(&self) -> Result<Vec<ProblemRow>, EngineError> {
        Ok(self.db.problems()?)
    }

    pub fn dismiss_problem(&mut self, event_id: &str) -> Result<(), EngineError> {
        self.db.clear_problem(event_id)?;
        Ok(())
    }

    // -------------------------------------------------------------- internals

    fn transport(&self) -> Result<SpoolTransport, EngineError> {
        SpoolTransport::new(self.config.spool_dir.clone(), self.fingerprint())
    }

    fn insert_signed(
        &self,
        signed: &SignedEvent,
        status: &str,
        reason: Option<&str>,
        parent: Option<&str>,
    ) -> Result<bool, EngineError> {
        let canonical = String::from_utf8(signed.event.canonical_bytes()?)
            .map_err(|error| EngineError::Invalid(error.to_string()))?;
        let row = EventRow {
            event_id: signed.event_id.clone(),
            author: signed.event.author.clone(),
            kind: signed.event.kind.as_str().to_string(),
            created_at: signed.event.created_at.clone(),
            received_at: now_rfc3339(),
            payload: signed.event.payload_json(),
            signature: signed.signature.clone(),
            canonical,
            status: status.to_string(),
            reason: reason.map(str::to_string),
            parent: parent.map(str::to_string),
        };
        Ok(self.db.insert_event(&row)?)
    }

    fn record_problem(
        &self,
        event_id: &str,
        author: Option<&str>,
        kind: Option<&str>,
        reason: &str,
        detail: &str,
    ) -> Result<(), EngineError> {
        self.db.add_problem(&ProblemRow {
            event_id: event_id.to_string(),
            author: author.map(str::to_string),
            kind: kind.map(str::to_string),
            reason: reason.to_string(),
            detail: if detail.is_empty() {
                None
            } else {
                Some(detail.to_string())
            },
            created_at: now_rfc3339(),
        })?;
        Ok(())
    }
}

fn audience_str(audience: Audience) -> &'static str {
    match audience {
        Audience::Followers => "followers",
        Audience::PublicList => "public-list",
    }
}

fn payload_text(payload: &str) -> String {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|value| {
            value
                .get("text")
                .and_then(|text| text.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn signed_from_row(row: &EventRow) -> Result<SignedEvent, EngineError> {
    let envelope: Envelope = serde_json::from_str(&row.canonical)?;
    Ok(SignedEvent {
        event: envelope,
        event_id: row.event_id.clone(),
        signature: row.signature.clone(),
    })
}

#[derive(Serialize, Deserialize)]
struct IdentityFile {
    kind: String,
    version: u32,
    ed25519_seed: String,
    x25519_secret: String,
}

fn write_identity(dir: &Path, identity: &Identity) -> Result<(), EngineError> {
    fs::create_dir_all(dir)?;
    let (seed, secret) = identity.secrets();
    let file = IdentityFile {
        kind: "social-mail-identity".into(),
        version: 1,
        ed25519_seed: base64::encode(&seed),
        x25519_secret: base64::encode(&secret),
    };
    let text = serde_json::to_string_pretty(&file)?;
    write_private(&dir.join(IDENTITY_FILE), text.as_bytes())
}

fn read_identity(dir: &Path) -> Result<Identity, EngineError> {
    let text = fs::read_to_string(dir.join(IDENTITY_FILE))?;
    let file: IdentityFile = serde_json::from_str(&text)?;
    if file.kind != "social-mail-identity" || file.version != 1 {
        return Err(EngineError::Config("unknown identity file format".into()));
    }
    let seed = base64::decode(&file.ed25519_seed)
        .map_err(|error| EngineError::Invalid(error.to_string()))?;
    let secret = base64::decode(&file.x25519_secret)
        .map_err(|error| EngineError::Invalid(error.to_string()))?;
    if seed.len() != 32 || secret.len() != 32 {
        return Err(EngineError::Config("identity file is truncated".into()));
    }
    let mut seed_bytes = [0u8; 32];
    seed_bytes.copy_from_slice(&seed);
    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&secret);
    Ok(Identity::from_secrets(seed_bytes, secret_bytes))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// Helper for callers that want an absolute data path.
pub fn data_path(engine: &Engine, name: &str) -> PathBuf {
    engine.data_dir().join(name)
}
