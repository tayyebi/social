//! The JNI bridge the Android app talks to.
//!
//! One native entry point, one JSON document in, one JSON document out:
//!
//! ```text
//! {"cmd": "feed", "home": "/data/user/0/tld.android.social/files", "tab": "following"}
//! {"ok": true, "data": [ ... ]}
//! {"ok": false, "error": "not initialised"}
//! ```
//!
//! Keeping the surface to a single string-in/string-out function means the JNI
//! signature can never drift out of sync with the Kotlin side, every failure path
//! is ordinary Rust, and the app speaks the same engine the CLI and TUI do.
//!
//! The mail password is never a parameter of the bridge: a request may carry a
//! `password`, which is placed in the process environment for the duration of the
//! call so the transport can read it, exactly as on the desktop. It is not logged,
//! not returned, and not written anywhere.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use serde_json::{json, Value};

use social_mail_engine::{
    Audience, Config, ContactView, ConversationItem, Engine, FeedItem, ImportOutcome, InitOptions,
    QueueItem, Tab,
};
use social_mail_storage::{Contact, Counts, DeliveryRow, FollowRow, ProblemRow, Trust};

/// Config file name inside the app's private directory.
const CONFIG_FILE: &str = "social-mail.toml";
/// Variable the transport reads the app password from.
const PASSWORD_ENV: &str = "SOCIAL_MAIL_PASSWORD";

// ------------------------------------------------------------------ entry point

