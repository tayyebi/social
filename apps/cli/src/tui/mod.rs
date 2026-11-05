//! The interactive interface.
//!
//! Layout follows the user-journey plan: source tabs over a sticky feed of cards,
//! a one-line status bar, banners that never steal focus, and modals only for
//! actions the user just started.

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::{Frame, Terminal};

use social_mail_engine::{
    Audience, ContactView, ConversationItem, DeliveryState, Engine, FeedItem, QueueItem, Tab, Trust,
};

use crate::text;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Timeline,
    Conversation,
    Contacts,
    Requests,
    Queue,
    Replies,
    Lists,
    Settings,
    Problems,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Density {
    Compact,
    Cozy,
    Full,
}

impl Density {
    fn cycle(self) -> Self {
        match self {
            Density::Compact => Density::Cozy,
            Density::Cozy => Density::Full,
            Density::Full => Density::Compact,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Density::Compact => "compact",
            Density::Cozy => "cozy",
            Density::Full => "full",
        }
    }
}

struct Composer {
    text: String,
    reply_to: Option<String>,
}

pub struct App {
    engine: Engine,
    screen: Screen,
    tab: Tab,
    density: Density,
    selected: usize,
    card_lines: Vec<usize>,
    feed: Vec<FeedItem>,
    conversation: Vec<ConversationItem>,
    conversation_root: String,
    contacts: Vec<ContactView>,
    requests: Vec<(String, Option<String>, Trust)>,
    queue: Vec<QueueItem>,
    replies: Vec<ConversationItem>,
    problems: Vec<social_mail_storage::ProblemRow>,
    composer: Option<Composer>,
    status: String,
    help: bool,
    pending_g: bool,
    last_sync: Option<Instant>,
}

impl App {
    fn new(engine: Engine) -> Self {
        let tab = Tab::Following;
        let density = density_from(&engine, tab);
        let mut app = App {
            engine,
            screen: Screen::Timeline,
            tab,
            density,
            selected: 0,
            card_lines: Vec::new(),
            feed: Vec::new(),
            conversation: Vec::new(),
            conversation_root: String::new(),
            contacts: Vec::new(),
            requests: Vec::new(),
            queue: Vec::new(),
            replies: Vec::new(),
            problems: Vec::new(),
            composer: None,
            status: String::new(),
            help: false,
            pending_g: false,
            last_sync: None,
        };
        app.refresh();
        if let Ok(Some(error)) = app.engine.transport_error() {
            if !error.trim().is_empty() {
                app.status = error;
            }
        }
        let _ = app.sync();
        app
    }

    fn sync(&mut self) -> Result<()> {
        match self.engine.sync() {
            Ok(report) => {
                self.last_sync = Some(Instant::now());
                self.status = format!(
                    "{} in · {} out · {} quarantined",
                    report.received, report.sent, report.quarantined
                );
                self.refresh();
                Ok(())
            }
            Err(error) => {
                self.status = format!("sync failed: {error}");
                Ok(())
            }
        }
    }

