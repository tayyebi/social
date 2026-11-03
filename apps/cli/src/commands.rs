//! Command implementations. Every handler is a thin wrapper over the engine.

use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::json;
use social_mail_engine::{
    Audience, Config, ConversationItem, DeliveryState, Engine, FeedItem, ImportOutcome,
    InitOptions, Tab, Trust,
};

use crate::text;
use crate::{
    BundleCmd, Command, ContactsCmd, FollowCmd, IdentityCmd, ListsCmd, ProblemsCmd, QueueCmd,
    SettingsCmd,
};

const PLACEHOLDER_ADDRESS: &str = "local@social-mail.invalid";

// ---------------------------------------------------------------------- init

pub fn init(
    config_path: &Path,
    mut options: InitOptions,
    force: bool,
    json_output: bool,
) -> Result<Engine> {
    if Engine::exists(config_path) && !force {
        bail!(
            "a profile already exists at {}; use --force to reinitialise",
            config_path.display()
        );
    }

    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    if interactive && !json_output {
        println!("Your identity is a key. Your posts travel as email.");
        println!("Nobody runs a server that decides who you can follow.\n");
        println!("Let's set up this machine. You'll need about two minutes.\n");
    }

    if options.locale.is_empty() {
        options.locale = if interactive && !json_output {
            prompt("Language / زبان [en/fa] (en): ", "en")
        } else {
            "en".to_string()
        };
        if options.locale != "en" && options.locale != "fa" {
            options.locale = "en".to_string();
        }
    }
    if options.display_name.is_empty() && interactive && !json_output {
        options.display_name = prompt("Display name: ", "social-mail user");
    }
    if options.display_name.is_empty() {
        options.display_name = "social-mail user".to_string();
    }
    if options.address.is_empty() && interactive && !json_output {
        println!("This is the mailbox others reach you at, or blank for no mail route yet.");
        options.address = prompt("Address: ", PLACEHOLDER_ADDRESS);
    }
    if options.address.is_empty() {
        options.address = PLACEHOLDER_ADDRESS.to_string();
    }

    let engine = Engine::create(config_path, options)?;

    if json_output {
        print_json(json!({
            "fingerprint": engine.fingerprint(),
            "display_name": engine.profile().display_name,
            "address": engine.profile().address,
            "locale": engine.config.locale,
            "data_dir": engine.data_dir().display().to_string(),
            "config": config_path.display().to_string(),
            "spool_dir": engine.config.spool_dir.display().to_string(),
        }));
    } else if interactive {
        println!("\nProfile created.\n");
        print_identity(&engine)?;
        println!("Next: share your contact card, then import theirs.");
        println!("  social-mail identity contact-card --out alice.socialmail");
        println!("  social-mail contacts import card-from-bob.socialmail");
    }
    Ok(engine)
}

