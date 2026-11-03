//! Signed contact cards: the unit of out-of-band pairing.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use social_mail_crypto::{Identity, PublicIdentity, Signature};

use crate::json::canonical_json;
use crate::{ProtocolError, PROTOCOL_VERSION};

pub const CARD_KIND: &str = "social-mail-contact-card";
pub const CARD_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CardKeys {
    pub ed25519: String,
    pub x25519: String,
}

/// A small signed file containing a public identity and one delivery route.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContactCard {
    pub kind: String,
    pub version: u32,
    pub fingerprint: String,
    pub display_name: String,
    pub address: String,
    pub keys: CardKeys,
    pub issued_at: String,
    pub signature: String,
}

impl ContactCard {
    pub fn create(
        identity: &Identity,
        display_name: &str,
        address: &str,
        issued_at: &str,
    ) -> Result<Self, ProtocolError> {
        let public = identity.public();
        let (ed25519, x25519) = public.to_b64();
        let mut card = ContactCard {
            kind: CARD_KIND.to_string(),
            version: CARD_VERSION,
            fingerprint: public.fingerprint(),
            display_name: display_name.to_string(),
            address: address.to_string(),
            keys: CardKeys { ed25519, x25519 },
            issued_at: issued_at.to_string(),
            signature: String::new(),
        };
        let unsigned = card.unsigned_value()?;
        let bytes = canonical_json(&unsigned)?.into_bytes();
        card.signature = identity.sign(&bytes).to_hex();
        Ok(card)
    }

    pub fn public_identity(&self) -> Result<PublicIdentity, ProtocolError> {
        PublicIdentity::from_b64(&self.keys.ed25519, &self.keys.x25519)
            .map_err(|error| ProtocolError::Card(error.to_string()))
    }

    fn unsigned_value(&self) -> Result<Value, ProtocolError> {
        let mut value = serde_json::to_value(self)?;
        match value.as_object_mut() {
            Some(map) => {
                map.remove("signature");
            }
            None => return Err(ProtocolError::Card("card is not an object".into())),
        }
        Ok(value)
    }

    /// Verify the self-signature and return the card's public identity.
    pub fn verify(&self) -> Result<PublicIdentity, ProtocolError> {
        if self.kind != CARD_KIND {
            return Err(ProtocolError::Card(format!(
                "unexpected card kind {:?}",
                self.kind
            )));
        }
        if self.version != CARD_VERSION {
            return Err(ProtocolError::Card(format!(
                "unsupported card version {}",
                self.version
            )));
        }
        if self.display_name.len() > 120 {
            return Err(ProtocolError::Card("display name too long".into()));
        }
        if self.address.trim().is_empty() || self.address.len() > 320 {
            return Err(ProtocolError::Card("invalid delivery address".into()));
        }
        if chrono::DateTime::parse_from_rfc3339(&self.issued_at).is_err() {
            return Err(ProtocolError::InvalidTimestamp(self.issued_at.clone()));
        }
        let public = self.public_identity()?;
        if public.fingerprint() != self.fingerprint {
            return Err(ProtocolError::Card(
                "card fingerprint does not match its keys".into(),
            ));
        }
        let signature = Signature::from_hex(&self.signature)
            .map_err(|error| ProtocolError::Card(error.to_string()))?;
        let bytes = canonical_json(&self.unsigned_value()?)?.into_bytes();
        if !public.verify(&bytes, &signature) {
            return Err(ProtocolError::BadSignature);
        }
        Ok(public)
    }

    pub fn to_pretty_json(&self) -> Result<String, ProtocolError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(text: &str) -> Result<Self, ProtocolError> {
        let card: ContactCard = serde_json::from_str(text).map_err(|error| {
            ProtocolError::Card(format!("could not read card: {error}"))
        })?;
        card.verify()?;
        Ok(card)
    }
}

/// Protocol version re-exported for callers that build cards by hand.
pub const fn protocol_version() -> u32 {
    PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> (Identity, ContactCard) {
        let identity = Identity::from_secrets([11u8; 32], [12u8; 32]);
        let card = ContactCard::create(
            &identity,
            "Alice",
            "alice@example.org",
            "2026-11-03T09:00:00Z",
        )
        .unwrap();
        (identity, card)
    }

    #[test]
    fn verifies_and_carries_identity() {
        let (identity, card) = card();
        let public = card.verify().unwrap();
        assert_eq!(public, identity.public());
        assert_eq!(card.fingerprint, identity.fingerprint());
    }

    #[test]
    fn rejects_tampering() {
        let (_, mut card) = card();
        card.display_name = "Mallory".to_string();
        assert!(matches!(card.verify(), Err(ProtocolError::BadSignature)));

        let (_, mut card) = card();
        card.address = "mallory@example.org".to_string();
        assert!(card.verify().is_err());

        let (_, mut card) = card();
        card.fingerprint = "0".repeat(40);
        assert!(card.verify().is_err());
    }

    #[test]
    fn rejects_wrong_kind_and_version() {
        let (_, mut card) = card();
        card.kind = "something-else".into();
        assert!(card.verify().is_err());

        let (_, mut card) = card();
        card.version = 99;
        assert!(card.verify().is_err());
    }

    #[test]
    fn round_trips_through_json() {
        let (_, card) = card();
        let text = card.to_pretty_json().unwrap();
        let parsed = ContactCard::from_json(&text).unwrap();
        assert_eq!(parsed, card);
    }

    #[test]
    fn rejects_broken_json() {
        assert!(ContactCard::from_json("{").is_err());
        assert!(ContactCard::from_json("{}").is_err());
    }
}
