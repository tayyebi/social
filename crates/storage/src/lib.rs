//! SQLite storage: schema, migrations and typed queries.
//!
//! Design notes:
//!
//! * Transport messages and application events are separate concepts; this crate
//!   stores verified events, not raw mail.
//! * `timeline.position` is assigned once, inside a transaction, and never
//!   updated (D32). Restart or re-sync can therefore never reorder the feed.
//! * Trust, mute and seen state are local; they never alter verification results.

mod error;
pub mod model;

pub use error::StorageError;
pub use model::*;

use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};

const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS profiles (
    fingerprint TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    address TEXT NOT NULL,
    locale TEXT NOT NULL DEFAULT 'en',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contacts (
    fingerprint TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    address TEXT NOT NULL,
    ed25519 TEXT NOT NULL,
    x25519 TEXT NOT NULL,
    trust TEXT NOT NULL,
    muted INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS follows (
    follower TEXT NOT NULL,
    followee TEXT NOT NULL,
    state TEXT NOT NULL,
    verified INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (follower, followee)
);

CREATE TABLE IF NOT EXISTS events (
    event_id TEXT PRIMARY KEY,
    author TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at TEXT NOT NULL,
    received_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    signature TEXT NOT NULL,
    canonical TEXT NOT NULL,
    status TEXT NOT NULL,
    reason TEXT,
    parent TEXT,
    parent_author TEXT
);

CREATE TABLE IF NOT EXISTS timeline (
    event_id TEXT PRIMARY KEY,
    position INTEGER NOT NULL UNIQUE,
    author TEXT NOT NULL,
    kind TEXT NOT NULL,
    audience TEXT NOT NULL,
    list_id TEXT,
    is_root INTEGER NOT NULL DEFAULT 1,
    seen INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS cursors (
    tab TEXT PRIMARY KEY,
    last_position INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS deliveries (
    post_event_id TEXT NOT NULL,
    recipient TEXT NOT NULL,
    state TEXT NOT NULL,
    reason TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (post_event_id, recipient)
);

CREATE TABLE IF NOT EXISTS problems (
    event_id TEXT PRIMARY KEY,
    author TEXT,
    kind TEXT,
    reason TEXT NOT NULL,
    detail TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS events_parent_idx ON events(parent);
CREATE INDEX IF NOT EXISTS events_author_idx ON events(author);
CREATE INDEX IF NOT EXISTS timeline_position_idx ON timeline(position);
"#;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        Self::from_conn(conn)
    }

    pub fn open_memory() -> Result<Self, StorageError> {
        Self::from_conn(Connection::open_in_memory()?)
    }

    fn from_conn(conn: Connection) -> Result<Self, StorageError> {
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        conn.execute_batch("PRAGMA busy_timeout=5000;")?;
        let db = Database { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn migrate(&self) -> Result<(), StorageError> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            self.conn.execute_batch(SCHEMA_V1)?;
            self.conn
                .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64, StorageError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    // ---------------------------------------------------------------- profile

    pub fn set_profile(&self, profile: &ProfileRow) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO profiles (fingerprint, display_name, address, locale, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(fingerprint) DO UPDATE SET
                display_name = excluded.display_name,
                address = excluded.address,
                locale = excluded.locale",
            params![
                profile.fingerprint,
                profile.display_name,
                profile.address,
                profile.locale,
                profile.created_at
            ],
        )?;
        Ok(())
    }

    pub fn profile(&self) -> Result<Option<ProfileRow>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT fingerprint, display_name, address, locale, created_at FROM profiles LIMIT 1",
                [],
                |row| {
                    Ok(ProfileRow {
                        fingerprint: row.get(0)?,
                        display_name: row.get(1)?,
                        address: row.get(2)?,
                        locale: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    // --------------------------------------------------------------- contacts

    pub fn upsert_contact(&self, contact: &Contact) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO contacts
                (fingerprint, display_name, address, ed25519, x25519, trust, muted, added_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(fingerprint) DO UPDATE SET
                display_name = excluded.display_name,
                address = excluded.address,
                ed25519 = excluded.ed25519,
                x25519 = excluded.x25519,
                updated_at = excluded.updated_at",
            params![
                contact.fingerprint,
                contact.display_name,
                contact.address,
                contact.ed25519,
                contact.x25519,
                contact.trust.as_str(),
                i64::from(contact.muted),
                contact.added_at,
                contact.updated_at
            ],
        )?;
        Ok(())
    }

    fn contact_from_row(row: &Row<'_>) -> rusqlite::Result<Contact> {
        let trust: String = row.get(5)?;
        let muted: i64 = row.get(6)?;
        Ok(Contact {
            fingerprint: row.get(0)?,
            display_name: row.get(1)?,
            address: row.get(2)?,
            ed25519: row.get(3)?,
            x25519: row.get(4)?,
            trust: trust.parse().unwrap_or(Trust::Unverified),
            muted: muted != 0,
            added_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }

    pub fn contact(&self, fingerprint: &str) -> Result<Option<Contact>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT fingerprint, display_name, address, ed25519, x25519, trust, muted, added_at, updated_at
                 FROM contacts WHERE fingerprint = ?1",
                params![fingerprint],
                Self::contact_from_row,
            )
            .optional()?)
    }

    /// Contact whose delivery address equals `address`.
    ///
    /// Used to find a public list's operator when a post is addressed to the
    /// list instead of to a fingerprint.
    pub fn contact_by_address(&self, address: &str) -> Result<Option<Contact>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT fingerprint, display_name, address, ed25519, x25519, trust, muted, added_at, updated_at
                 FROM contacts WHERE address = ?1 COLLATE NOCASE LIMIT 1",
                params![address],
                Self::contact_from_row,
            )
            .optional()?)
    }

    pub fn contacts(&self) -> Result<Vec<Contact>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT fingerprint, display_name, address, ed25519, x25519, trust, muted, added_at, updated_at
             FROM contacts ORDER BY display_name COLLATE NOCASE, fingerprint",
        )?;
        let rows = statement.query_map([], Self::contact_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn set_trust(&self, fingerprint: &str, trust: Trust) -> Result<(), StorageError> {
        let changed = self.conn.execute(
            "UPDATE contacts SET trust = ?2, updated_at = ?3 WHERE fingerprint = ?1",
            params![fingerprint, trust.as_str(), Utc::now().to_rfc3339()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound(fingerprint.to_string()));
        }
        Ok(())
    }

    pub fn set_muted(&self, fingerprint: &str, muted: bool) -> Result<(), StorageError> {
        let changed = self.conn.execute(
            "UPDATE contacts SET muted = ?2, updated_at = ?3 WHERE fingerprint = ?1",
            params![fingerprint, i64::from(muted), Utc::now().to_rfc3339()],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound(fingerprint.to_string()));
        }
        Ok(())
    }

    // ---------------------------------------------------------------- follows

    pub fn upsert_follow(
        &self,
        follower: &str,
        followee: &str,
        state: FollowState,
        verified: bool,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO follows (follower, followee, state, verified, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(follower, followee) DO UPDATE SET
                state = excluded.state,
                verified = excluded.verified,
                updated_at = excluded.updated_at",
            params![
                follower,
                followee,
                state.as_str(),
                i64::from(verified),
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    fn follow_from_row(row: &Row<'_>) -> rusqlite::Result<FollowRow> {
        let state: String = row.get(2)?;
        let verified: i64 = row.get(3)?;
        Ok(FollowRow {
            follower: row.get(0)?,
            followee: row.get(1)?,
            state: state.parse().unwrap_or(FollowState::Pending),
            verified: verified != 0,
            updated_at: row.get(4)?,
        })
    }

    pub fn follow(
        &self,
        follower: &str,
        followee: &str,
    ) -> Result<Option<FollowRow>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT follower, followee, state, verified, updated_at FROM follows
                 WHERE follower = ?1 AND followee = ?2",
                params![follower, followee],
                Self::follow_from_row,
            )
            .optional()?)
    }

    pub fn follows_of(&self, follower: &str) -> Result<Vec<FollowRow>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT follower, followee, state, verified, updated_at FROM follows
             WHERE follower = ?1 ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map(params![follower], Self::follow_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn followers_of(&self, followee: &str) -> Result<Vec<FollowRow>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT follower, followee, state, verified, updated_at FROM follows
             WHERE followee = ?1 ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map(params![followee], Self::follow_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ----------------------------------------------------------------- events

    /// Insert an event idempotently. Returns `true` when it was new.
    pub fn insert_event(&self, event: &EventRow) -> Result<bool, StorageError> {
        let changed = self.conn.execute(
            "INSERT INTO events
                (event_id, author, kind, created_at, received_at, payload, signature, canonical, status, reason, parent, parent_author)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(event_id) DO NOTHING",
            params![
                event.event_id,
                event.author,
                event.kind,
                event.created_at,
                event.received_at,
                event.payload,
                event.signature,
                event.canonical,
                event.status,
                event.reason,
                event.parent,
                Option::<String>::None
            ],
        )?;
        Ok(changed > 0)
    }

    fn event_from_row(row: &Row<'_>) -> rusqlite::Result<EventRow> {
        Ok(EventRow {
            event_id: row.get(0)?,
            author: row.get(1)?,
            kind: row.get(2)?,
            created_at: row.get(3)?,
            received_at: row.get(4)?,
            payload: row.get(5)?,
            signature: row.get(6)?,
            canonical: row.get(7)?,
            status: row.get(8)?,
            reason: row.get(9)?,
            parent: row.get(10)?,
        })
    }

    const EVENT_COLUMNS: &'static str =
        "event_id, author, kind, created_at, received_at, payload, signature, canonical, status, reason, parent";

    pub fn event(&self, event_id: &str) -> Result<Option<EventRow>, StorageError> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {} FROM events WHERE event_id = ?1",
                    Self::EVENT_COLUMNS
                ),
                params![event_id],
                Self::event_from_row,
            )
            .optional()?)
    }

    pub fn event_count(&self) -> Result<i64, StorageError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?)
    }

    /// Set the resolved author of the parent event (used for "replies to you").
    pub fn set_parent_author(
        &self,
        event_id: &str,
        parent_author: &str,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "UPDATE events SET parent_author = ?2 WHERE event_id = ?1",
            params![event_id, parent_author],
        )?;
        Ok(())
    }

    /// Walk a thread: the root plus every descendant, in author-time order.
    pub fn conversation(&self, root: &str) -> Result<Vec<EventRow>, StorageError> {
        // Ordered by depth first so the root always leads its thread, then by
        // author time. Without the depth key, equal timestamps would put a reply
        // before its parent.
        let mut statement = self.conn.prepare(
            "WITH RECURSIVE thread(event_id, depth) AS (
                 SELECT event_id, 0 FROM events WHERE event_id = ?1
                 UNION ALL
                 SELECT e.event_id, thread.depth + 1
                 FROM events e JOIN thread ON e.parent = thread.event_id
                 WHERE thread.depth < 8
             )
             SELECT e.event_id, e.author, e.kind, e.created_at, e.received_at,
                    e.payload, e.signature, e.canonical, e.status, e.reason, e.parent
             FROM events e JOIN thread t ON e.event_id = t.event_id
             ORDER BY t.depth ASC, e.created_at ASC, e.event_id ASC",
        )?;
        let rows = statement.query_map(params![root], Self::event_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn direct_reply_count(&self, parent: &str) -> Result<i64, StorageError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE parent = ?1",
            params![parent],
            |row| row.get(0),
        )?)
    }

    /// Replies addressed to me, newest first.
    pub fn replies_to(&self, me: &str, limit: usize) -> Result<Vec<EventRow>, StorageError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM events WHERE kind = 'reply' AND parent_author = ?1
             ORDER BY received_at DESC LIMIT ?2",
            Self::EVENT_COLUMNS
        ))?;
        let rows = statement.query_map(params![me, limit as i64], Self::event_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Replies whose parent has not arrived yet.
    pub fn orphan_replies(&self, limit: usize) -> Result<Vec<EventRow>, StorageError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM events e
             WHERE e.kind = 'reply'
               AND e.parent IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM events p WHERE p.event_id = e.parent)
             ORDER BY e.received_at DESC LIMIT ?1",
            Self::EVENT_COLUMNS
        ))?;
        let rows = statement.query_map(params![limit as i64], Self::event_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // --------------------------------------------------------------- timeline

    /// Assign the next immutable position and insert a root into the feed.
    pub fn append_root(
        &mut self,
        event_id: &str,
        author: &str,
        kind: &str,
        audience: &str,
        list_id: Option<&str>,
    ) -> Result<i64, StorageError> {
        let tx = self.conn.transaction()?;
        let position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM timeline",
            [],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO timeline (event_id, position, author, kind, audience, list_id, is_root, seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 0)
             ON CONFLICT(event_id) DO NOTHING",
            params![event_id, position, author, kind, audience, list_id],
        )?;
        tx.commit()?;
        Ok(position)
    }

    pub fn position_of(&self, event_id: &str) -> Result<Option<i64>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT position FROM timeline WHERE event_id = ?1",
                params![event_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn tab_clause(tab: Tab) -> &'static str {
        match tab {
            // Root posts by people I follow, excluding my own.
            Tab::Following => {
                " AND t.audience = 'followers' AND t.author <> ?1
                  AND EXISTS (SELECT 1 FROM follows f WHERE f.follower = ?1 AND f.followee = t.author AND f.state = 'following')"
            }
            Tab::Lists => " AND t.audience = 'public-list'",
            Tab::You => " AND t.author = ?1",
            Tab::All => {
                " AND (t.author = ?1 OR t.audience = 'public-list'
                   OR EXISTS (SELECT 1 FROM follows f WHERE f.follower = ?1 AND f.followee = t.author AND f.state = 'following'))"
            }
        }
    }

    pub fn timeline(
        &self,
        tab: Tab,
        me: &str,
        limit: usize,
        before_position: Option<i64>,
    ) -> Result<Vec<TimelineRow>, StorageError> {
        let sql = format!(
            "SELECT t.event_id, t.position, t.author, t.kind, t.audience, t.list_id, e.parent, t.seen
             FROM timeline t JOIN events e ON e.event_id = t.event_id
             WHERE t.is_root = 1
               AND NOT EXISTS (
                   SELECT 1 FROM contacts c
                   WHERE c.fingerprint = t.author AND (c.muted = 1 OR c.trust = 'blocked')
               )
               {} 
               AND (?2 IS NULL OR t.position < ?2)
             ORDER BY t.position DESC
             LIMIT ?3",
            Self::tab_clause(tab)
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params![me, before_position, limit as i64], |row| {
            let seen: i64 = row.get(7)?;
            Ok(TimelineRow {
                event_id: row.get(0)?,
                position: row.get(1)?,
                author: row.get(2)?,
                kind: row.get(3)?,
                audience: row.get(4)?,
                list_id: row.get(5)?,
                parent: row.get(6)?,
                seen: seen != 0,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn unseen_count(&self, tab: Tab, me: &str) -> Result<i64, StorageError> {
        let sql = format!(
            "SELECT COUNT(*) FROM timeline t
             WHERE t.is_root = 1 AND t.seen = 0
               AND NOT EXISTS (
                   SELECT 1 FROM contacts c
                   WHERE c.fingerprint = t.author AND (c.muted = 1 OR c.trust = 'blocked')
               )
               {}",
            Self::tab_clause(tab)
        );
        Ok(self.conn.query_row(&sql, params![me], |row| row.get(0))?)
    }

    pub fn max_position(&self) -> Result<i64, StorageError> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(position), 0) FROM timeline",
            [],
            |row| row.get(0),
        )?)
    }

    /// Mark everything at or below `position` as seen, for one tab's filter.
    pub fn mark_seen(&mut self, tab: Tab, me: &str, position: i64) -> Result<(), StorageError> {
        let tx = self.conn.transaction()?;
        let sql = format!(
            "UPDATE timeline AS t SET seen = 1
             WHERE t.is_root = 1 AND t.seen = 0 AND t.position <= ?2
               AND NOT EXISTS (
                   SELECT 1 FROM contacts c
                   WHERE c.fingerprint = t.author AND (c.muted = 1 OR c.trust = 'blocked')
               )
               {}",
            Self::tab_clause(tab)
        );
        tx.execute(&sql, params![me, position])?;
        tx.execute(
            "INSERT INTO cursors (tab, last_position) VALUES (?1, ?2)
             ON CONFLICT(tab) DO UPDATE SET last_position = MAX(last_position, excluded.last_position)",
            params![tab.as_str(), position],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn cursor(&self, tab: Tab) -> Result<i64, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT last_position FROM cursors WHERE tab = ?1",
                params![tab.as_str()],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn set_unread(&self, event_id: &str, unread: bool) -> Result<(), StorageError> {
        self.conn.execute(
            "UPDATE timeline SET seen = ?2 WHERE event_id = ?1",
            params![event_id, i64::from(!unread)],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------- deliveries

    pub fn upsert_delivery(&self, delivery: &DeliveryRow) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO deliveries
                (post_event_id, recipient, state, reason, attempts, next_attempt_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(post_event_id, recipient) DO UPDATE SET
                state = excluded.state,
                reason = excluded.reason,
                attempts = excluded.attempts,
                next_attempt_at = excluded.next_attempt_at,
                updated_at = excluded.updated_at",
            params![
                delivery.post_event_id,
                delivery.recipient,
                delivery.state.as_str(),
                delivery.reason,
                delivery.attempts,
                delivery.next_attempt_at,
                delivery.updated_at
            ],
        )?;
        Ok(())
    }

    fn delivery_from_row(row: &Row<'_>) -> rusqlite::Result<DeliveryRow> {
        let state: String = row.get(2)?;
        Ok(DeliveryRow {
            post_event_id: row.get(0)?,
            recipient: row.get(1)?,
            state: state.parse().unwrap_or(DeliveryState::Queued),
            reason: row.get(3)?,
            attempts: row.get(4)?,
            next_attempt_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    }

    pub fn deliveries_for(&self, post: &str) -> Result<Vec<DeliveryRow>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT post_event_id, recipient, state, reason, attempts, next_attempt_at, updated_at
             FROM deliveries WHERE post_event_id = ?1 ORDER BY recipient",
        )?;
        let rows = statement.query_map(params![post], Self::delivery_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn pending_deliveries(&self) -> Result<Vec<DeliveryRow>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT post_event_id, recipient, state, reason, attempts, next_attempt_at, updated_at
             FROM deliveries WHERE state IN ('queued', 'retrying', 'sending')
             ORDER BY updated_at ASC",
        )?;
        let rows = statement.query_map([], Self::delivery_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn set_delivery_state(
        &self,
        post: &str,
        recipient: &str,
        state: DeliveryState,
        reason: Option<&str>,
        next_attempt_at: Option<&str>,
        bump_attempts: bool,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "UPDATE deliveries SET
                state = ?3,
                reason = ?4,
                next_attempt_at = ?5,
                attempts = attempts + ?6,
                updated_at = ?7
             WHERE post_event_id = ?1 AND recipient = ?2",
            params![
                post,
                recipient,
                state.as_str(),
                reason,
                next_attempt_at,
                i64::from(bump_attempts),
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    // --------------------------------------------------------------- problems

    pub fn add_problem(&self, problem: &ProblemRow) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO problems (event_id, author, kind, reason, detail, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(event_id) DO UPDATE SET reason = excluded.reason, detail = excluded.detail",
            params![
                problem.event_id,
                problem.author,
                problem.kind,
                problem.reason,
                problem.detail,
                problem.created_at
            ],
        )?;
        Ok(())
    }

    pub fn problems(&self) -> Result<Vec<ProblemRow>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT event_id, author, kind, reason, detail, created_at FROM problems
             ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ProblemRow {
                event_id: row.get(0)?,
                author: row.get(1)?,
                kind: row.get(2)?,
                reason: row.get(3)?,
                detail: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn clear_problem(&self, event_id: &str) -> Result<(), StorageError> {
        self.conn.execute(
            "DELETE FROM problems WHERE event_id = ?1",
            params![event_id],
        )?;
        Ok(())
    }

    // --------------------------------------------------------------- settings

    pub fn setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn setting_or(&self, key: &str, default: &str) -> Result<String, StorageError> {
        Ok(self.setting(key)?.unwrap_or_else(|| default.to_string()))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn settings(&self) -> Result<Vec<(String, String)>, StorageError> {
        let mut statement = self
            .conn
            .prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ------------------------------------------------------------- sync state

    pub fn set_sync_state(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn sync_state(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM sync_state WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Resolve replies whose parent just arrived.
    pub fn resolve_orphans(
        &self,
        parent: &str,
        parent_author: &str,
    ) -> Result<usize, StorageError> {
        let changed = self.conn.execute(
            "UPDATE events SET parent_author = ?2 WHERE parent = ?1",
            params![parent, parent_author],
        )?;
        Ok(changed)
    }

    /// Events authored by one fingerprint, for bundle export.
    pub fn events_by_author(
        &self,
        author: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, StorageError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM events WHERE author = ?1 ORDER BY received_at ASC LIMIT ?2",
            Self::EVENT_COLUMNS
        ))?;
        let rows = statement.query_map(params![author, limit as i64], Self::event_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Accepted root posts written by other people, oldest first.
    ///
    /// The reflector uses this to find list traffic that has not been
    /// redistributed yet; the engine narrows the result to the configured list.
    pub fn posts_for_relay(
        &self,
        exclude_author: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, StorageError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {} FROM events
             WHERE kind = 'post' AND status = 'accepted' AND author <> ?1
             ORDER BY received_at ASC LIMIT ?2",
            Self::EVENT_COLUMNS
        ))?;
        let rows =
            statement.query_map(params![exclude_author, limit as i64], Self::event_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Every delivery row, newest first, for the queue screen.
    pub fn all_deliveries(&self, limit: usize) -> Result<Vec<DeliveryRow>, StorageError> {
        let mut statement = self.conn.prepare(
            "SELECT post_event_id, recipient, state, reason, attempts, next_attempt_at, updated_at
             FROM deliveries ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], Self::delivery_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // -------------------------------------------------------------- diagnostics

    pub fn counts(&self, me: &str) -> Result<Counts, StorageError> {
        let scalar = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<i64, StorageError> {
            Ok(self.conn.query_row(sql, params, |row| row.get(0))?)
        };
        Ok(Counts {
            events: scalar("SELECT COUNT(*) FROM events", &[])?,
            roots: scalar("SELECT COUNT(*) FROM timeline WHERE is_root = 1", &[])?,
            replies: scalar("SELECT COUNT(*) FROM events WHERE kind = 'reply'", &[])?,
            contacts: scalar("SELECT COUNT(*) FROM contacts", &[])?,
            verified_contacts: scalar(
                "SELECT COUNT(*) FROM contacts WHERE trust = 'verified'",
                &[],
            )?,
            queued_deliveries: scalar(
                "SELECT COUNT(*) FROM deliveries WHERE state IN ('queued','retrying','sending')",
                &[],
            )?,
            failed_deliveries: scalar(
                "SELECT COUNT(*) FROM deliveries WHERE state = 'failed'",
                &[],
            )?,
            withheld_deliveries: scalar(
                "SELECT COUNT(*) FROM deliveries WHERE state = 'withheld'",
                &[],
            )?,
            problems: scalar("SELECT COUNT(*) FROM problems", &[])?,
            unseen_following: self.unseen_count(Tab::Following, me)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        Database::open_memory().expect("in-memory database")
    }

    fn contact(fingerprint: &str, trust: Trust) -> Contact {
        Contact {
            fingerprint: fingerprint.to_string(),
            display_name: format!("name-{fingerprint}"),
            address: format!("{fingerprint}@example.org"),
            ed25519: "ed".into(),
            x25519: "x".into(),
            trust,
            muted: false,
            added_at: "2026-11-03T09:00:00Z".into(),
            updated_at: "2026-11-03T09:00:00Z".into(),
        }
    }

    fn event(event_id: &str, author: &str, kind: &str, parent: Option<&str>) -> EventRow {
        EventRow {
            event_id: event_id.to_string(),
            author: author.to_string(),
            kind: kind.to_string(),
            created_at: "2026-11-03T09:00:00Z".into(),
            received_at: "2026-11-03T09:00:01Z".into(),
            payload: "{}".into(),
            signature: "sig".into(),
            canonical: "{}".into(),
            status: "accepted".into(),
            reason: None,
            parent: parent.map(str::to_string),
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let database = db();
        assert_eq!(database.schema_version().unwrap(), 1);
        database.migrate().unwrap();
        database.migrate().unwrap();
        assert_eq!(database.schema_version().unwrap(), 1);
    }

    #[test]
    fn contacts_round_trip_and_update_local_state() {
        let database = db();
        database
            .upsert_contact(&contact("A", Trust::Unverified))
            .unwrap();
        assert_eq!(database.contacts().unwrap().len(), 1);
        // Upsert keeps local trust; only attributes refresh.
        let mut updated = contact("A", Trust::Blocked);
        updated.display_name = "Alice".into();
        database.upsert_contact(&updated).unwrap();
        let stored = database.contact("A").unwrap().unwrap();
        assert_eq!(stored.display_name, "Alice");
        assert_eq!(stored.trust, Trust::Unverified);

        database.set_trust("A", Trust::Verified).unwrap();
        database.set_muted("A", true).unwrap();
        let stored = database.contact("A").unwrap().unwrap();
        assert!(stored.trust.is_verified());
        assert!(stored.muted);

        assert!(matches!(
            database.set_trust("missing", Trust::Verified),
            Err(StorageError::NotFound(_))
        ));
    }

    #[test]
    fn events_are_idempotent() {
        let database = db();
        let row = event("e1", "A", "post", None);
        assert!(database.insert_event(&row).unwrap());
        assert!(!database.insert_event(&row).unwrap());
        assert_eq!(database.event_count().unwrap(), 1);
        assert_eq!(database.event("e1").unwrap().unwrap(), row);
    }

    #[test]
    fn positions_are_monotonic_and_never_reused() {
        let mut database = db();
        database
            .insert_event(&event("e1", "A", "post", None))
            .unwrap();
        database
            .insert_event(&event("e2", "A", "post", None))
            .unwrap();
        let first = database
            .append_root("e1", "A", "post", "followers", None)
            .unwrap();
        let second = database
            .append_root("e2", "A", "post", "followers", None)
            .unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 2);
        // Re-appending must not move an item.
        database
            .append_root("e1", "A", "post", "followers", None)
            .unwrap();
        assert_eq!(database.position_of("e1").unwrap(), Some(1));
        assert_eq!(database.max_position().unwrap(), 2);
    }

    #[test]
    fn tabs_filter_by_follow_and_trust() {
        let mut database = db();
        let me = "ME";
        for id in ["p1", "p2", "p3", "mine", "listed"] {
            database
                .insert_event(&event(id, "X", "post", None))
                .unwrap();
        }
        database
            .append_root("p1", "ALICE", "post", "followers", None)
            .unwrap();
        database
            .append_root("p2", "BOB", "post", "followers", None)
            .unwrap();
        database
            .append_root("p3", "MUTED", "post", "followers", None)
            .unwrap();
        database
            .append_root("mine", me, "post", "followers", None)
            .unwrap();
        database
            .append_root("listed", "LISTER", "post", "public-list", Some("rust"))
            .unwrap();

        database
            .upsert_contact(&contact("ALICE", Trust::Verified))
            .unwrap();
        database
            .upsert_contact(&contact("BOB", Trust::Unverified))
            .unwrap();
        database
            .upsert_contact(&contact("MUTED", Trust::Unverified))
            .unwrap();
        database.set_muted("MUTED", true).unwrap();
        database
            .upsert_follow(me, "ALICE", FollowState::Following, true)
            .unwrap();
        database
            .upsert_follow(me, "BOB", FollowState::Following, false)
            .unwrap();
        database
            .upsert_follow(me, "MUTED", FollowState::Following, false)
            .unwrap();

        let following = database.timeline(Tab::Following, me, 50, None).unwrap();
        let ids: Vec<&str> = following.iter().map(|row| row.event_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["p2", "p1"],
            "newest first, muted and own excluded"
        );

        let lists = database.timeline(Tab::Lists, me, 50, None).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].list_id.as_deref(), Some("rust"));

        let mine_rows = database.timeline(Tab::You, me, 50, None).unwrap();
        assert_eq!(mine_rows.len(), 1);

        let all = database.timeline(Tab::All, me, 50, None).unwrap();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn seen_state_and_cursors() {
        let mut database = db();
        let me = "ME";
        for (id, author) in [("p1", "A"), ("p2", "A"), ("p3", "A")] {
            database
                .insert_event(&event(id, author, "post", None))
                .unwrap();
            database
                .append_root(id, author, "post", "followers", None)
                .unwrap();
        }
        database
            .upsert_contact(&contact("A", Trust::Unverified))
            .unwrap();
        database
            .upsert_follow(me, "A", FollowState::Following, false)
            .unwrap();

        assert_eq!(database.unseen_count(Tab::Following, me).unwrap(), 3);
        database.mark_seen(Tab::Following, me, 2).unwrap();
        assert_eq!(database.unseen_count(Tab::Following, me).unwrap(), 1);
        assert_eq!(database.cursor(Tab::Following).unwrap(), 2);

        let rows = database.timeline(Tab::Following, me, 50, None).unwrap();
        assert!(rows.iter().find(|r| r.event_id == "p1").unwrap().seen);
        assert!(!rows.iter().find(|r| r.event_id == "p3").unwrap().seen);

        database.set_unread("p1", true).unwrap();
        assert_eq!(database.unseen_count(Tab::Following, me).unwrap(), 2);
    }

    #[test]
    fn conversations_include_nested_replies_only_when_stored() {
        let database = db();
        database
            .insert_event(&event("root", "A", "post", None))
            .unwrap();
        database
            .insert_event(&event("r1", "B", "reply", Some("root")))
            .unwrap();
        database
            .insert_event(&event("r2", "C", "reply", Some("r1")))
            .unwrap();
        database
            .insert_event(&event("orphan", "D", "reply", Some("missing")))
            .unwrap();

        let thread = database.conversation("root").unwrap();
        let ids: Vec<&str> = thread.iter().map(|row| row.event_id.as_str()).collect();
        assert_eq!(ids, vec!["root", "r1", "r2"]);
        assert_eq!(database.direct_reply_count("root").unwrap(), 1);

        let orphans = database.orphan_replies(10).unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].event_id, "orphan");
    }

    #[test]
    fn replies_to_me_use_the_resolved_parent_author() {
        let database = db();
        database
            .insert_event(&event("root", "ME", "post", None))
            .unwrap();
        database
            .insert_event(&event("r1", "ALICE", "reply", Some("root")))
            .unwrap();
        database.set_parent_author("r1", "ME").unwrap();
        let replies = database.replies_to("ME", 10).unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].author, "ALICE");
    }

    #[test]
    fn deliveries_track_withheld_and_pending() {
        let database = db();
        let base = "2026-11-03T09:00:00Z";
        database
            .upsert_delivery(&DeliveryRow {
                post_event_id: "post".into(),
                recipient: "A".into(),
                state: DeliveryState::Queued,
                reason: None,
                attempts: 0,
                next_attempt_at: None,
                updated_at: base.into(),
            })
            .unwrap();
        database
            .upsert_delivery(&DeliveryRow {
                post_event_id: "post".into(),
                recipient: "B".into(),
                state: DeliveryState::Withheld,
                reason: Some("unverified".into()),
                attempts: 0,
                next_attempt_at: None,
                updated_at: base.into(),
            })
            .unwrap();

        assert_eq!(database.deliveries_for("post").unwrap().len(), 2);
        assert_eq!(database.pending_deliveries().unwrap().len(), 1);

        database
            .set_delivery_state(
                "post",
                "A",
                DeliveryState::Failed,
                Some("550 rejected"),
                None,
                true,
            )
            .unwrap();
        let rows = database.deliveries_for("post").unwrap();
        let a = rows.iter().find(|row| row.recipient == "A").unwrap();
        assert_eq!(a.state, DeliveryState::Failed);
        assert_eq!(a.attempts, 1);
        assert!(database.pending_deliveries().unwrap().is_empty());
    }

    #[test]
    fn settings_and_sync_state_round_trip() {
        let database = db();
        assert_eq!(database.setting("ui.locale").unwrap(), None);
        assert_eq!(database.setting_or("ui.locale", "en").unwrap(), "en");
        database.set_setting("ui.locale", "fa").unwrap();
        assert_eq!(database.setting_or("ui.locale", "en").unwrap(), "fa");
        database.set_sync_state("imap.uid", "42").unwrap();
        assert_eq!(
            database.sync_state("imap.uid").unwrap().as_deref(),
            Some("42")
        );
    }

    #[test]
    fn problems_are_upserted_and_clearable() {
        let database = db();
        database
            .add_problem(&ProblemRow {
                event_id: "bad".into(),
                author: Some("X".into()),
                kind: Some("post".into()),
                reason: "signature invalid".into(),
                detail: None,
                created_at: "2026-11-03T09:00:00Z".into(),
            })
            .unwrap();
        database
            .add_problem(&ProblemRow {
                event_id: "bad".into(),
                author: Some("X".into()),
                kind: Some("post".into()),
                reason: "signature invalid again".into(),
                detail: None,
                created_at: "2026-11-03T09:01:00Z".into(),
            })
            .unwrap();
        let problems = database.problems().unwrap();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].reason, "signature invalid again");
        database.clear_problem("bad").unwrap();
        assert!(database.problems().unwrap().is_empty());
    }

    #[test]
    fn counts_summarise_state() {
        let mut database = db();
        database
            .insert_event(&event("p1", "A", "post", None))
            .unwrap();
        database
            .append_root("p1", "A", "post", "followers", None)
            .unwrap();
        database
            .upsert_contact(&contact("A", Trust::Verified))
            .unwrap();
        database
            .upsert_follow("ME", "A", FollowState::Following, true)
            .unwrap();
        let counts = database.counts("ME").unwrap();
        assert_eq!(counts.events, 1);
        assert_eq!(counts.roots, 1);
        assert_eq!(counts.contacts, 1);
        assert_eq!(counts.verified_contacts, 1);
        assert_eq!(counts.unseen_following, 1);
    }
}