#[no_mangle]
pub extern "system" fn Java_tld_android_social_Core_call<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    request: JString<'local>,
) -> jstring {
    let input: String = match env.get_string(&request) {
        Ok(value) => value.into(),
        Err(_) => String::new(),
    };
    let response = catch_unwind(AssertUnwindSafe(|| handle(&input)))
        .unwrap_or_else(|_| failure("the engine panicked; nothing was written"));
    match env.new_string(response) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Parse one request and answer with one JSON document. Never panics on bad input.
pub fn handle(input: &str) -> String {
    let request: Value = match serde_json::from_str(input) {
        Ok(value) => value,
        Err(err) => return failure(&format!("request is not valid JSON: {err}")),
    };
    if let Some(password) = request.get("password").and_then(Value::as_str) {
        if !password.is_empty() {
            std::env::set_var(PASSWORD_ENV, password);
        }
    }
    let command = request.get("cmd").and_then(Value::as_str).unwrap_or("");
    match run(command, &request) {
        Ok(data) => json!({ "ok": true, "data": data }).to_string(),
        Err(err) => failure(&err),
    }
}

fn failure(message: &str) -> String {
    json!({ "ok": false, "error": message }).to_string()
}

fn run(command: &str, request: &Value) -> Result<Value, String> {
    match command {
        "status" => status(request),
        "create" => create(request),
        "feed" => feed(request),
        "conversation" => conversation(request),
        "post" => post(request),
        "reply" => reply(request),
        "follow" => follow(request),
        "unfollow" => with_contact(request, |engine, fingerprint| {
            engine.unfollow(fingerprint).map_err(engine_error)
        }),
        "verify" => with_contact(request, |engine, fingerprint| {
            engine.verify_contact(fingerprint).map_err(engine_error)
        }),
        "block" => with_contact(request, |engine, fingerprint| {
            engine.block_contact(fingerprint).map_err(engine_error)
        }),
        "unblock" => with_contact(request, |engine, fingerprint| {
            engine.unblock_contact(fingerprint).map_err(engine_error)
        }),
        "mute" => mute(request),
        "contacts" => contacts(request),
        "requests" => requests(request),
        "approve" => approve(request),
        "decline" => with_contact(request, |engine, fingerprint| {
            engine.decline_follower(fingerprint).map_err(engine_error)
        }),
        "remove_follower" => with_contact(request, |engine, fingerprint| {
            engine.remove_follower(fingerprint).map_err(engine_error)
        }),
        "problems" => problems(request),
        "dismiss_problem" => dismiss_problem(request),
        "sync" => sync(request),
        "queue" => queue(request),
        "retry_delivery" => retry_delivery(request),
        "cancel_delivery" => cancel_delivery(request),
        "lists" => lists(request),
        "add_list" => add_list(request),
        "remove_list" => remove_list(request),
        "mark_seen" => mark_seen(request),
        "unseen" => unseen(request),
        "counts" => counts(request),
        "card" => card(request),
        "import_card" => import_card(request),
        "settings" => settings(request),
        "settings_set" => settings_set(request),
        "mail_test" => mail_test(request),
        other => Err(format!("unknown command {other:?}")),
    }
}

// --------------------------------------------------------------------- commands

fn status(request: &Value) -> Result<Value, String> {
    let path = config_path(request)?;
    if !Engine::exists(&path) {
        return Ok(json!({ "initialised": false }));
    }
    let engine = Engine::open(&path).map_err(engine_error)?;
    let counts = engine.counts().map_err(engine_error)?;
    let transport_error = engine.transport_error().map_err(engine_error)?;
    let profile = engine.profile();
    Ok(json!({
        "initialised": true,
        "fingerprint": profile.fingerprint.clone(),
        "name": profile.display_name.clone(),
        "address": profile.address.clone(),
        "locale": profile.locale.clone(),
        "createdAt": profile.created_at.clone(),
        "counts": counts_json(&counts),
        "mail": mail_json(&engine.config),
        "transportError": transport_error,
    }))
}

fn create(request: &Value) -> Result<Value, String> {
    let home = home_dir(request)?;
    std::fs::create_dir_all(&home).map_err(|err| format!("could not create {home:?}: {err}"))?;
    let path = home.join(CONFIG_FILE);
    let mut config = Config::load(&path).map_err(engine_error)?;
    // Absolute locations inside the app's private directory: the working directory
    // of an Android process is "/", so relative paths would be unwritable.
    config.data_dir = home.join("data");
    config.spool_dir = home.join("spool");
    config.save(&path).map_err(engine_error)?;
    let options = InitOptions {
        display_name: text(request, "name"),
        address: text(request, "address"),
        locale: text(request, "locale"),
    };
    let engine = Engine::create(&path, options).map_err(engine_error)?;
    Ok(json!({
        "fingerprint": engine.fingerprint(),
        "name": engine.profile().display_name.clone(),
    }))
}

fn feed(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let tab = tab_from(&text(request, "tab"))?;
    let limit = number(request, "limit").unwrap_or(80).clamp(1, 500) as usize;
    let items = engine.timeline(tab, limit).map_err(engine_error)?;
    Ok(Value::Array(items.iter().map(feed_json).collect()))
}

fn conversation(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let root = required(request, "root")?;
    let items = engine.conversation(&root).map_err(engine_error)?;
    Ok(Value::Array(items.iter().map(conversation_json).collect()))
}

fn post(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let body = required(request, "text")?;
    let audience = audience_from(&text(request, "audience"))?;
    let list = optional(request, "list");
    if audience == Audience::PublicList && list.is_none() {
        return Err("a public list post needs a list address".to_string());
    }
    let event_id = engine.post(&body, audience, list).map_err(engine_error)?;
    Ok(json!({ "eventId": event_id }))
}

fn reply(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let parent = required(request, "parent")?;
    let body = required(request, "text")?;
    let event_id = engine.reply(&parent, &body).map_err(engine_error)?;
    Ok(json!({ "eventId": event_id }))
}

fn follow(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let target = required(request, "target")?;
    let event_id = engine.follow(&target).map_err(engine_error)?;
    Ok(json!({ "eventId": event_id }))
}

fn mute(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let fingerprint = required(request, "fingerprint")?;
    engine
        .mute_contact(&fingerprint, flag(request, "muted"))
        .map_err(engine_error)?;
    Ok(json!({ "ok": true }))
}

fn contacts(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let views = engine.contacts_view().map_err(engine_error)?;
    Ok(Value::Array(views.iter().map(contact_view_json).collect()))
}

fn requests(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let pending = engine.pending_requests().map_err(engine_error)?;
    let mut out = Vec::with_capacity(pending.len());
    for (row, contact) in pending.iter() {
        out.push(json!({
            "fingerprint": row.follower.clone(),
            "name": contact
                .as_ref()
                .map(|value| value.display_name.clone())
                .unwrap_or_default(),
            "address": contact
                .as_ref()
                .map(|value| value.address.clone())
                .unwrap_or_default(),
            "updatedAt": row.updated_at.clone(),
        }));
    }
    Ok(Value::Array(out))
}

fn approve(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let fingerprint = required(request, "fingerprint")?;
    let event_id = engine
        .approve_follower(&fingerprint, flag(request, "verify"))
        .map_err(engine_error)?;
    Ok(json!({ "eventId": event_id }))
}

fn problems(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let rows = engine.problems().map_err(engine_error)?;
    Ok(Value::Array(rows.iter().map(problem_json).collect()))
}

fn dismiss_problem(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let event_id = required(request, "eventId")?;
    engine.dismiss_problem(&event_id).map_err(engine_error)?;
    Ok(json!({ "ok": true }))
}

fn sync(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let report = engine.sync().map_err(engine_error)?;
    Ok(json!({
        "received": report.received,
        "accepted": report.accepted,
        "duplicates": report.duplicates,
        "dropped": report.dropped,
        "quarantined": report.quarantined,
        "sent": report.sent,
        "failed": report.failed,
    }))
}

fn queue(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let limit = number(request, "limit").unwrap_or(50).clamp(1, 200) as usize;
    let items = engine.queue(limit).map_err(engine_error)?;
    Ok(Value::Array(items.iter().map(queue_json).collect()))
}

fn retry_delivery(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let post = required(request, "post")?;
    let recipient = required(request, "recipient")?;
    engine
        .retry_delivery(&post, &recipient)
        .map_err(engine_error)?;
    Ok(json!({ "ok": true }))
}

fn cancel_delivery(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let post = required(request, "post")?;
    let recipient = required(request, "recipient")?;
    engine
        .cancel_delivery(&post, &recipient)
        .map_err(engine_error)?;
    Ok(json!({ "ok": true }))
}

fn lists(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let lists = engine.lists().map_err(engine_error)?;
    Ok(json!({ "lists": lists }))
}

fn add_list(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let address = required(request, "address")?;
    engine.add_list(&address).map_err(engine_error)?;
    Ok(json!({ "ok": true }))
}

fn remove_list(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let address = required(request, "address")?;
    engine.remove_list(&address).map_err(engine_error)?;
    Ok(json!({ "ok": true }))
}

fn mark_seen(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let tab = tab_from(&text(request, "tab"))?;
    engine.mark_seen(tab).map_err(engine_error)?;
    Ok(json!({ "ok": true }))
}

fn unseen(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let tab = tab_from(&text(request, "tab"))?;
    Ok(json!({ "unseen": engine.unseen(tab).map_err(engine_error)? }))
}

fn counts(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let counts = engine.counts().map_err(engine_error)?;
    Ok(counts_json(&counts))
}

fn card(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let card = engine.contact_card().map_err(engine_error)?;
    let text = card.to_pretty_json().map_err(|err| err.to_string())?;
    Ok(json!({
        "fingerprint": card.fingerprint.clone(),
        "address": card.address.clone(),
        "text": text,
    }))
}

fn import_card(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    let payload = required(request, "text")?;
    let outcome = engine.import_card(&payload).map_err(engine_error)?;
    Ok(json!({ "outcome": outcome_str(&outcome) }))
}

fn settings(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    Ok(settings_json(&engine.config))
}

fn settings_set(request: &Value) -> Result<Value, String> {
    let mut engine = open_engine(request)?;
    {
        let config = &mut engine.config;
        if let Some(value) = optional(request, "name") {
            config.profile_name = value;
        }
        if let Some(value) = optional(request, "address") {
            config.address = value;
        }
        if let Some(value) = optional(request, "locale") {
            config.locale = value;
        }
        if let Some(value) = optional(request, "transport") {
            if value != "spool" && value != "mail" {
                return Err(format!("unknown transport {value:?}"));
            }
            config.transport = value;
        }
        if let Some(value) = optional(request, "account") {
            config.account = value;
        }
        if let Some(value) = optional(request, "imapHost") {
            config.imap_host = value;
        }
        if let Some(value) = number(request, "imapPort") {
            config.imap_port = port(value)?;
        }
        if let Some(value) = optional(request, "imapTls") {
            config.imap_tls = tls_from(&value)?;
        }
        if let Some(value) = optional(request, "smtpHost") {
            config.smtp_host = value;
        }
        if let Some(value) = number(request, "smtpPort") {
            config.smtp_port = port(value)?;
        }
        if let Some(value) = optional(request, "smtpTls") {
            config.smtp_tls = tls_from(&value)?;
        }
        if let Some(value) = optional(request, "username") {
            config.username = value;
        }
        if let Some(value) = optional(request, "passwordEnv") {
            config.password_env = value;
        }
        if let Some(value) = optional(request, "folder") {
            config.mail_folder = value;
        }
        if let Some(value) = optional(request, "protocolFolder") {
            config.protocol_folder = value;
        }
        if let Some(value) = request.get("moveProtocolMessages").and_then(Value::as_bool) {
            config.move_protocol_messages = value;
        }
        if let Some(value) = number(request, "autoSyncMinutes") {
            config.auto_sync_minutes = value.clamp(0, 24 * 60);
        }
        if config.transport == "mail" {
            if config.imap_host.trim().is_empty() {
                return Err("IMAP host is required for the mail transport".to_string());
            }
            if config.smtp_host.trim().is_empty() {
                return Err("SMTP host is required for the mail transport".to_string());
            }
        }
    }
    engine.save_config().map_err(engine_error)?;
    Ok(settings_json(&engine.config))
}

fn mail_test(request: &Value) -> Result<Value, String> {
    let engine = open_engine(request)?;
    let report = if flag(request, "send") {
        engine.mail_send_test().map_err(engine_error)?
    } else {
        engine.mail_probe().map_err(engine_error)?
    };
    Ok(json!({ "report": report }))
}

// ---------------------------------------------------------------------- helpers

fn home_dir(request: &Value) -> Result<PathBuf, String> {
    optional(request, "home")
        .map(PathBuf::from)
        .ok_or_else(|| "home directory is missing".to_string())
}

fn config_path(request: &Value) -> Result<PathBuf, String> {
    Ok(home_dir(request)?.join(CONFIG_FILE))
}

fn open_engine(request: &Value) -> Result<Engine, String> {
    let path = config_path(request)?;
    if !Engine::exists(&path) {
        return Err("not initialised".to_string());
    }
    Engine::open(&path).map_err(engine_error)
}

/// Run a contact operation against the fingerprint in the request.
fn with_contact<F>(request: &Value, action: F) -> Result<Value, String>
where
    F: FnOnce(&mut Engine, &str) -> Result<(), String>,
{
    let mut engine = open_engine(request)?;
    let fingerprint = required(request, "fingerprint")?;
    action(&mut engine, &fingerprint)?;
    Ok(json!({ "ok": true }))
}

fn engine_error(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn text(request: &Value, key: &str) -> String {
    request
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn required(request: &Value, key: &str) -> Result<String, String> {
    optional(request, key).ok_or_else(|| format!("{key} is required"))
}

fn optional(request: &Value, key: &str) -> Option<String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn flag(request: &Value, key: &str) -> bool {
    request.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn number(request: &Value, key: &str) -> Option<i64> {
    request.get(key).and_then(Value::as_i64)
}

fn port(value: i64) -> Result<i64, String> {
    if (1..=65_535).contains(&value) {
        Ok(value)
    } else {
        Err(format!("port {value} is out of range"))
    }
}

fn tls_from(value: &str) -> Result<String, String> {
    match value {
        "implicit" | "starttls" | "none" => Ok(value.to_string()),
        other => Err(format!("unknown TLS mode {other:?}")),
    }
}

fn tab_from(value: &str) -> Result<Tab, String> {
    match value {
        "" | "following" => Ok(Tab::Following),
        "lists" => Ok(Tab::Lists),
        "you" => Ok(Tab::You),
        "all" => Ok(Tab::All),
        other => Err(format!("unknown tab {other:?}")),
    }
}

fn audience_from(value: &str) -> Result<Audience, String> {
    match value {
        "" | "followers" => Ok(Audience::Followers),
        "public-list" | "public_list" | "list" => Ok(Audience::PublicList),
        other => Err(format!("unknown audience {other:?}")),
    }
}

fn outcome_str(outcome: &ImportOutcome) -> String {
    match outcome {
        ImportOutcome::Added => "added",
        ImportOutcome::Updated => "updated",
        ImportOutcome::Unchanged => "unchanged",
        ImportOutcome::Mismatch { .. } => "mismatch",
        ImportOutcome::OwnCard => "own-card",
    }
    .to_string()
}

fn trust_str(trust: Trust) -> &'static str {
    trust.as_str()
}

fn counts_json(counts: &Counts) -> Value {
    json!({
        "events": counts.events,
        "roots": counts.roots,
        "replies": counts.replies,
        "contacts": counts.contacts,
        "verifiedContacts": counts.verified_contacts,
        "queuedDeliveries": counts.queued_deliveries,
        "failedDeliveries": counts.failed_deliveries,
        "withheldDeliveries": counts.withheld_deliveries,
        "problems": counts.problems,
        "unseenFollowing": counts.unseen_following,
    })
}

fn mail_json(config: &Config) -> Value {
    json!({
        "transport": config.transport.clone(),
        "account": config.account.clone(),
        "imapHost": config.imap_host.clone(),
        "imapPort": config.imap_port,
        "imapTls": config.imap_tls.clone(),
        "smtpHost": config.smtp_host.clone(),
        "smtpPort": config.smtp_port,
        "smtpTls": config.smtp_tls.clone(),
        "username": config.username.clone(),
        "passwordEnv": config.password_env.clone(),
        "folder": config.mail_folder.clone(),
    })
}

fn settings_json(config: &Config) -> Value {
    let mut value = mail_json(config);
    if let Value::Object(ref mut map) = value {
        map.insert("name".into(), json!(config.profile_name));
        map.insert("address".into(), json!(config.address));
        map.insert("locale".into(), json!(config.locale));
        map.insert(
            "moveProtocolMessages".into(),
            json!(config.move_protocol_messages),
        );
        map.insert("protocolFolder".into(), json!(config.protocol_folder));
        map.insert("autoSyncMinutes".into(), json!(config.auto_sync_minutes));
    }
    value
}

fn feed_json(item: &FeedItem) -> Value {
    json!({
        "eventId": item.event_id.clone(),
        "position": item.position,
        "author": item.author.clone(),
        "authorName": item.author_name.clone(),
        "trust": trust_str(item.trust),
        "audience": match item.audience {
            Audience::Followers => "followers",
            Audience::PublicList => "public-list",
        },
        "listId": item.list_id.clone(),
        "text": item.text.clone(),
        "createdAt": item.created_at.clone(),
        "receivedAt": item.received_at.clone(),
        "seen": item.seen,
        "replyCount": item.reply_count,
        "delivery": item.delivery_summary.clone(),
        "mine": item.is_mine,
    })
}

fn conversation_json(item: &ConversationItem) -> Value {
    json!({
        "eventId": item.event_id.clone(),
        "author": item.author.clone(),
        "authorName": item.author_name.clone(),
        "trust": trust_str(item.trust),
        "text": item.text.clone(),
        "createdAt": item.created_at.clone(),
        "depth": item.depth,
        "kind": item.kind.clone(),
    })
}

fn queue_json(item: &QueueItem) -> Value {
    json!({
        "post": item.post_event_id.clone(),
        "preview": item.preview.clone(),
        "deliveries": item.deliveries.iter().map(delivery_json).collect::<Vec<_>>(),
    })
}

fn delivery_json(row: &DeliveryRow) -> Value {
    json!({
        "recipient": row.recipient.clone(),
        "state": row.state.as_str(),
        "reason": row.reason.clone(),
        "attempts": row.attempts,
        "nextAttemptAt": row.next_attempt_at.clone(),
    })
}

fn contact_view_json(view: &ContactView) -> Value {
    json!({
        "fingerprint": view.contact.fingerprint.clone(),
        "name": view.contact.display_name.clone(),
        "address": view.contact.address.clone(),
        "trust": trust_str(view.contact.trust),
        "muted": view.contact.muted,
        "iFollow": view.i_follow.map(|state| state.as_str()),
        "followsMe": view.follows_me.map(|state| state.as_str()),
        "pendingRequest": view.pending_request,
        "addedAt": view.contact.added_at.clone(),
    })
}

fn problem_json(row: &ProblemRow) -> Value {
    json!({
        "eventId": row.event_id.clone(),
        "author": row.author.clone(),
        "kind": row.kind.clone(),
        "reason": row.reason.clone(),
        "detail": row.detail.clone(),
        "createdAt": row.created_at.clone(),
    })
}

#[allow(dead_code)]
fn contact_json(contact: &Contact) -> Value {
    json!({
        "fingerprint": contact.fingerprint.clone(),
        "name": contact.display_name.clone(),
        "address": contact.address.clone(),
        "trust": trust_str(contact.trust),
        "muted": contact.muted,
    })
}

#[allow(dead_code)]
fn follow_json(row: &FollowRow) -> Value {
    json!({
        "follower": row.follower.clone(),
        "followee": row.followee.clone(),
        "state": row.state.as_str(),
        "verified": row.verified,
        "updatedAt": row.updated_at.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_commands_fail_without_panicking() {
        let answer: Value = serde_json::from_str(&handle(r#"{"cmd":"nope"}"#)).unwrap();
        assert_eq!(answer["ok"], json!(false));
        assert!(answer["error"].as_str().unwrap().contains("unknown command"));
    }

    #[test]
    fn invalid_json_is_reported() {
        let answer: Value = serde_json::from_str(&handle("{not json")).unwrap();
        assert_eq!(answer["ok"], json!(false));
        assert!(answer["error"]
            .as_str()
            .unwrap()
            .contains("not valid JSON"));
    }

    #[test]
    fn missing_home_is_reported() {
        let answer: Value = serde_json::from_str(&handle(r#"{"cmd":"status"}"#)).unwrap();
        assert_eq!(answer["ok"], json!(false));
        assert!(answer["error"].as_str().unwrap().contains("home directory"));
    }

    #[test]
    fn tabs_and_audiences_map_to_the_engine() {
        assert_eq!(tab_from("following").unwrap(), Tab::Following);
        assert_eq!(tab_from("").unwrap(), Tab::Following);
        assert_eq!(tab_from("lists").unwrap(), Tab::Lists);
        assert_eq!(tab_from("you").unwrap(), Tab::You);
        assert_eq!(tab_from("all").unwrap(), Tab::All);
        assert!(tab_from("nope").is_err());
        assert_eq!(audience_from("followers").unwrap(), Audience::Followers);
        assert_eq!(audience_from("public-list").unwrap(), Audience::PublicList);
        assert!(audience_from("nope").is_err());
    }

    #[test]
    fn tls_and_ports_are_validated() {
        assert_eq!(tls_from("implicit").unwrap(), "implicit");
        assert!(tls_from("plaintext").is_err());
        assert_eq!(port(993).unwrap(), 993);
        assert!(port(0).is_err());
        assert!(port(70_000).is_err());
    }

    #[test]
    fn helper_readers_trim_and_filter() {
        let request = json!({"a": "  x  ", "b": "   ", "c": 3, "d": true});
        assert_eq!(text(&request, "a"), "x");
        assert_eq!(optional(&request, "b"), None);
        assert_eq!(optional(&request, "missing"), None);
        assert_eq!(number(&request, "c"), Some(3));
        assert!(flag(&request, "d"));
        assert!(!flag(&request, "missing"));
        assert!(required(&request, "b").is_err());
    }
}
