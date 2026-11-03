//! The `social-mail` event protocol: canonical JSON, event ids, signatures,
//! typed payloads, and the transport carrier.

pub mod card;
mod error;
pub mod event;
pub mod json;

pub use card::{CardKeys, ContactCard, CARD_KIND, CARD_VERSION};
pub use error::ProtocolError;
pub use event::{
    now_rfc3339, parse_timestamp, random_nonce, sha256_hex, Audience, Carrier, Envelope,
    EventKind, FollowAcceptPayload, FollowRemovePayload, FollowRequestPayload, PostPayload,
    ProfilePayload, ReplyPayload, RouteUpdatePayload, SignedEvent, CARRIER_KIND,
};
pub use json::canonical_json;

/// Protocol name carried in every envelope.
pub const PROTOCOL_NAME: &str = "social-mail";
/// Protocol version carried in every envelope.
pub const PROTOCOL_VERSION: u32 = 1;
/// Hard ceiling for a canonical event.
pub const MAX_EVENT_BYTES: usize = 256 * 1024;
/// Hard ceiling for post text.
pub const MAX_TEXT_BYTES: usize = 20_000;
/// Maximum nesting depth accepted by the canonicalizer.
pub const MAX_PAYLOAD_DEPTH: usize = 16;
