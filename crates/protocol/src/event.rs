//! Event envelope, typed payloads, signing and verification.

use std::str::FromStr;

use chrono::{DateTime, SecondsFormat, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use social_mail_crypto::{hex, Identity, PublicIdentity, Signature};

use crate::card::ContactCard;
use crate::json::canonical_json;
use crate::{ProtocolError, MAX_EVENT_BYTES, MAX_TEXT_BYTES, PROTOCOL_NAME, PROTOCOL_VERSION};

pub const CARRIER_KIND: &str = "social-mail-event";
pub const CARRIER_VERSION: u32 = 1;
pub const FINGERPRINT_LEN: usize = 40;
pub const MAX_LIST_ADDRESS_LEN: usize = 320;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    Profile,
    FollowRequest,
    FollowAccept,
    FollowRemove,
    RouteUpdate,
    Post,
    Reply,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Profile => "profile",
            EventKind::FollowRequest => "follow-request",
            EventKind::FollowAccept => "follow-accept",
            EventKind::FollowRemove => "follow-remove",
            EventKind::RouteUpdate => "route-update",
            EventKind::Post => "post",
            EventKind::Reply => "reply",
        }
    }
}

impl FromStr for EventKind {
    type Err = ProtocolError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "profile" => Ok(EventKind::Profile),
            "follow-request" => Ok(EventKind::FollowRequest),
            "follow-accept" => Ok(EventKind::FollowAccept),
            "follow-remove" => Ok(EventKind::FollowRemove),
            "route-update" => Ok(EventKind::RouteUpdate),
            "post" => Ok(EventKind::Post),
            "reply" => Ok(EventKind::Reply),
            other => Err(ProtocolError::InvalidPayload(format!(
                "unknown event type {other:?}"
            ))),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    pub protocol: String,
    pub version: u32,
    #[serde(rename = "type")]
    pub kind: EventKind,
    pub author: String,
    pub nonce: String,
    pub created_at: String,
    pub payload: Value,
}

impl Envelope {
    pub fn new(kind: EventKind, author: impl Into<String>, payload: Value) -> Self {
        Envelope {
            protocol: PROTOCOL_NAME.to_string(),
            version: PROTOCOL_VERSION,
            kind,
            author: author.into(),
            nonce: random_nonce(),
            created_at: now_rfc3339(),
            payload,
        }
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        let value = serde_json::to_value(self)?;
        let text = canonical_json(&value)?;
        if text.len() > MAX_EVENT_BYTES {
            return Err(ProtocolError::TooLarge(text.len()));
        }
        Ok(text.into_bytes())
    }

    pub fn payload_as<T: for<'de> Deserialize<'de>>(&self) -> Result<T, ProtocolError> {
        serde_json::from_value(self.payload.clone())
            .map_err(|error| ProtocolError::InvalidPayload(error.to_string()))
    }

    pub fn payload_json(&self) -> String {
        serde_json::to_string(&self.payload).unwrap_or_else(|_| "null".to_string())
    }

