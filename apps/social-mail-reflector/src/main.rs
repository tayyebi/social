//! `social-mail-reflector`: the headless list relay.
//!
//! A reflector is a mailbox, not a service: one operator, one identity, one
//! spool. It keeps an approved-member list, refuses to relay anything whose
//! signature it cannot verify, and re-encrypts one copy per approved member
//! while leaving the original author signature untouched.
//!
//! The operator can read list traffic in transit. That is stated in `init`,
//! in `list-members`, and in `docs/security.md` — never hidden.

use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use social_mail_engine::{DeliveryState, Engine, FollowState, InitOptions, Trust};

#[derive(Parser, Debug)]
#[command(
    name = "social-mail-reflector",
    version,
    about = "Relay a social-mail list from a mailbox you control"
)]
struct Cli {
    #[arg(long, global = true, default_value = "reflector.toml")]
    config: PathBuf,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create the list identity (this mailbox is the list)
    Init {
        /// Address members send to, e.g. list@example.org
        #[arg(long)]
        list: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        spool: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    /// Process one batch and relay new list traffic
    Run {
        #[arg(long)]
        once: bool,
        #[arg(long, default_value_t = 30)]
        interval: u64,
        #[arg(long, default_value_t = 50)]
        batch: usize,
    },
    /// Show approved members and pending requests
    ListMembers {
        #[arg(long)]
        pending: bool,
    },
    /// Approve a member (verifies their key; --force accepts an unchecked one)
    ApproveMember {
        fingerprint: String,
        #[arg(long)]
        force: bool,
    },
    /// Remove a member: they receive nothing from now on
    RemoveMember { fingerprint: String },
    /// Block a member: their posts are never relayed
    BlockMember { fingerprint: String },
    /// Show per-member delivery state
    ShowQueue,
    /// Retry every failed delivery once
    RetryFailed,
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("social-mail-reflector: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init {
            list,
            name,
            spool,
            force,
        } => init(&cli.config, list, name, spool, force),
        Command::Run {
            once,
            interval,
            batch,
        } => run_worker(&cli.config, once, interval, batch, cli.json),
        Command::ListMembers { pending } => {
            let engine = open(&cli.config)?;
            list_members(&engine, pending, cli.json)
        }
        Command::ApproveMember { fingerprint, force } => {
            let mut engine = open(&cli.config)?;
            approve_member(&mut engine, &fingerprint, force, cli.json)
        }
        Command::RemoveMember { fingerprint } => {
            let mut engine = open(&cli.config)?;
            engine.remove_follower(&fingerprint)?;
            println!("Removed {}. They receive nothing from now on.", fingerprint);
            Ok(())
        }
        Command::BlockMember { fingerprint } => {
            let mut engine = open(&cli.config)?;
            engine.block_contact(&fingerprint)?;
            engine.remove_follower(&fingerprint)?;
            println!("Blocked {}. Their posts are never relayed, and nobody is told.", fingerprint);
            Ok(())
        }
        Command::ShowQueue => {
            let engine = open(&cli.config)?;
            show_queue(&engine, cli.json)
        }
        Command::RetryFailed => {
            let mut engine = open(&cli.config)?;
            retry_failed(&mut engine)
        }
    }
}

fn open(config_path: &PathBuf) -> Result<Engine> {
    Engine::open(config_path).map_err(|error| anyhow::anyhow!("{error}"))
}

fn init(
    config_path: &PathBuf,
    list: Option<String>,
    name: Option<String>,
    spool: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    if Engine::exists(config_path) && !force {
        bail!(
            "a reflector already exists at {}; use --force to replace the list identity",
            config_path.display()
        );
    }
    let address = list.unwrap_or_else(|| "list@social-mail.invalid".to_string());
    if !address.contains('@') {
        bail!("--list must be a delivery address such as list@example.org");
    }
    let mut config = social_mail_engine::Config::load(config_path)?;
    if let Some(spool) = spool {
        config.spool_dir = spool;
        config.save(config_path)?;
    }
    let engine = Engine::create(
        config_path,
        InitOptions {
            display_name: name.unwrap_or_else(|| format!("list {address}")),
            address,
            locale: "en".to_string(),
        },
    )?;
    println!("List identity created.");
    println!("Address      {}", engine.profile().address);
    println!("Fingerprint  {}", group(&engine.fingerprint()));
    println!("Data         {}", engine.data_dir().display());
    println!();
    println!("Publish that fingerprint out of band: there is no directory.");
    println!("Members ask to join; approve them with `approve-member`.");
    println!("The operator can read list traffic while it is relayed — say so plainly.");
    Ok(())
}