fn prompt(question: &str, default: &str) -> String {
    print!("{question}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return default.to_string();
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn read_secret(label: &str) -> Result<String> {
    if let Ok(value) = std::env::var("SOCIAL_MAIL_PASSPHRASE") {
        if !value.is_empty() {
            return Ok(value);
        }
    }
    if io::stdin().is_terminal() {
        eprintln!("{label} (input is visible; set SOCIAL_MAIL_PASSPHRASE to avoid this)");
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    } else {
        bail!("no passphrase: set SOCIAL_MAIL_PASSPHRASE")
    }
}

fn read_body(text: Option<String>, file: Option<PathBuf>, stdin: bool) -> Result<String> {
    if let Some(text) = text {
        return Ok(text);
    }
    if let Some(file) = file {
        return Ok(std::fs::read_to_string(file).context("reading the text file")?);
    }
    if stdin {
        let mut body = String::new();
        io::stdin().read_to_string(&mut body)?;
        return Ok(body);
    }
    bail!("no text: pass --text, --file or --stdin")
}

// ------------------------------------------------------------------ dispatch

pub fn run(engine: &mut Engine, command: Command, json_output: bool) -> Result<()> {
    match command {
        Command::Init { .. } | Command::Tui => unreachable!("handled in main"),
        Command::Identity { cmd } => identity(engine, cmd, json_output),
        Command::Contacts { cmd } => contacts(engine, cmd, json_output),
        Command::Follow { cmd } => follow(engine, cmd, json_output),
        Command::Post {
            text,
            file,
            stdin,
            public_list,
        } => post(engine, text, file, stdin, public_list, json_output),
        Command::Reply {
            event_id,
            text,
            stdin,
        } => reply(engine, &event_id, text, stdin, json_output),
        Command::Timeline {
            limit,
            tab,
            thread,
            replies,
        } => timeline(engine, limit, tab, thread, replies, json_output),
        Command::Sync => sync(engine, json_output),
        Command::Queue { cmd } => queue(engine, cmd, json_output),
        Command::Lists { cmd } => lists(engine, cmd, json_output),
        Command::Settings { cmd } => settings(engine, cmd, json_output),
        Command::Problems { cmd } => problems(engine, cmd, json_output),
        Command::Bundle { cmd } => bundle(engine, cmd, json_output),
        Command::Diagnostics => diagnostics(engine, json_output),
    }
}

fn identity(engine: &mut Engine, cmd: IdentityCmd, json_output: bool) -> Result<()> {
    match cmd {
        IdentityCmd::Show => {
            if json_output {
                print_identity(engine)?;
            } else {
                print_identity(engine)?;
            }
            Ok(())
        }
        IdentityCmd::ExportPublic { out } => {
            let card = engine.export_card(&out)?;
            if json_output {
                print_json(json!({"fingerprint": card.fingerprint, "path": out.display().to_string()}));
            } else {
                println!("Contact card written to {}", out.display());
                println!("Fingerprint: {}", social_mail_crypto::group_fingerprint(&card.fingerprint));
            }
            Ok(())
        }
        IdentityCmd::ContactCard { out } => {
            let card = engine.export_card(&out)?;
            if json_output {
                print_json(json!({"fingerprint": card.fingerprint, "path": out.display().to_string()}));
            } else {
                println!("Contact card written to {}", out.display());
                println!("Fingerprint: {}", social_mail_crypto::group_fingerprint(&card.fingerprint));
            }
            Ok(())
        }
        IdentityCmd::ExportBackup { out, passphrase } => {
            let passphrase = match passphrase {
                Some(value) => value,
                None => read_secret("Backup passphrase")?,
            };
            let fingerprint = engine.export_backup(&out, &passphrase)?;
            if json_output {
                print_json(json!({"fingerprint": fingerprint, "path": out.display().to_string()}));
            } else {
                println!("Encrypted backup written to {}", out.display());
                println!("Keep it somewhere you control. Without it there is no recovery.");
            }
            Ok(())
        }
        IdentityCmd::ImportBackup { file, passphrase } => {
            let passphrase = match passphrase {
                Some(value) => value,
                None => read_secret("Backup passphrase")?,
            };
            let fingerprint = engine.import_backup(&file, &passphrase)?;
            if json_output {
                print_json(json!({"fingerprint": fingerprint}));
            } else {
                println!("Identity restored: {fingerprint}");
                println!("Contacts kept their local trust state; re-enter mail settings if needed.");
            }
            Ok(())
        }
        IdentityCmd::Revocation { out } => {
            let statement = engine.revocation_statement()?;
            std::fs::write(&out, &statement)?;
            if json_output {
                print_json(json!({"path": out.display().to_string()}));
            } else {
                println!("Key-loss statement written to {}", out.display());
                println!("It is advisory only: see docs/security.md.");
            }
            Ok(())
        }
    }
}

fn contacts(engine: &mut Engine, cmd: ContactsCmd, json_output: bool) -> Result<()> {
    match cmd {
        ContactsCmd::Import { file } => {
            let outcome = engine.import_card_file(&file)?;
            let message = match &outcome {
                ImportOutcome::Added => "contact added, unverified".to_string(),
                ImportOutcome::Updated => "contact updated".to_string(),
                ImportOutcome::Unchanged => "contact already known".to_string(),
                ImportOutcome::Mismatch { stored } => format!(
                    "KEY MISMATCH: a different key claims this contact ({stored}). Nothing changed."
                ),
                ImportOutcome::OwnCard => "that is your own card".to_string(),
            };
            if json_output {
                print_json(json!({"outcome": format!("{outcome:?}"), "message": message}));
            } else {
                println!("{message}");
            }
            Ok(())
        }
        ContactsCmd::List => {
            let views = engine.contacts_view()?;
            if json_output {
                let items: Vec<_> = views
                    .iter()
                    .map(|view| {
                        json!({
                            "fingerprint": view.contact.fingerprint,
                            "name": view.contact.display_name,
                            "address": view.contact.address,
                            "trust": view.contact.trust.as_str(),
                            "muted": view.contact.muted,
                            "i_follow": view.i_follow.map(|state| state.as_str()),
                            "follows_me": view.follows_me.map(|state| state.as_str()),
                            "pending_request": view.pending_request,
                        })
                    })
                    .collect();
                print_json(json!({"contacts": items}));
            } else if views.is_empty() {
                println!("No contacts yet. Import someone's contact card, or ask for their fingerprint.");
            } else {
                println!(
                    "{:<10} {:<20} {:<12} {:<14} {:<12}",
                    "FINGERPRINT", "NAME", "TRUST", "YOU FOLLOW", "FOLLOWS YOU"
                );
                for view in views {
                    println!(
                        "{:<10} {:<20} {:<12} {:<14} {:<12}",
                        short(&view.contact.fingerprint),
                        truncate(&view.contact.display_name, 20),
                        view.contact.trust.as_str(),
                        follow_label(view.i_follow),
                        follow_label(view.follows_me),
                    );
                    if view.pending_request {
                        println!("           -> pending request: approve or decline");
                    }
                }
            }
            Ok(())
        }
        ContactsCmd::Show { fingerprint } => {
            let fingerprint = engine.resolve_fingerprint(&fingerprint)?;
            let contact = engine
                .contact(&fingerprint)?
                .ok_or_else(|| anyhow::anyhow!("no such contact"))?;
            if json_output {
                print_json(json!({
                    "fingerprint": contact.fingerprint,
                    "name": contact.display_name,
                    "address": contact.address,
                    "trust": contact.trust.as_str(),
                    "muted": contact.muted,
                    "added_at": contact.added_at,
                }));
            } else {
                println!("Name        {}", contact.display_name);
                println!("Address     {}", contact.address);
                println!(
                    "Fingerprint {}",
                    social_mail_crypto::group_fingerprint(&contact.fingerprint)
                );
                println!("Trust       {}", contact.trust.as_str());
                println!("Muted       {}", if contact.muted { "yes" } else { "no" });
                println!("Added       {}", contact.added_at);
            }
            Ok(())
        }
        ContactsCmd::Verify { fingerprint } => {
            engine.verify_contact(&fingerprint)?;
            println!("Contact verified. Their posts are trusted, and they can receive private posts from now on.");
            Ok(())
        }
        ContactsCmd::Block { fingerprint } => {
            engine.block_contact(&fingerprint)?;
            println!("Blocked. New messages from them are discarded and never shown. They are not told.");
            Ok(())
        }
        ContactsCmd::Unblock { fingerprint } => {
            engine.unblock_contact(&fingerprint)?;
            println!("Unblocked. They are unverified again; the backlog is not replayed.");
            Ok(())
        }
        ContactsCmd::Mute { fingerprint } => {
            engine.mute_contact(&fingerprint, true)?;
            println!("Muted. Hidden from your timeline; delivery continues.");
            Ok(())
        }
        ContactsCmd::Unmute { fingerprint } => {
            engine.mute_contact(&fingerprint, false)?;
            println!("Unmuted.");
            Ok(())
        }
    }
}

fn follow(engine: &mut Engine, cmd: FollowCmd, json_output: bool) -> Result<()> {
    match cmd {
        FollowCmd::Request { target } => {
            let fingerprint = engine.resolve_fingerprint(&target)?;
            let contact = engine.contact(&fingerprint)?;
            match contact {
                Some(contact) if contact.trust == Trust::Unverified => {
                    if !json_output {
                        println!("{} isn't verified. Their posts will be marked unverified until you check their fingerprint.", contact.display_name);
                    }
                }
                None => bail!("no contact matches {target}; import their card first"),
                _ => {}
            }
            let event_id = engine.follow(&fingerprint)?;
            if json_output {
                print_json(json!({"event_id": event_id, "state": "pending"}));
            } else {
                println!("Follow request sent (event {event_id}). Waiting for approval.");
            }
            Ok(())
        }
        FollowCmd::Approve { fingerprint, force } => {
            let verify = !force;
            let event_id = engine.approve_follower(&fingerprint, verify)?;
            if json_output {
                print_json(json!({"event_id": event_id, "verified": verify}));
            } else if verify {
                println!("Approved and verified. They can now receive your private posts.");
            } else {
                println!("Approved unverified. They will NOT receive private posts until you verify them.");
            }
            Ok(())
        }
        FollowCmd::Decline { fingerprint } => {
            engine.decline_follower(&fingerprint)?;
            println!("Declined. No message was sent.");
            Ok(())
        }
        FollowCmd::Remove { fingerprint } => {
            engine.remove_follower(&fingerprint)?;
            println!("Removed. They stop receiving new posts; what they already have, they keep.");
            Ok(())
        }
        FollowCmd::Unfollow { fingerprint } => {
            engine.unfollow(&fingerprint)?;
            println!("Unfollowed. You stop receiving their posts; they are not told.");
            Ok(())
        }
        FollowCmd::List => {
            let rows = engine.my_follows()?;
            if json_output {
                let items: Vec<_> = rows
                    .iter()
                    .map(|row| json!({"followee": row.followee, "state": row.state.as_str(), "verified": row.verified}))
                    .collect();
                print_json(json!({"following": items}));
            } else if rows.is_empty() {
                println!("You don't follow anyone yet.");
            } else {
                for row in rows {
                    println!(
                        "{:<10} {:<10} verified={}",
                        short(&row.followee),
                        row.state.as_str(),
                        row.verified
                    );
                }
            }
            Ok(())
        }
        FollowCmd::Requests => {
            let requests = engine.pending_requests()?;
            if json_output {
                let items: Vec<_> = requests
                    .iter()
                    .map(|(row, contact)| {
                        json!({
                            "fingerprint": row.follower,
                            "name": contact.as_ref().map(|c| c.display_name.clone()),
                            "trust": contact.as_ref().map(|c| c.trust.as_str().to_string()),
                            "received": row.updated_at,
                        })
                    })
                    .collect();
                print_json(json!({"requests": items}));
            } else if requests.is_empty() {
                println!("No follow requests. People who want your private posts appear here first.");
            } else {
                for (row, contact) in requests {
                    let name = contact
                        .as_ref()
                        .map(|c| c.display_name.clone())
                        .unwrap_or_else(|| short(&row.follower));
                    let trust = contact
                        .as_ref()
                        .map(|c| c.trust.as_str())
                        .unwrap_or("unknown");
                    println!("{name}  ({trust})  {}", group(&row.follower));
                    println!(
                        "  approve: social-mail follow approve {}   decline: social-mail follow decline {}",
                        short(&row.follower),
                        short(&row.follower)
                    );
                }
            }
            Ok(())
        }
    }
}

fn post(
    engine: &mut Engine,
    text_option: Option<String>,
    file: Option<PathBuf>,
    stdin: bool,
    public_list: Option<String>,
    json_output: bool,
) -> Result<()> {
    let body = read_body(text_option, file, stdin)?;
    let (audience, list) = match public_list {
        Some(list) => (Audience::PublicList, Some(list)),
        None => (Audience::Followers, None),
    };
    let event_id = engine.post(&body, audience, list.clone())?;
    if json_output {
        print_json(json!({
            "event_id": event_id,
            "audience": match audience { Audience::Followers => "followers", Audience::PublicList => "public-list" },
            "list": list,
        }));
    } else {
        println!("Posted ({event_id}).");
        match audience {
            Audience::Followers => println!(
                "Audience: followers. Encrypted separately for each verified follower; nothing is sent to unverified ones."
            ),
            Audience::PublicList => println!(
                "Audience: public list {}. Stored locally; SMTP delivery lands in the next milestone (see TODO.md).",
                list.unwrap_or_default()
            ),
        }
    }
    Ok(())
}

fn reply(
    engine: &mut Engine,
    event_id: &str,
    text_option: Option<String>,
    stdin: bool,
    json_output: bool,
) -> Result<()> {
    let body = read_body(text_option, None, stdin)?;
    let new_id = engine.reply(event_id, &body)?;
    if json_output {
        print_json(json!({"event_id": new_id, "parent": event_id}));
    } else {
        println!("Reply sent ({new_id}).");
    }
    Ok(())
}

fn timeline(
    engine: &mut Engine,
    limit: usize,
    tab: Option<String>,
    thread: Option<String>,
    replies: bool,
    json_output: bool,
) -> Result<()> {
    if let Some(root) = thread {
        let items = engine.conversation(&root)?;
        if json_output {
            print_json(json!({"root": root, "items": items.iter().map(conversation_json).collect::<Vec<_>>()}));
        } else {
            print_conversation(&items);
        }
        return Ok(());
    }
    if replies {
        let items = engine.replies_to_me(limit)?;
        if json_output {
            print_json(json!({"replies": items.iter().map(conversation_json).collect::<Vec<_>>()}));
        } else if items.is_empty() {
            println!("No replies to you yet.");
        } else {
            for item in items {
                println!(
                    "{}  {}  {}  {}",
                    short(&item.event_id),
                    item.created_at,
                    item.author_name,
                    trust_label(item.trust)
                );
                for line in text::layout(&item.text, 74) {
                    println!("   {line}");
                }
            }
        }
        return Ok(());
    }

    let tab = parse_tab(tab.as_deref())?;
    let items = engine.timeline(tab, limit)?;
    if json_output {
        print_json(json!({
            "tab": tab.as_str(),
            "items": items.iter().map(feed_json).collect::<Vec<_>>(),
        }));
    } else {
        print_feed(tab, &items);
    }
    Ok(())
}

fn sync(engine: &mut Engine, json_output: bool) -> Result<()> {
    let report = engine.sync()?;
    if json_output {
        print_json(json!({
            "received": report.received,
            "accepted": report.accepted,
            "duplicates": report.duplicates,
            "dropped": report.dropped,
            "quarantined": report.quarantined,
            "sent": report.sent,
            "failed": report.failed,
        }));
    } else {
        println!(
            "Sync: {} received, {} accepted, {} duplicates, {} dropped, {} quarantined",
            report.received, report.accepted, report.duplicates, report.dropped, report.quarantined
        );
        println!("Deliveries: {} accepted, {} failed/retrying", report.sent, report.failed);
    }
    Ok(())
}

fn queue(engine: &mut Engine, cmd: QueueCmd, json_output: bool) -> Result<()> {
    match cmd {
        QueueCmd::Status => {
            let items = engine.queue(200)?;
            if json_output {
                let value: Vec<_> = items
                    .iter()
                    .map(|item| {
                        json!({
                            "post": item.post_event_id,
                            "preview": item.preview,
                            "deliveries": item.deliveries.iter().map(|row| json!({
                                "recipient": row.recipient,
                                "state": row.state.as_str(),
                                "label": row.state.label(),
                                "reason": row.reason,
                                "attempts": row.attempts,
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                print_json(json!({"queue": value}));
            } else if items.is_empty() {
                println!("Nothing queued. Everything you've sent has been accepted by your transport.");
            } else {
                for item in items {
                    println!("{}  {}", short(&item.post_event_id), item.preview);
                    for row in item.deliveries {
                        let name = engine
                            .contact(&row.recipient)?
                            .map(|contact| contact.display_name)
                            .unwrap_or_else(|| short(&row.recipient));
                        let reason = row
                            .reason
                            .map(|reason| format!("  ({reason})"))
                            .unwrap_or_default();
                        println!("   {:<16} {}{}", name, row.state.label(), reason);
                    }
                }
            }
            Ok(())
        }
        QueueCmd::Retry { post, recipient } => {
            let recipient = engine.resolve_fingerprint(&recipient)?;
            engine.retry_delivery(&post, &recipient)?;
            println!("Retry queued.");
            Ok(())
        }
        QueueCmd::Cancel { post, recipient } => {
            let recipient = engine.resolve_fingerprint(&recipient)?;
            engine.cancel_delivery(&post, &recipient)?;
            println!("Delivery cancelled.");
            Ok(())
        }
    }
}

fn lists(engine: &mut Engine, cmd: ListsCmd, json_output: bool) -> Result<()> {
    match cmd {
        ListsCmd::Add { address } => {
            engine.add_list(&address)?;
            println!("Added list {address}. Public list posts to it are signed, not encrypted.");
            Ok(())
        }
        ListsCmd::Remove { address } => {
            engine.remove_list(&address)?;
            println!("Removed list {address}. Past items stay in your timeline.");
            Ok(())
        }
        ListsCmd::List => {
            let lists = engine.lists()?;
            if json_output {
                print_json(json!({"lists": lists}));
            } else if lists.is_empty() {
                println!("No lists. Add a public list to read it, or follow people instead.");
            } else {
                for list in lists {
                    println!("{list}");
                }
            }
            Ok(())
        }
    }
}

fn settings(engine: &mut Engine, cmd: SettingsCmd, json_output: bool) -> Result<()> {
    match cmd {
        SettingsCmd::Show => {
            if json_output {
                print_json(json!({
                    "config": engine.config.as_map(),
                    "config_path": engine.config_path().display().to_string(),
                    "data_dir": engine.data_dir().display().to_string(),
                    "spool_dir": engine.config.spool_dir.display().to_string(),
                }));
            } else {
                println!("config      {}", engine.config_path().display());
                println!("data        {}", engine.data_dir().display());
                println!("spool       {}", engine.config.spool_dir.display());
                for (key, value) in engine.config.as_map() {
                    println!("{key:<32} {value}");
                }
                println!("{:<32} {}", "ui.density_following", engine.config.density_following);
                println!("{:<32} {}", "ui.density_lists", engine.config.density_lists);
                println!("{:<32} {}", "ui.density_you", engine.config.density_you);
                println!("{:<32} {}", "ui.density_all", engine.config.density_all);
            }
            Ok(())
        }
        SettingsCmd::SetMailFolder { value } => {
            let enabled = parse_bool(&value)?;
            engine.set_mail_folder(enabled)?;
            println!(
                "Protocol folder moves {}.",
                if enabled { "enabled" } else { "disabled" }
            );
            Ok(())
        }
        SettingsCmd::SetLocale { value } => {
            engine.set_locale(&value)?;
            println!("UI language set to {value}.");
            Ok(())
        }
        SettingsCmd::SetColor { value } => {
            engine.set_color(&value)?;
            println!("Colour mode set to {value}.");
            Ok(())
        }
        SettingsCmd::SetDensity { tab, value } => {
            let tab = parse_tab(Some(&tab))?;
            engine.set_density(tab, &value)?;
            println!("Density for {} set to {value}.", tab.as_str());
            Ok(())
        }
    }
}

fn problems(engine: &mut Engine, cmd: ProblemsCmd, json_output: bool) -> Result<()> {
    match cmd {
        ProblemsCmd::List => {
            let rows = engine.problems()?;
            if json_output {
                let value: Vec<_> = rows
                    .iter()
                    .map(|row| json!({
                        "event_id": row.event_id,
                        "author": row.author,
                        "kind": row.kind,
                        "reason": row.reason,
                        "detail": row.detail,
                        "created_at": row.created_at,
                    }))
                    .collect();
                print_json(json!({"problems": value}));
            } else if rows.is_empty() {
                println!("No problems. Every message that arrived verified.");
            } else {
                for row in rows {
                    println!(
                        "{}  {}  {}",
                        short(&row.event_id),
                        row.reason,
                        row.created_at
                    );
                    if let Some(detail) = row.detail {
                        for line in text::layout(&detail, 72) {
                            println!("   {line}");
                        }
                    }
                }
            }
            Ok(())
        }
        ProblemsCmd::Dismiss { event_id } => {
            engine.dismiss_problem(&event_id)?;
            println!("Dismissed {event_id}.");
            Ok(())
        }
    }
}

fn bundle(engine: &mut Engine, cmd: BundleCmd, json_output: bool) -> Result<()> {
    match cmd {
        BundleCmd::Export { file } => {
            let count = engine.export_bundle(&file)?;
            if json_output {
                print_json(json!({"carriers": count, "path": file.display().to_string()}));
            } else {
                println!("Wrote {count} signed events to {}", file.display());
                println!("Send this file to the other person over any channel you trust.");
            }
            Ok(())
        }
        BundleCmd::Import { file } => {
            let outcomes = engine.import_bundle(&file)?;
            let accepted = outcomes
                .iter()
                .filter(|outcome| matches!(outcome, social_mail_engine::IngestOutcome::Accepted))
                .count();
            if json_output {
                print_json(json!({
                    "results": outcomes.iter().map(|outcome| format!("{outcome:?}")).collect::<Vec<_>>(),
                }));
            } else {
                println!("Imported {accepted} of {} events.", outcomes.len());
                for outcome in &outcomes {
                    if let social_mail_engine::IngestOutcome::Quarantined(reason) = outcome {
                        println!("  quarantined: {reason}");
                    }
                }
            }
            Ok(())
        }
    }
}

fn diagnostics(engine: &mut Engine, json_output: bool) -> Result<()> {
    let counts = engine.counts()?;
    let problems = engine.problems()?;
    let pending = engine.queue(200)?;
    if json_output {
        print_json(json!({
            "fingerprint": engine.fingerprint(),
            "config": engine.config_path().display().to_string(),
            "data_dir": engine.data_dir().display().to_string(),
            "spool_dir": engine.config.spool_dir.display().to_string(),
            "transport": engine.config.transport,
            "counts": {
                "events": counts.events,
                "roots": counts.roots,
                "replies": counts.replies,
                "contacts": counts.contacts,
                "verified_contacts": counts.verified_contacts,
                "queued_deliveries": counts.queued_deliveries,
                "failed_deliveries": counts.failed_deliveries,
                "withheld_deliveries": counts.withheld_deliveries,
                "problems": counts.problems,
                "unseen_following": counts.unseen_following,
            },
            "queue_groups": pending.len(),
            "problem_rows": problems.len(),
        }));
    } else {
        println!("Identity    {}", group(&engine.fingerprint()));
        println!("Config      {}", engine.config_path().display());
        println!("Data        {}", engine.data_dir().display());
        println!("Spool       {}", engine.config.spool_dir.display());
        println!("Transport   {}", engine.config.transport);
        println!();
        println!("Events      {} ({} roots, {} replies)", counts.events, counts.roots, counts.replies);
        println!(
            "Contacts    {} ({} verified)",
            counts.contacts, counts.verified_contacts
        );
        println!(
            "Deliveries  {} pending, {} failed, {} withheld",
            counts.queued_deliveries, counts.failed_deliveries, counts.withheld_deliveries
        );
        println!("Problems    {}", counts.problems);
        println!("Unseen      {} in Following", counts.unseen_following);
        if counts.problems > 0 {
            println!("\nRun `social-mail problems list` to see what was rejected and why.");
        }
        if counts.failed_deliveries > 0 {
            println!("Run `social-mail queue status` to inspect failed deliveries.");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- printing

pub fn print_identity(engine: &Engine) -> Result<()> {
    println!(
        "Fingerprint {}",
        social_mail_crypto::group_fingerprint(&engine.fingerprint())
    );
    println!("Name        {}", engine.profile().display_name);
    println!("Address     {}", engine.profile().address);
    println!("Language    {}", engine.config.locale);
    println!("Data        {}", engine.data_dir().display());
    println!("Config      {}", engine.config_path().display());
    println!("Spool       {}", engine.config.spool_dir.display());
    Ok(())
}

fn print_feed(tab: Tab, items: &[FeedItem]) {
    if tab == Tab::Following {
        println!("# followers · newest first · encrypted");
    }
    let mut current_day = String::new();
    for item in items {
        let day = item.created_at.chars().take(10).collect::<String>();
        if day != current_day {
            println!("\n── {day} ──────────────────────────────────────");
            current_day = day;
        }
        let marker = if item.seen { " " } else { "•" };
        let audience = match item.audience {
            Audience::Followers => "private".to_string(),
            Audience::PublicList => format!("list:{}", item.list_id.clone().unwrap_or_default()),
        };
        println!(
            "{marker} {:>6}  {}  {}  {}{}",
            short(&item.event_id),
            item.author_name,
            trust_label(item.trust),
            audience,
            item.delivery_summary
                .as_ref()
                .map(|summary| format!("  {summary}"))
                .unwrap_or_default()
        );
        for line in text::layout(&item.text, 74) {
            println!("    {line}");
        }
        if item.reply_count > 0 {
            println!("    {} replies · social-mail timeline --thread {}", item.reply_count, item.event_id);
        }
    }
    if items.is_empty() {
        println!("Nothing here yet. When someone you follow posts, it appears here.");
    }
}

fn print_conversation(items: &[ConversationItem]) {
    for item in items {
        let indent = "   ".repeat(item.depth);
        println!(
            "{indent}{}  {}  {}",
            short(&item.event_id),
            item.author_name,
            trust_label(item.trust)
        );
        for line in text::layout(&item.text, 74usize.saturating_sub(indent.len())) {
            println!("{indent}  {line}");
        }
    }
    if items.is_empty() {
        println!("This conversation is empty or its parent has not arrived yet.");
    }
}

fn feed_json(item: &FeedItem) -> serde_json::Value {
    json!({
        "event_id": item.event_id,
        "position": item.position,
        "author": item.author,
        "author_name": item.author_name,
        "trust": item.trust.as_str(),
        "audience": match item.audience { Audience::Followers => "followers", Audience::PublicList => "public-list" },
        "list": item.list_id,
        "text": item.text,
        "created_at": item.created_at,
        "received_at": item.received_at,
        "seen": item.seen,
        "reply_count": item.reply_count,
        "delivery_summary": item.delivery_summary,
        "mine": item.is_mine,
    })
}

fn conversation_json(item: &ConversationItem) -> serde_json::Value {
    json!({
        "event_id": item.event_id,
        "author": item.author,
        "author_name": item.author_name,
        "trust": item.trust.as_str(),
        "text": item.text,
        "created_at": item.created_at,
        "depth": item.depth,
        "kind": item.kind,
    })
}

fn print_json(value: serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()));
}

// -------------------------------------------------------------------- helpers

fn parse_tab(value: Option<&str>) -> Result<Tab> {
    match value.unwrap_or("following") {
        "following" => Ok(Tab::Following),
        "lists" => Ok(Tab::Lists),
        "you" => Ok(Tab::You),
        "all" => Ok(Tab::All),
        other => bail!("unknown tab {other:?}; use following, lists, you or all"),
    }
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "true" | "yes" | "1" | "on" => Ok(true),
        "false" | "no" | "0" | "off" => Ok(false),
        other => bail!("expected true or false, got {other:?}"),
    }
}

fn short(fingerprint: &str) -> String {
    fingerprint
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn group(fingerprint: &str) -> String {
    social_mail_crypto::group_fingerprint(fingerprint)
}

fn trust_label(trust: Trust) -> &'static str {
    match trust {
        Trust::Verified => "verified",
        Trust::Unverified => "unverified",
        Trust::Blocked => "blocked",
    }
}

fn follow_label(state: Option<social_mail_engine::FollowState>) -> &'static str {
    match state {
        None => "-",
        Some(social_mail_engine::FollowState::Pending) => "pending",
        Some(social_mail_engine::FollowState::Following) => "yes",
        Some(social_mail_engine::FollowState::Declined) => "declined",
        Some(social_mail_engine::FollowState::Removed) => "removed",
    }
}

fn truncate(value: &str, width: usize) -> String {
    text::truncate(value, width)
}

/// Delivery label used by callers that want to print the honest wording (D16).
pub fn delivery_label(state: DeliveryState) -> &'static str {
    state.label()
}
