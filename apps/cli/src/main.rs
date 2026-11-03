//! `social-mail`: a decentralised, email-based social application.
//!
//! The binary is a thin shell: every action routes through
//! `social_mail_engine::Engine`, so the CLI and the TUI cannot drift apart.

mod commands;
mod text;
mod tui;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use social_mail_engine::{Config, Engine};

#[derive(Parser, Debug)]
#[command(
    name = "social-mail",
    version,
    about = "Decentralised, email-based social app",
    long_about = "Your identity is a key. Your posts travel as email. Nobody runs a server \
that decides who you can follow."
)]
struct Cli {
    /// Path to the configuration file
    #[arg(long, global = true, default_value = "social-mail.toml")]
    config: PathBuf,
    /// Machine-readable output
    #[arg(long, global = true)]
    json: bool,
    /// Launch the interactive interface
    #[arg(long)]
    tui: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create this machine's identity and profile
    Init {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        address: Option<String>,
        #[arg(long)]
        locale: Option<String>,
        /// Shared spool directory used by the file transport
        #[arg(long)]
        spool: Option<PathBuf>,
        /// Reinitialise even when a profile already exists
        #[arg(long)]
        force: bool,
    },
    /// Open the interactive interface
    Tui,
    /// Inspect, export or restore the identity
    Identity {
        #[command(subcommand)]
        cmd: IdentityCmd,
    },
    /// Manage contacts
    Contacts {
        #[command(subcommand)]
        cmd: ContactsCmd,
    },
    /// Follow, approve, decline and remove
    Follow {
        #[command(subcommand)]
        cmd: FollowCmd,
    },
    /// Publish a post
    Post {
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        stdin: bool,
        /// Publish to a public mailing list instead of followers
        #[arg(long)]
        public_list: Option<String>,
    },
    /// Reply to an event
    Reply {
        event_id: String,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        stdin: bool,
    },
    /// Read the timeline
    Timeline {
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// following | lists | you | all
        #[arg(long)]
        tab: Option<String>,
        /// Open a conversation
        #[arg(long)]
        thread: Option<String>,
        /// Show replies addressed to you
        #[arg(long)]
        replies: bool,
    },
    /// Exchange pending messages with the transport
    Sync,
    /// Inspect and repair deliveries
    Queue {
        #[command(subcommand)]
        cmd: QueueCmd,
    },
    /// Manage public mailing lists
    Lists {
        #[command(subcommand)]
        cmd: ListsCmd,
    },
    /// Read and change settings
    Settings {
        #[command(subcommand)]
        cmd: SettingsCmd,
    },
    /// Inspect quarantined messages
    Problems {
        #[command(subcommand)]
        cmd: ProblemsCmd,
    },
    /// Import or export events as files (offline exchange)
    Bundle {
        #[command(subcommand)]
        cmd: BundleCmd,
    },
    /// Show storage and sync diagnostics
    Diagnostics,
}

#[derive(Subcommand, Debug)]
pub enum IdentityCmd {
    Show,
    ExportPublic {
        #[arg(long)]
        out: PathBuf,
    },
    ExportBackup {
        #[arg(long)]
        out: PathBuf,
        /// Passphrase (otherwise SOCIAL_MAIL_PASSPHRASE is used)
        #[arg(long)]
        passphrase: Option<String>,
    },
    ImportBackup {
        file: PathBuf,
        #[arg(long)]
        passphrase: Option<String>,
    },
    ContactCard {
        #[arg(long)]
        out: PathBuf,
    },
    Revocation {
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum ContactsCmd {
    Import { file: PathBuf },
    List,
    Show { fingerprint: String },
    Verify { fingerprint: String },
    Block { fingerprint: String },
    Unblock { fingerprint: String },
    Mute { fingerprint: String },
    Unmute { fingerprint: String },
}

#[derive(Subcommand, Debug)]
pub enum FollowCmd {
    Request {
        target: String,
    },
    Approve {
        fingerprint: String,
        /// Approve without verifying the fingerprint (withheld from private posts)
        #[arg(long)]
        force: bool,
    },
    Decline {
        fingerprint: String,
    },
    Remove {
        fingerprint: String,
    },
    Unfollow {
        fingerprint: String,
    },
    List,
    Requests,
}

#[derive(Subcommand, Debug)]
pub enum QueueCmd {
    Status,
    Retry { post: String, recipient: String },
    Cancel { post: String, recipient: String },
}

#[derive(Subcommand, Debug)]
pub enum ListsCmd {
    Add { address: String },
    Remove { address: String },
    List,
}

#[derive(Subcommand, Debug)]
pub enum SettingsCmd {
    Show,
    SetMailFolder {
        /// true or false
        value: String,
    },
    SetLocale {
        value: String,
    },
    SetColor {
        value: String,
    },
    SetDensity {
        tab: String,
        value: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProblemsCmd {
    List,
    Dismiss { event_id: String },
}

#[derive(Subcommand, Debug)]
pub enum BundleCmd {
    Export { file: PathBuf },
    Import { file: PathBuf },
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("social-mail: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let config_path = cli.config.clone();
    let json = cli.json;

    if cli.tui {
        return open_or_setup(&config_path).and_then(tui::run);
    }

    match cli.command {
        None => open_or_setup(&config_path).and_then(tui::run),
        Some(Command::Tui) => open_or_setup(&config_path).and_then(tui::run),
        Some(Command::Init {
            name,
            address,
            locale,
            spool,
            force,
        }) => {
            let mut config = Config::load(&config_path)?;
            if let Some(spool) = spool {
                config.spool_dir = spool;
                config.save(&config_path)?;
            }
            let options = InitOptions {
                display_name: name.unwrap_or_default(),
                address: address.unwrap_or_default(),
                locale: locale.unwrap_or_default(),
            };
            let engine = commands::init(&config_path, options, force, json)?;
            if !json {
                commands::print_identity(&engine)?;
            }
            Ok(())
        }
        Some(command) => {
            let mut engine = Engine::open(&config_path)?;
            commands::run(&mut engine, command, json)
        }
    }
}

/// Open the profile, or run the guided wizard when there is none yet (D3).
fn open_or_setup(config_path: &std::path::Path) -> Result<Engine> {
    if Engine::exists(config_path) {
        return Engine::open(config_path).map_err(Into::into);
    }
    commands::init(config_path, Default::default(), false, false)
}