    fn refresh(&mut self) {
        self.feed = self.engine.timeline(self.tab, 200).unwrap_or_default();
        self.contacts = self.engine.contacts_view().unwrap_or_default();
        self.requests = self
            .engine
            .pending_requests()
            .unwrap_or_default()
            .into_iter()
            .map(|(row, contact)| {
                (
                    row.follower,
                    contact.as_ref().map(|c| c.display_name.clone()),
                    contact.map(|c| c.trust).unwrap_or(Trust::Unverified),
                )
            })
            .collect();
        self.queue = self.engine.queue(100).unwrap_or_default();
        self.replies = self.engine.replies_to_me(100).unwrap_or_default();
        self.problems = self.engine.problems().unwrap_or_default();
        if let Screen::Conversation = self.screen {
            if !self.conversation_root.is_empty() {
                self.conversation = self
                    .engine
                    .conversation(&self.conversation_root)
                    .unwrap_or_default();
            }
        }
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let len = match self.screen {
            Screen::Timeline => self.feed.len(),
            Screen::Conversation => self.conversation.len(),
            Screen::Contacts => self.contacts.len(),
            Screen::Requests => self.requests.len(),
            Screen::Queue => self.queue.len(),
            Screen::Replies => self.replies.len(),
            Screen::Problems => self.problems.len(),
            Screen::Lists | Screen::Settings => 0,
        };
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    // ------------------------------------------------------------------ input

    fn handle(&mut self, key: KeyEvent) -> Result<bool> {
        if self.composer.is_some() {
            return self.handle_composer(key);
        }
        if self.help {
            self.help = false;
            return Ok(false);
        }
        if self.pending_g {
            self.pending_g = false;
            return self.handle_g(key);
        }
        match key.code {
            KeyCode::Char('q') => {
                if self.screen == Screen::Timeline {
                    return Ok(true);
                }
                self.screen = Screen::Timeline;
                self.selected = 0;
            }
            KeyCode::Esc => {
                self.screen = Screen::Timeline;
                self.selected = 0;
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('r') => {
                self.sync()?;
            }
            KeyCode::Char('n') => {
                self.composer = Some(Composer {
                    text: String::new(),
                    reply_to: None,
                });
            }
            KeyCode::Char('R') if self.screen != Screen::Queue => self.reply_to_selected(),
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('1') => self.switch_tab(Tab::Following),
            KeyCode::Char('2') => self.switch_tab(Tab::Lists),
            KeyCode::Char('3') => self.switch_tab(Tab::You),
            KeyCode::Char('4') => self.switch_tab(Tab::All),
            KeyCode::Char(']') => {
                let index = Tab::all()
                    .iter()
                    .position(|tab| *tab == self.tab)
                    .unwrap_or(0);
                self.switch_tab(Tab::all()[(index + 1) % 4]);
            }
            KeyCode::Char('[') => {
                let index = Tab::all()
                    .iter()
                    .position(|tab| *tab == self.tab)
                    .unwrap_or(0);
                self.switch_tab(Tab::all()[(index + 3) % 4]);
            }
            KeyCode::Char('Z') => {
                self.density = self.density.cycle();
                let _ = self.engine.set_density(self.tab, self.density.label());
                self.status = format!("density: {}", self.density.label());
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('N') => {
                self.selected = 0;
                let _ = self.engine.mark_seen(self.tab);
                self.refresh();
            }
            KeyCode::Char('v') => self.verify_selected(),
            KeyCode::Char('a') => self.approve_selected(false),
            KeyCode::Char('d') => self.decline_selected(),
            KeyCode::Char('b') => self.block_selected(),
            KeyCode::Char('m') => self.mute_selected(),
            KeyCode::Char('F') => self.remove_follower_selected(),
            KeyCode::Char('f') => self.follow_selected(),
            KeyCode::Char('R') if self.screen == Screen::Queue => self.retry_selected(),
            KeyCode::Char('c') if self.screen == Screen::Queue => self.cancel_selected(),
            KeyCode::Char('x') if self.screen == Screen::Problems => self.dismiss_selected(),
            KeyCode::Enter => self.open_selected(),
            _ => {}
        }
        Ok(false)
    }

    fn handle_g(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('t') => self.screen = Screen::Timeline,
            KeyCode::Char('c') => {
                self.screen = Screen::Contacts;
                self.selected = 0;
            }
            KeyCode::Char('f') => {
                self.screen = Screen::Requests;
                self.selected = 0;
            }
            KeyCode::Char('l') => self.screen = Screen::Lists,
            KeyCode::Char('q') => {
                self.screen = Screen::Queue;
                self.selected = 0;
            }
            KeyCode::Char('r') => {
                self.screen = Screen::Replies;
                self.selected = 0;
            }
            KeyCode::Char('p') => {
                self.screen = Screen::Problems;
                self.selected = 0;
            }
            KeyCode::Char('s') => self.screen = Screen::Settings,
            _ => {}
        }
        Ok(false)
    }

    fn handle_composer(&mut self, key: KeyEvent) -> Result<bool> {
        let send =
            key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('s'));
        if send {
            let composer = self.composer.take().expect("composer");
            if composer.text.trim().is_empty() {
                self.status = "nothing to send".to_string();
                self.composer = Some(composer);
                return Ok(false);
            }
            let result = match &composer.reply_to {
                Some(parent) => self
                    .engine
                    .reply(parent, &composer.text)
                    .map(|id| format!("replied ({})", short(&id))),
                None => self
                    .engine
                    .post(&composer.text, Audience::Followers, None)
                    .map(|id| format!("posted ({})", short(&id))),
            };
            match result {
                Ok(message) => {
                    self.status = message;
                    self.refresh();
                }
                Err(error) => {
                    self.status = format!("could not send: {error}");
                }
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc => {
                self.composer = None;
                self.status = "draft discarded".to_string();
            }
            KeyCode::Enter => {
                if let Some(composer) = self.composer.as_mut() {
                    composer.text.push('\n');
                }
            }
            KeyCode::Backspace => {
                if let Some(composer) = self.composer.as_mut() {
                    composer.text.pop();
                }
            }
            KeyCode::Char(ch) => {
                if let Some(composer) = self.composer.as_mut() {
                    if composer.text.len() < 20_000 {
                        composer.text.push(ch);
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.screen = Screen::Timeline;
        self.selected = 0;
        self.density = density_from(&self.engine, tab);
        self.refresh();
    }

    fn move_selection(&mut self, delta: i64) {
        let len = match self.screen {
            Screen::Timeline => self.feed.len(),
            Screen::Conversation => self.conversation.len(),
            Screen::Contacts => self.contacts.len(),
            Screen::Requests => self.requests.len(),
            Screen::Queue => self.queue.len(),
            Screen::Replies => self.replies.len(),
            Screen::Problems => self.problems.len(),
            Screen::Lists | Screen::Settings => 0,
        };
        if len == 0 {
            return;
        }
        let next = self.selected as i64 + delta;
        self.selected = next.clamp(0, len as i64 - 1) as usize;
    }

    fn open_selected(&mut self) {
        match self.screen {
            Screen::Timeline => {
                if let Some(item) = self.feed.get(self.selected) {
                    self.conversation_root = item.event_id.clone();
                    self.conversation =
                        self.engine.conversation(&item.event_id).unwrap_or_default();
                    self.screen = Screen::Conversation;
                    self.selected = 0;
                    let _ = self.engine.mark_seen(self.tab);
                }
            }
            Screen::Replies => {
                if let Some(item) = self.replies.get(self.selected) {
                    self.conversation_root = item.event_id.clone();
                    self.conversation =
                        self.engine.conversation(&item.event_id).unwrap_or_default();
                    self.screen = Screen::Conversation;
                    self.selected = 0;
                }
            }
            Screen::Settings => self.toggle_setting(),
            _ => {}
        }
    }

    fn reply_to_selected(&mut self) {
        if let Some(item) = self.feed.get(self.selected) {
            self.composer = Some(Composer {
                text: String::new(),
                reply_to: Some(item.event_id.clone()),
            });
        } else if let Some(item) = self.replies.get(self.selected) {
            self.composer = Some(Composer {
                text: String::new(),
                reply_to: Some(item.event_id.clone()),
            });
        }
    }

    fn verify_selected(&mut self) {
        let fingerprint = match self.screen {
            Screen::Contacts => self
                .contacts
                .get(self.selected)
                .map(|view| view.contact.fingerprint.clone()),
            Screen::Requests => self.requests.get(self.selected).map(|row| row.0.clone()),
            _ => None,
        };
        if let Some(fingerprint) = fingerprint {
            match self.engine.verify_contact(&fingerprint) {
                Ok(()) => self.status = "verified: they can receive private posts".into(),
                Err(error) => self.status = format!("{error}"),
            }
            self.refresh();
        }
    }

    fn approve_selected(&mut self, verify: bool) {
        if let Screen::Requests = self.screen {
            if let Some(row) = self.requests.get(self.selected) {
                let fingerprint = row.0.clone();
                match self.engine.approve_follower(&fingerprint, verify) {
                    Ok(_) if verify => {
                        self.status = "approved and verified".into();
                    }
                    Ok(_) => {
                        self.status = "approved unverified: no private posts until verified".into();
                    }
                    Err(error) => self.status = format!("{error}"),
                }
                self.refresh();
            }
        }
    }

    fn decline_selected(&mut self) {
        if let Screen::Requests = self.screen {
            if let Some(row) = self.requests.get(self.selected) {
                let fingerprint = row.0.clone();
                let _ = self.engine.decline_follower(&fingerprint);
                self.status = "declined (no message sent)".into();
                self.refresh();
            }
        }
    }

    fn block_selected(&mut self) {
        let fingerprint = match self.screen {
            Screen::Contacts => self
                .contacts
                .get(self.selected)
                .map(|view| view.contact.fingerprint.clone()),
            Screen::Requests => self.requests.get(self.selected).map(|row| row.0.clone()),
            _ => None,
        };
        if let Some(fingerprint) = fingerprint {
            match self.engine.block_contact(&fingerprint) {
                Ok(()) => self.status = "blocked: messages are discarded, nobody is told".into(),
                Err(error) => self.status = format!("{error}"),
            }
            self.refresh();
        }
    }

    fn mute_selected(&mut self) {
        if let Screen::Contacts = self.screen {
            if let Some(view) = self.contacts.get(self.selected) {
                let fingerprint = view.contact.fingerprint.clone();
                let muted = !view.contact.muted;
                let _ = self.engine.mute_contact(&fingerprint, muted);
                self.status = if muted {
                    "muted: hidden from the timeline".into()
                } else {
                    "unmuted".into()
                };
                self.refresh();
            }
        }
    }

    fn remove_follower_selected(&mut self) {
        if let Screen::Contacts = self.screen {
            if let Some(view) = self.contacts.get(self.selected) {
                let fingerprint = view.contact.fingerprint.clone();
                let _ = self.engine.remove_follower(&fingerprint);
                self.status = "removed: they keep what they already have".into();
                self.refresh();
            }
        }
    }

    fn follow_selected(&mut self) {
        if let Screen::Contacts = self.screen {
            if let Some(view) = self.contacts.get(self.selected) {
                let fingerprint = view.contact.fingerprint.clone();
                match self.engine.follow(&fingerprint) {
                    Ok(_) => self.status = "follow request sent".into(),
                    Err(error) => self.status = format!("{error}"),
                }
                self.refresh();
            }
        }
    }

    fn retry_selected(&mut self) {
        if let Screen::Queue = self.screen {
            if let Some(item) = self.queue.get(self.selected) {
                let post = item.post_event_id.clone();
                let recipients: Vec<String> = item
                    .deliveries
                    .iter()
                    .filter(|row| {
                        matches!(row.state, DeliveryState::Failed | DeliveryState::Retrying)
                    })
                    .map(|row| row.recipient.clone())
                    .collect();
                for recipient in recipients {
                    let _ = self.engine.retry_delivery(&post, &recipient);
                }
                self.status = "retry queued".into();
                self.refresh();
            }
        }
    }

    fn cancel_selected(&mut self) {
        if let Screen::Queue = self.screen {
            if let Some(item) = self.queue.get(self.selected) {
                let post = item.post_event_id.clone();
                let recipients: Vec<String> = item
                    .deliveries
                    .iter()
                    .filter(|row| !row.state.is_terminal())
                    .map(|row| row.recipient.clone())
                    .collect();
                for recipient in recipients {
                    let _ = self.engine.cancel_delivery(&post, &recipient);
                }
                self.status = "cancelled".into();
                self.refresh();
            }
        }
    }

    fn dismiss_selected(&mut self) {
        if let Some(problem) = self.problems.get(self.selected) {
            let event_id = problem.event_id.clone();
            let _ = self.engine.dismiss_problem(&event_id);
            self.status = "problem dismissed".into();
            self.refresh();
        }
    }

    fn toggle_setting(&mut self) {
        let enabled = !self.engine.config.move_protocol_messages;
        let _ = self.engine.set_mail_folder(enabled);
        self.status = format!(
            "folder moves {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    // ----------------------------------------------------------------- render

    fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(area);

        self.render_tabs(frame, chunks[0]);
        match self.screen {
            Screen::Timeline => self.render_timeline(frame, chunks[1]),
            Screen::Conversation => self.render_conversation(frame, chunks[1]),
            Screen::Contacts => self.render_contacts(frame, chunks[1]),
            Screen::Requests => self.render_requests(frame, chunks[1]),
            Screen::Queue => self.render_queue(frame, chunks[1]),
            Screen::Replies => self.render_replies(frame, chunks[1]),
            Screen::Lists => self.render_lists(frame, chunks[1]),
            Screen::Settings => self.render_settings(frame, chunks[1]),
            Screen::Problems => self.render_problems(frame, chunks[1]),
        }
        if self.composer.is_some() {
            let composer_area = Rect {
                x: chunks[1].x,
                y: chunks[1].y + chunks[1].height.saturating_sub(8),
                width: chunks[1].width,
                height: 8.min(chunks[1].height),
            };
            self.render_composer(frame, composer_area);
        }
        self.render_banner(frame, chunks[2]);
        self.render_status(frame, chunks[3]);
        if self.help {
            self.render_help(frame, area);
        }
    }

    fn color(&self, style: Color) -> Style {
        if self.engine.config.color == "never" {
            Style::default()
        } else {
            Style::default().fg(style)
        }
    }

    fn trust_style(&self, trust: Trust) -> Style {
        match trust {
            Trust::Verified => self.color(Color::Green),
            Trust::Unverified => self.color(Color::Yellow),
            Trust::Blocked => self.color(Color::Red),
        }
    }

    fn render_tabs(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut spans = Vec::new();
        for (index, tab) in Tab::all().iter().enumerate() {
            let unseen = self.engine.unseen(*tab).unwrap_or(0);
            let label = match tab {
                Tab::Following => "Following",
                Tab::Lists => "Lists",
                Tab::You => "You",
                Tab::All => "All",
            };
            let active = *tab == self.tab;
            let text_value = if unseen > 0 {
                format!(" {} {label} ({unseen}) ", index + 1)
            } else {
                format!(" {} {label} ", index + 1)
            };
            let mut style = if active {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            if unseen > 0 && !active {
                style = style.add_modifier(Modifier::BOLD);
                if self.engine.config.color != "never" {
                    style = style.fg(Color::Yellow);
                }
            }
            spans.push(Span::styled(text_value, style));
        }
        spans.push(Span::raw(format!("  {} ", self.density.label())));
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn render_timeline(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let width = area.width.saturating_sub(2) as usize;
        let mut lines: Vec<Line<'static>> = Vec::new();
        self.card_lines.clear();
        if self.feed.is_empty() {
            lines.push(Line::from(
                "Nothing here yet. When someone you follow posts, it arrives by email and appears here.",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(
                "n  write a post      g c  contacts      ?  help",
            ));
        }
        let mut current_day = String::new();
        for (index, item) in self.feed.iter().enumerate() {
            let day = item.created_at.chars().take(10).collect::<String>();
            if day != current_day {
                lines.push(Line::from(Span::styled(
                    format!("── {day} "),
                    Style::default().add_modifier(Modifier::DIM),
                )));
                current_day = day;
            }
            self.card_lines.push(lines.len());
            let selected = index == self.selected;
            self.push_card(&mut lines, item, width, selected);
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Timeline ")),
            area,
        );
    }

    fn push_card(
        &self,
        lines: &mut Vec<Line<'static>>,
        item: &FeedItem,
        width: usize,
        selected: bool,
    ) {
        let marker = if selected { "▶" } else { " " };
        let unread = if item.seen { "" } else { "•" };
        let audience = match item.audience {
            Audience::Followers => "private".to_string(),
            Audience::PublicList => format!("list:{}", item.list_id.clone().unwrap_or_default()),
        };
        let time = item
            .created_at
            .split('T')
            .nth(1)
            .map(|rest| rest.chars().take(5).collect::<String>())
            .unwrap_or_else(|| item.created_at.clone());

        let mut header = vec![
            Span::raw(format!("{marker}{unread} {time}  ")),
            Span::styled(
                item.author_name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                match item.trust {
                    Trust::Verified => "verified",
                    Trust::Unverified => "unverified",
                    Trust::Blocked => "blocked",
                },
                self.trust_style(item.trust),
            ),
            Span::raw("  "),
            Span::styled(
                audience,
                if self.engine.config.color == "never" {
                    Style::default()
                } else if matches!(item.audience, Audience::PublicList) {
                    Style::default().fg(Color::Magenta)
                } else {
                    Style::default().fg(Color::Cyan)
                },
            ),
        ];
        if let Some(summary) = &item.delivery_summary {
            header.push(Span::styled(
                format!("  {summary}"),
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        lines.push(Line::from(header));

        let body_budget = match self.density {
            Density::Compact => 1,
            Density::Cozy => self.engine.config.body_lines.clamp(1, 40) as usize,
            Density::Full => (self.engine.config.body_lines.clamp(1, 40) as usize) * 2,
        };
        let body = text::layout(&item.text, width.saturating_sub(4));
        for line in body.iter().take(body_budget) {
            lines.push(Line::from(format!("    {line}")));
        }
        if body.len() > body_budget {
            lines.push(Line::from(Span::styled(
                format!("    … +{} lines", body.len() - body_budget),
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        if item.reply_count > 0 {
            lines.push(Line::from(Span::styled(
                format!("    {} replies · Enter to open", item.reply_count),
                Style::default().add_modifier(Modifier::DIM),
            )));
        }
        lines.push(Line::from(""));
    }

    fn render_conversation(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let width = area.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.conversation.is_empty() {
            lines.push(Line::from("No messages in this conversation yet."));
        }
        for (index, item) in self.conversation.iter().enumerate() {
            let marker = if index == self.selected { "▶" } else { " " };
            lines.push(Line::from(vec![
                Span::raw(format!("{marker}{}  ", "  ".repeat(item.depth))),
                Span::styled(
                    item.author_name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    match item.trust {
                        Trust::Verified => "verified",
                        Trust::Unverified => "unverified",
                        Trust::Blocked => "blocked",
                    },
                    self.trust_style(item.trust),
                ),
            ]));
            for line in text::layout(&item.text, width.saturating_sub(item.depth * 2)) {
                lines.push(Line::from(format!("   {}{line}", "  ".repeat(item.depth))));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Conversation ")),
            area,
        );
    }

    fn render_contacts(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.contacts.is_empty() {
            lines.push(Line::from(
                "No contacts yet. Import someone's contact card, or ask for their fingerprint.",
            ));
        }
        for (index, view) in self.contacts.iter().enumerate() {
            let marker = if index == self.selected { "▶" } else { " " };
            let follows = view.i_follow.map(|state| state.as_str()).unwrap_or("-");
            let follows_me = view.follows_me.map(|state| state.as_str()).unwrap_or("-");
            lines.push(Line::from(vec![
                Span::raw(format!(
                    "{marker} {:<20} ",
                    truncate(&view.contact.display_name, 20)
                )),
                Span::styled(
                    format!("{:<10}", view.contact.trust.as_str()),
                    self.trust_style(view.contact.trust),
                ),
                Span::raw(format!(
                    " you:{follows:<9} them:{follows_me:<9} {}{}",
                    short(&view.contact.fingerprint),
                    if view.contact.muted { "  muted" } else { "" }
                )),
            ]));
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .title(" Contacts · v verify  b block  m mute  f follow  F remove follower "),
            ),
            area,
        );
    }

    fn render_requests(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.requests.is_empty() {
            lines.push(Line::from(
                "No follow requests. People who want your private posts appear here first.",
            ));
        }
        for (index, (fingerprint, name, trust)) in self.requests.iter().enumerate() {
            let marker = if index == self.selected { "▶" } else { " " };
            lines.push(Line::from(vec![
                Span::raw(format!(
                    "{marker} {:<20} ",
                    name.clone().unwrap_or_else(|| short(fingerprint))
                )),
                Span::styled(format!("{:<10}", trust.as_str()), self.trust_style(*trust)),
                Span::raw(format!(" {}", short(fingerprint))),
            ]));
            lines.push(Line::from(
                "    v verify & approve   a approve unverified   d decline   b block",
            ));
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Follow requests ")),
            area,
        );
    }

    fn render_queue(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.queue.is_empty() {
            lines.push(Line::from(
                "Nothing queued. Everything you've sent has been accepted for delivery.",
            ));
        }
        for (index, item) in self.queue.iter().enumerate() {
            let marker = if index == self.selected { "▶" } else { " " };
            lines.push(Line::from(vec![Span::raw(format!(
                "{marker} {}  {}",
                short(&item.post_event_id),
                truncate(&item.preview, 40)
            ))]));
            for row in &item.deliveries {
                let name = self
                    .engine
                    .contact(&row.recipient)
                    .ok()
                    .flatten()
                    .map(|contact| contact.display_name)
                    .unwrap_or_else(|| short(&row.recipient));
                let style = match row.state {
                    DeliveryState::Accepted => self.color(Color::Green),
                    DeliveryState::Failed => self.color(Color::Red),
                    DeliveryState::Retrying | DeliveryState::Queued => self.color(Color::Yellow),
                    DeliveryState::Withheld => self.color(Color::Yellow),
                    _ => Style::default(),
                };
                lines.push(Line::from(vec![
                    Span::raw(format!("    {:<16} ", truncate(&name, 16))),
                    Span::styled(row.state.label().to_string(), style),
                ]));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Queue · R retry  c cancel ")),
            area,
        );
    }

    fn render_replies(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.replies.is_empty() {
            lines.push(Line::from("No replies to you yet."));
        }
        for (index, item) in self.replies.iter().enumerate() {
            let marker = if index == self.selected { "▶" } else { " " };
            lines.push(Line::from(vec![
                Span::raw(format!(
                    "{marker} {}  ",
                    item.created_at.chars().take(16).collect::<String>()
                )),
                Span::styled(
                    item.author_name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    item.trust.as_str().to_string(),
                    self.trust_style(item.trust),
                ),
            ]));
            for line in text::layout(&item.text, area.width.saturating_sub(6) as usize) {
                lines.push(Line::from(format!("    {line}")));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(" Replies to you · Enter to open ")),
            area,
        );
    }

    fn render_lists(&self, frame: &mut Frame<'_>, area: Rect) {
        let lists = self.engine.lists().unwrap_or_default();
        let mut lines: Vec<Line<'static>> = Vec::new();
        if lists.is_empty() {
            lines.push(Line::from(
                "No lists. Add a public list with: social-mail lists add <address>",
            ));
        }
        for list in lists {
            lines.push(Line::from(format!("  {list}")));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "List posts are signed but not encrypted: the operator and every subscriber can read them.",
            Style::default().add_modifier(Modifier::DIM),
        )));
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Lists ")),
            area,
        );
    }

    fn render_settings(&self, frame: &mut Frame<'_>, area: Rect) {
        let config = &self.engine.config;
        let lines = vec![
            Line::from(format!("[x] Move SocialMail messages out of INBOX   {}", config.move_protocol_messages)),
            Line::from(""),
            Line::from(format!("Language        {}", config.locale)),
            Line::from(format!("Colour          {}", config.color)),
            Line::from(format!("Density         {}", self.density.label())),
            Line::from(format!("Body lines      {}", config.body_lines)),
            Line::from(format!("Auto-sync (min) {}", config.auto_sync_minutes)),
            Line::from(""),
            Line::from(format!("Transport       {}", config.transport)),
            Line::from(format!("Spool           {}", config.spool_dir.display())),
            Line::from(format!("Data            {}", config.data_dir.display())),
            Line::from(""),
            Line::from(Span::styled(
                "Enter toggles the folder setting. Mail transport is not implemented in this build.",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Settings ")),
            area,
        );
    }

    fn render_problems(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.problems.is_empty() {
            lines.push(Line::from(
                "No problems. Every message that arrived verified.",
            ));
        }
        for (index, problem) in self.problems.iter().enumerate() {
            let marker = if index == self.selected { "▶" } else { " " };
            lines.push(Line::from(vec![
                Span::raw(format!("{marker} ")),
                Span::styled(
                    problem.reason.clone(),
                    self.color(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  {}", problem.created_at)),
            ]));
            if let Some(detail) = &problem.detail {
                for line in text::layout(detail, area.width.saturating_sub(6) as usize) {
                    lines.push(Line::from(format!("    {line}")));
                }
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Problems · x dismiss ")),
            area,
        );
    }

    fn render_composer(&self, frame: &mut Frame<'_>, area: Rect) {
        let composer = match &self.composer {
            Some(composer) => composer,
            None => return,
        };
        let target = match &composer.reply_to {
            Some(parent) => format!("Replying to {}", short(parent)),
            None => "New post · Followers (encrypted)".to_string(),
        };
        let mut lines = vec![Line::from(Span::styled(
            target,
            Style::default().add_modifier(Modifier::BOLD),
        ))];
        for line in text::layout(&composer.text, area.width.saturating_sub(4) as usize) {
            lines.push(Line::from(format!(" {line}")));
        }
        lines.push(Line::from(Span::styled(
            " Ctrl+S send   Esc discard   plain text, Enter for a new line",
            Style::default().add_modifier(Modifier::DIM),
        )));
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" Compose ")),
            area,
        );
    }

    fn render_banner(&self, frame: &mut Frame<'_>, area: Rect) {
        let text_value = if self.engine.config.color == "never" {
            Span::raw(self.status.clone())
        } else {
            Span::styled(self.status.clone(), Style::default().fg(Color::Yellow))
        };
        frame.render_widget(Paragraph::new(Line::from(text_value)), area);
    }

    fn render_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let counts = self.engine.counts().unwrap_or_default();
        let sync_age = match self.last_sync {
            Some(instant) => format!("{}s ago", instant.elapsed().as_secs()),
            None => "never".to_string(),
        };
        let text_value = format!(
            " {} · synced {sync_age} · {} queued · {} requests · {} new replies · {} ",
            short(&self.engine.fingerprint()),
            counts.queued_deliveries,
            self.requests.len(),
            self.replies.len(),
            self.engine.config.locale,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text_value,
                Style::default().add_modifier(Modifier::REVERSED),
            ))),
            area,
        );
    }

    fn render_help(&self, frame: &mut Frame<'_>, area: Rect) {
        let text_value = vec![
            Line::from("social-mail keys"),
            Line::from(""),
            Line::from("  1-4        feed tabs (Following, Lists, You, All)"),
            Line::from("  [ ]        cycle tabs"),
            Line::from("  Z          cycle density (compact, cozy, full)"),
            Line::from("  j k ↑ ↓    move          Enter  open          N  jump to new"),
            Line::from("  n          new post      R      reply        r  sync now"),
            Line::from("  g t/c/f/q/r/p/s   timeline, contacts, requests, queue, replies, problems, settings"),
            Line::from("  v verify · a approve · d decline · b block · m mute · F remove follower · f follow"),
            Line::from("  q back       ? this help       Esc cancel"),
            Line::from(""),
            Line::from("Private is the default audience. Delivery means 'accepted for delivery', never 'read'."),
        ];
        let popup = Rect {
            x: area.x + 2,
            y: area.y + 2,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(4),
        };
        frame.render_widget(
            Paragraph::new(text_value).block(Block::bordered().title(" Help ")),
            popup,
        );
    }
}

fn density_from(engine: &Engine, tab: Tab) -> Density {
    match engine.config.density_for(tab.as_str()).as_str() {
        "compact" => Density::Compact,
        "full" => Density::Full,
        _ => Density::Cozy,
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

fn truncate(value: &str, width: usize) -> String {
    text::truncate(value, width)
}

pub fn run(engine: Engine) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(engine);
    let result = event_loop(&mut terminal, &mut app);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| app.render(frame))?;
        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                if app.handle(key)? {
                    return Ok(());
                }
            }
        }
        // Optional periodic sync; 0 means manual only.
        if app.engine.config.auto_sync_minutes > 0 {
            let due = app
                .last_sync
                .map(|instant| {
                    instant.elapsed()
                        >= Duration::from_secs(app.engine.config.auto_sync_minutes as u64 * 60)
                })
                .unwrap_or(true);
            if due {
                let _ = app.sync();
            }
        }
    }
}