fn run_worker(
    config_path: &PathBuf,
    once: bool,
    interval: u64,
    batch: usize,
    json: bool,
) -> Result<()> {
    let mut engine = open(config_path)?;
    let list = engine.profile().address.clone();
    let mut cycle: u64 = 0;
    loop {
        cycle += 1;
        let sync = engine.sync()?;
        let candidates = engine.relay_candidates(&list, batch)?;
        let mut relayed = 0usize;
        let mut failed = 0usize;
        let mut refused: Vec<(String, String)> = Vec::new();
        for row in &candidates {
            match engine.redistribute(&row.event_id) {
                Ok(report) => {
                    relayed += report.sent;
                    failed += report.failed;
                }
                Err(error) => refused.push((row.event_id.clone(), error.to_string())),
            }
        }
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "cycle": cycle,
                    "received": sync.received,
                    "accepted": sync.accepted,
                    "quarantined": sync.quarantined,
                    "candidates": candidates.len(),
                    "relayed": relayed,
                    "failed": failed,
                    "refused": refused.iter().map(|(id, reason)| serde_json::json!({
                        "event": id, "reason": reason
                    })).collect::<Vec<_>>(),
                })
            );
        } else {
            println!(
                "cycle {cycle}: {} in, {relayed} out, {failed} failed, {} refused",
                sync.received,
                refused.len()
            );
        }
        for (event_id, reason) in refused {
            eprintln!("  not relayed {}: {reason}", short(&event_id));
        }
        if once {
            return Ok(());
        }
        sleep(Duration::from_secs(interval.max(1)));
    }
}

fn list_members(engine: &Engine, pending_only: bool, json: bool) -> Result<()> {
    let members = engine.members()?;
    let pending = engine.pending_requests()?;
    if json {
        let rows: Vec<_> = members
            .iter()
            .filter(|row| !pending_only || row.state == FollowState::Pending)
            .map(|row| {
                serde_json::json!({
                    "fingerprint": row.follower,
                    "state": row.state.as_str(),
                    "verified": row.verified,
                })
            })
            .collect();
        let requests: Vec<_> = pending
            .iter()
            .map(|(row, contact)| serde_json::json!({
                "fingerprint": row.follower,
                "name": contact.as_ref().map(|c| c.display_name.clone()),
                "trust": contact.as_ref().map(|c| c.trust.as_str().to_string()),
                "received": row.updated_at,
            }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "list": engine.profile().address,
                "members": rows,
                "pending": requests,
            }))?
        );
        return Ok(());
    }

    println!("List         {}", engine.profile().address);
    println!("Fingerprint  {}", group(&engine.fingerprint()));
    println!("Operator can read list traffic while relaying it.\n");
    if !pending_only {
        println!("MEMBERS");
        if members.is_empty() {
            println!("  none approved yet");
        }
        for row in &members {
            let name = engine
                .contact(&row.follower)?
                .map(|contact| contact.display_name)
                .unwrap_or_else(|| short(&row.follower));
            println!(
                "  {:<20} {:<10} verified={}  {}",
                name,
                row.state.as_str(),
                row.verified,
                short(&row.follower)
            );
        }
        println!();
    }
    println!("PENDING REQUESTS");
    if pending.is_empty() {
        println!("  none");
    }
    for (row, contact) in pending {
        let trust = contact
            .as_ref()
            .map(|c| c.trust.as_str())
            .unwrap_or("unknown");
        println!(
            "  {}  ({trust})\n    approve-member {}   # add --force for an unchecked key",
            group(&row.follower),
            short(&row.follower)
        );
    }
    Ok(())
}

fn approve_member(engine: &mut Engine, fingerprint: &str, force: bool, json: bool) -> Result<()> {
    let fingerprint = engine.resolve_fingerprint(fingerprint)?;
    let contact = engine
        .contact(&fingerprint)?
        .ok_or_else(|| anyhow::anyhow!("no contact {fingerprint}; they must request membership first"))?;
    if contact.trust != Trust::Verified && !force {
        bail!(
            "{} key is unverified. Compare {} out of band, then re-run with --force.\n\
             Unverified members are never relayed to.",
            contact.display_name,
            group(&fingerprint)
        );
    }
    engine.approve_follower(&fingerprint, true)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "fingerprint": fingerprint,
                "state": "following",
                "verified": true,
            }))?
        );
    } else {
        println!("Approved {} ({})", contact.display_name, group(&fingerprint));
        println!("They receive new list posts; the accept notice is on its way.");
    }
    Ok(())
}

fn show_queue(engine: &Engine, json: bool) -> Result<()> {
    let items = engine.queue(200)?;
    if json {
        let rows: Vec<_> = items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "post": item.post_event_id,
                    "preview": item.preview,
                    "deliveries": item.deliveries.iter().map(|row| serde_json::json!({
                        "recipient": row.recipient,
                        "state": row.state.as_str(),
                        "label": DeliveryState::label(row.state),
                        "reason": row.reason,
                        "attempts": row.attempts,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"queue": rows}))?);
        return Ok(());
    }
    if items.is_empty() {
        println!("Nothing queued.");
        return Ok(());
    }
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
            println!("   {name:<20} {}{reason}", row.state.label());
        }
    }
    Ok(())
}

fn retry_failed(engine: &mut Engine) -> Result<()> {
    let items = engine.queue(200)?;
    let mut retried = 0usize;
    for item in items {
        for row in item.deliveries {
            if matches!(row.state, DeliveryState::Failed | DeliveryState::Retrying) {
                engine.retry_delivery(&item.post_event_id, &row.recipient)?;
                retried += 1;
                println!("Retried {} -> {}", short(&item.post_event_id), short(&row.recipient));
            }
        }
    }
    if retried == 0 {
        println!("Nothing failed. The queue is clean.");
    }
    Ok(())
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
    social_mail_engine::group_fingerprint(fingerprint)
}