    /// Structural validation. Signature verification happens in
    /// [`SignedEvent::verify`], which needs the author's public key.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol != PROTOCOL_NAME {
            return Err(ProtocolError::UnsupportedProtocol(self.protocol.clone()));
        }
        if self.version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(self.version));
        }
        if self.author.len() != FINGERPRINT_LEN
            || !self
                .author
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        {
            return Err(ProtocolError::InvalidFingerprint(self.author.clone()));
        }
        if self.nonce.len() < 16
            || self.nonce.len() > 128
            || !self.nonce.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(ProtocolError::InvalidNonce);
        }
        if DateTime::parse_from_rfc3339(&self.created_at).is_err() {
            return Err(ProtocolError::InvalidTimestamp(self.created_at.clone()));
        }
        // Size guard before any payload work.
        let _ = self.canonical_bytes()?;
        match self.kind {
            EventKind::Post => self.payload_as::<PostPayload>()?.validate(),
            EventKind::Reply => self.payload_as::<ReplyPayload>()?.validate(),
            EventKind::Profile => self.payload_as::<ProfilePayload>()?.validate(),
            EventKind::FollowRequest => self.payload_as::<FollowRequestPayload>()?.validate(),
            EventKind::FollowAccept => self.payload_as::<FollowAcceptPayload>()?.validate(),
            EventKind::FollowRemove => self.payload_as::<FollowRemovePayload>()?.validate(),
            EventKind::RouteUpdate => self.payload_as::<RouteUpdatePayload>()?.validate(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Audience {
    #[default]
    Followers,
    PublicList,
}

pub fn validate_common_text(text: &str) -> Result<(), ProtocolError> {
    if text.trim().is_empty() {
        return Err(ProtocolError::InvalidPayload("text is empty".into()));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(ProtocolError::InvalidPayload(format!(
            "text is {} bytes, limit is {MAX_TEXT_BYTES}",
            text.len()
        )));
    }
    Ok(())
}

pub fn validate_lang(lang: &Option<String>) -> Result<(), ProtocolError> {
    if let Some(lang) = lang {
        if lang.is_empty()
            || lang.len() > 35
            || !lang
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(ProtocolError::InvalidPayload(format!(
                "invalid language tag {lang:?}"
            )));
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PostPayload {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default)]
    pub audience: Audience,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<String>,
}

impl PostPayload {
    pub fn new(text: impl Into<String>) -> Self {
        PostPayload {
            text: text.into(),
            lang: None,
            audience: Audience::Followers,
            list: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_common_text(&self.text)?;
        validate_lang(&self.lang)?;
        match self.audience {
            Audience::Followers => {
                if self.list.is_some() {
                    return Err(ProtocolError::InvalidPayload(
                        "a followers post cannot name a list".into(),
                    ));
                }
            }
            Audience::PublicList => match &self.list {
                None => {
                    return Err(ProtocolError::InvalidPayload(
                        "a public-list post needs a list identifier".into(),
                    ))
                }
                Some(list) => {
                    if list.trim().is_empty() || list.len() > MAX_LIST_ADDRESS_LEN {
                        return Err(ProtocolError::InvalidPayload(
                            "invalid list identifier".into(),
                        ));
                    }
                }
            },
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReplyPayload {
    pub parent: String,
    #[serde(flatten)]
    pub post: PostPayload,
}

impl ReplyPayload {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.parent.len() != 64 || !self.parent.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ProtocolError::InvalidPayload(
                "parent is not a 64-character hex event id".into(),
            ));
        }
        self.post.validate()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProfilePayload {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl ProfilePayload {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.display_name.trim().is_empty() || self.display_name.chars().count() > 80 {
            return Err(ProtocolError::InvalidPayload(
                "display name must be 1-80 characters".into(),
            ));
        }
        validate_lang(&self.lang)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FollowRequestPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub card: ContactCard,
}

impl FollowRequestPayload {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(message) = &self.message {
            if message.len() > 500 {
                return Err(ProtocolError::InvalidPayload(
                    "request message is longer than 500 bytes".into(),
                ));
            }
        }
        self.card.verify()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FollowAcceptPayload {
    pub requester: String,
    pub card: ContactCard,
}

impl FollowAcceptPayload {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.requester.len() != FINGERPRINT_LEN
            || !self.requester.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(ProtocolError::InvalidPayload(
                "requester is not a fingerprint".into(),
            ));
        }
        self.card.verify()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct FollowRemovePayload {}

impl FollowRemovePayload {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteUpdatePayload {
    pub address: String,
}

impl RouteUpdatePayload {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.address.trim().is_empty() || self.address.len() > MAX_LIST_ADDRESS_LEN {
            return Err(ProtocolError::InvalidPayload(
                "invalid delivery address".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignedEvent {
    pub event: Envelope,
    pub event_id: String,
    pub signature: String,
}

impl SignedEvent {
    pub fn sign(event: Envelope, identity: &Identity) -> Result<Self, ProtocolError> {
        event.validate()?;
        if identity.fingerprint() != event.author {
            return Err(ProtocolError::WrongAuthor);
        }
        let bytes = event.canonical_bytes()?;
        let event_id = sha256_hex(&bytes);
        let signature = identity.sign(&bytes).to_hex();
        Ok(SignedEvent {
            event,
            event_id,
            signature,
        })
    }

    pub fn verify(&self, author: &PublicIdentity) -> Result<(), ProtocolError> {
        self.event.validate()?;
        if author.fingerprint() != self.event.author {
            return Err(ProtocolError::WrongAuthor);
        }
        let bytes = self.event.canonical_bytes()?;
        let expected = sha256_hex(&bytes);
        if !expected.eq_ignore_ascii_case(&self.event_id) {
            return Err(ProtocolError::EventIdMismatch);
        }
        let signature =
            Signature::from_hex(&self.signature).map_err(|_| ProtocolError::BadSignature)?;
        if !author.verify(&bytes, &signature) {
            return Err(ProtocolError::BadSignature);
        }
        Ok(())
    }

    pub fn carrier(self) -> Carrier {
        Carrier {
            kind: CARRIER_KIND.to_string(),
            version: CARRIER_VERSION,
            event_id: self.event_id,
            signature: self.signature,
            event: self.event,
            card: None,
        }
    }
}

/// The transport object: a signed event plus, optionally, the author's card.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Carrier {
    pub kind: String,
    pub version: u32,
    pub event_id: String,
    pub signature: String,
    pub event: Envelope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<ContactCard>,
}

impl Carrier {
    pub fn from_signed(signed: SignedEvent) -> Self {
        signed.carrier()
    }

    pub fn with_card(mut self, card: ContactCard) -> Self {
        self.card = Some(card);
        self
    }

    pub fn signed(&self) -> SignedEvent {
        SignedEvent {
            event: self.event.clone(),
            event_id: self.event_id.clone(),
            signature: self.signature.clone(),
        }
    }

    pub fn verify_with(&self, author: &PublicIdentity) -> Result<(), ProtocolError> {
        self.signed().verify(author)?;
        if let Some(card) = &self.card {
            let public = card.verify()?;
            if public.fingerprint() != self.event.author {
                return Err(ProtocolError::WrongAuthor);
            }
        }
        Ok(())
    }

    /// The verified embedded card, if this carrier advertises one.
    pub fn embedded_card(&self) -> Result<Option<ContactCard>, ProtocolError> {
        match &self.card {
            None => Ok(None),
            Some(card) => {
                card.verify()?;
                if card.fingerprint != self.event.author {
                    return Err(ProtocolError::WrongAuthor);
                }
                Ok(Some(card.clone()))
            }
        }
    }

    pub fn to_json(&self) -> Result<String, ProtocolError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_pretty_json(&self) -> Result<String, ProtocolError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(text: &str) -> Result<Self, ProtocolError> {
        if text.len() > MAX_EVENT_BYTES * 2 {
            return Err(ProtocolError::TooLarge(text.len()));
        }
        let carrier: Carrier = serde_json::from_str(text)?;
        if carrier.kind != CARRIER_KIND {
            return Err(ProtocolError::InvalidPayload(format!(
                "unexpected carrier kind {:?}",
                carrier.kind
            )));
        }
        if carrier.version != CARRIER_VERSION {
            return Err(ProtocolError::UnsupportedVersion(carrier.version));
        }
        Ok(carrier)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::to_hex(&hasher.finalize())
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn random_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::to_hex(&bytes)
}

/// Convenience alias used by callers that only need the parsed timestamp.
pub fn parse_timestamp(text: &str) -> Result<DateTime<Utc>, ProtocolError> {
    DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| ProtocolError::InvalidTimestamp(text.to_string()))
}
