//! Row types shared by the storage layer and the engine.

use std::str::FromStr;

use crate::StorageError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    Unverified,
    Verified,
    Blocked,
}

impl Trust {
    pub fn as_str(self) -> &'static str {
        match self {
            Trust::Unverified => "unverified",
            Trust::Verified => "verified",
            Trust::Blocked => "blocked",
        }
    }

    pub fn is_blocked(self) -> bool {
        matches!(self, Trust::Blocked)
    }

    pub fn is_verified(self) -> bool {
        matches!(self, Trust::Verified)
    }
}

impl FromStr for Trust {
    type Err = StorageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unverified" => Ok(Trust::Unverified),
            "verified" => Ok(Trust::Verified),
            "blocked" => Ok(Trust::Blocked),
            other => Err(StorageError::Data(format!("unknown trust state {other:?}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    pub fingerprint: String,
    pub display_name: String,
    pub address: String,
    pub ed25519: String,
    pub x25519: String,
    pub trust: Trust,
    pub muted: bool,
    pub added_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileRow {
    pub fingerprint: String,
    pub display_name: String,
    pub address: String,
    pub locale: String,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowState {
    Pending,
    Following,
    Declined,
    Removed,
}

impl FollowState {
    pub fn as_str(self) -> &'static str {
        match self {
            FollowState::Pending => "pending",
            FollowState::Following => "following",
            FollowState::Declined => "declined",
            FollowState::Removed => "removed",
        }
    }
}

impl FromStr for FollowState {
    type Err = StorageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(FollowState::Pending),
            "following" => Ok(FollowState::Following),
            "declined" => Ok(FollowState::Declined),
            "removed" => Ok(FollowState::Removed),
            other => Err(StorageError::Data(format!("unknown follow state {other:?}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FollowRow {
    pub follower: String,
    pub followee: String,
    pub state: FollowState,
    pub verified: bool,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    pub event_id: String,
    pub author: String,
    pub kind: String,
    pub created_at: String,
    pub received_at: String,
    pub payload: String,
    pub signature: String,
    pub canonical: String,
    pub status: String,
    pub reason: Option<String>,
    pub parent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineRow {
    pub event_id: String,
    pub position: i64,
    pub author: String,
    pub kind: String,
    pub audience: String,
    pub list_id: Option<String>,
    pub parent: Option<String>,
    pub seen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Following,
    Lists,
    You,
    All,
}

impl Tab {
    pub fn as_str(self) -> &'static str {
        match self {
            Tab::Following => "following",
            Tab::Lists => "lists",
            Tab::You => "you",
            Tab::All => "all",
        }
    }

    pub fn all() -> [Tab; 4] {
        [Tab::Following, Tab::Lists, Tab::You, Tab::All]
    }
}

impl FromStr for Tab {
    type Err = StorageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "following" => Ok(Tab::Following),
            "lists" => Ok(Tab::Lists),
            "you" => Ok(Tab::You),
            "all" => Ok(Tab::All),
            other => Err(StorageError::Data(format!("unknown tab {other:?}"))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryState {
    Queued,
    Sending,
    Accepted,
    Retrying,
    Failed,
    Cancelled,
    Withheld,
}

impl DeliveryState {
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryState::Queued => "queued",
            DeliveryState::Sending => "sending",
            DeliveryState::Accepted => "accepted",
            DeliveryState::Retrying => "retrying",
            DeliveryState::Failed => "failed",
            DeliveryState::Cancelled => "cancelled",
            DeliveryState::Withheld => "withheld",
        }
    }

    /// Plain-language rendering shared by the TUI and the CLI (D31).
    pub fn label(self) -> &'static str {
        match self {
            DeliveryState::Queued => "Queued",
            DeliveryState::Sending => "Sending",
            DeliveryState::Accepted => "Accepted by your mail server",
            DeliveryState::Retrying => "Retrying",
            DeliveryState::Failed => "Failed - needs action",
            DeliveryState::Cancelled => "Cancelled",
            DeliveryState::Withheld => "Not sent - unverified",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            DeliveryState::Accepted | DeliveryState::Cancelled | DeliveryState::Withheld
        )
    }
}

impl FromStr for DeliveryState {
    type Err = StorageError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(DeliveryState::Queued),
            "sending" => Ok(DeliveryState::Sending),
            "accepted" => Ok(DeliveryState::Accepted),
            "retrying" => Ok(DeliveryState::Retrying),
            "failed" => Ok(DeliveryState::Failed),
            "cancelled" => Ok(DeliveryState::Cancelled),
            "withheld" => Ok(DeliveryState::Withheld),
            other => Err(StorageError::Data(format!(
                "unknown delivery state {other:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryRow {
    pub post_event_id: String,
    pub recipient: String,
    pub state: DeliveryState,
    pub reason: Option<String>,
    pub attempts: i64,
    pub next_attempt_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProblemRow {
    pub event_id: String,
    pub author: Option<String>,
    pub kind: Option<String>,
    pub reason: String,
    pub detail: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub events: i64,
    pub roots: i64,
    pub replies: i64,
    pub contacts: i64,
    pub verified_contacts: i64,
    pub queued_deliveries: i64,
    pub failed_deliveries: i64,
    pub withheld_deliveries: i64,
    pub problems: i64,
    pub unseen_following: i64,
}
