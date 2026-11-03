//! J9: a private list relayed by a reflector mailbox.
//!
//! Alice and Bob both ask the list to accept them. The list approves both, then
//! relays Alice's post. Bob must receive a message whose author is Alice — the
//! list never signs for the author — and an unverified member never receives
//! private traffic.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use social_mail_engine::{Audience, Config, Engine, FollowState, InitOptions, Tab, Trust};

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sm-relay-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Sandbox { root }
    }

    fn profile(&self, name: &str, address: &str) -> Engine {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("social-mail.toml");
        let config = Config {
            data_dir: dir.join("data"),
            spool_dir: self.root.join("spool"),
            profile_name: name.to_string(),
            address: address.to_string(),
            ..Config::default()
        };
        config.save(&config_path).unwrap();
        Engine::create(
            &config_path,
            InitOptions {
                display_name: name.to_string(),
                address: address.to_string(),
                locale: "en".to_string(),
            },
        )
        .unwrap()
    }

    fn export_card(&self, engine: &Engine, name: &str) -> PathBuf {
        let path = self.root.join(format!("{name}.card"));
        engine.export_card(&path).unwrap();
        path
    }
}

#[test]
fn relay_preserves_the_author_and_skips_unverified_members() {
    let sandbox = Sandbox::new();
    let mut alice = sandbox.profile("alice", "alice@example.org");
    let mut bob = sandbox.profile("bob", "bob@example.org");
    let mut list = sandbox.profile("list", "list@example.org");

    let list_card = sandbox.export_card(&list, "list");
    alice.import_card_file(&list_card).unwrap();
    bob.import_card_file(&list_card).unwrap();

    // Both members ask to join; a handshake reaches contacts we have not
    // verified yet, which is the only way onboarding can work.
    alice.follow(&list.fingerprint()).unwrap();
    bob.follow(&list.fingerprint()).unwrap();
    let report = list.sync().unwrap();
    assert_eq!(report.received, 2);
    assert_eq!(report.accepted, 2);

    // The operator is deliberately stricter than personal trust: approval
    // verifies the member, and approval is what lets them receive traffic.
    list.approve_follower(&alice.fingerprint(), true).unwrap();
    list.approve_follower(&bob.fingerprint(), true).unwrap();
    alice.sync().unwrap();
    bob.sync().unwrap();

    let members = list.members().unwrap();
    assert_eq!(members.len(), 2);
    assert!(members
        .iter()
        .all(|row| row.state == FollowState::Following));

    let post_id = alice
        .post(
            "سلام، این یک آزمایش است.\nHello world",
            Audience::PublicList,
            Some("list@example.org".to_string()),
        )
        .unwrap();

    let received = list.sync().unwrap();
    assert!(
        received.accepted >= 1,
        "list did not receive the post: {received:?}"
    );
    let candidates = list.relay_candidates("list@example.org", 10).unwrap();
    assert_eq!(candidates.len(), 1, "exactly one post waits for relay");
    assert_eq!(candidates[0].event_id, post_id);

    let relay = list.redistribute(&post_id).unwrap();
    assert_eq!(relay.failed, 0);
    assert_eq!(relay.sent, 1, "only Bob gets a copy; the author is skipped");

    let delivered = bob.sync().unwrap();
    assert!(delivered.accepted >= 1, "bob got nothing: {delivered:?}");
    assert_eq!(delivered.quarantined, 0);

    let feed = bob.timeline(Tab::All, 20).unwrap();
    let item = feed
        .iter()
        .find(|item| item.event_id == post_id)
        .expect("relayed post is in Bob's timeline");
    assert_eq!(item.author, alice.fingerprint());
    assert_ne!(
        item.author,
        list.fingerprint(),
        "the list never signs for the author"
    );
    assert_eq!(
        item.trust,
        Trust::Unverified,
        "Bob has not verified Alice; the post still reads"
    );
    assert!(item.text.contains("آزمایش"));

    // Once relayed, the same post is not offered again.
    assert!(list
        .relay_candidates("list@example.org", 10)
        .unwrap()
        .is_empty());

    // A removed member receives nothing further.
    list.remove_follower(&bob.fingerprint()).unwrap();
    let second = alice
        .post(
            "second post",
            Audience::PublicList,
            Some("list@example.org".to_string()),
        )
        .unwrap();
    list.sync().unwrap();
    let relay = list.redistribute(&second).unwrap();
    assert_eq!(relay.sent, 0);
    assert!(relay.skipped >= 1);
    assert!(bob.sync().unwrap().accepted == 0);
    assert!(!bob
        .timeline(Tab::All, 50)
        .unwrap()
        .iter()
        .any(|item| item.event_id == second));
}

#[test]
fn unverified_members_are_never_relayed_to() {
    let sandbox = Sandbox::new();
    let mut alice = sandbox.profile("alice2", "alice2@example.org");
    let mut carol = sandbox.profile("carol", "carol@example.org");
    let mut list = sandbox.profile("list2", "list2@example.org");

    let list_card = sandbox.export_card(&list, "list2");
    alice.import_card_file(&list_card).unwrap();
    carol.import_card_file(&list_card).unwrap();

    alice.follow(&list.fingerprint()).unwrap();
    carol.follow(&list.fingerprint()).unwrap();
    list.sync().unwrap();
    list.approve_follower(&alice.fingerprint(), true).unwrap();
    // Carol is approved on paper but never verified.
    list.approve_follower(&carol.fingerprint(), false).unwrap();

    let post_id = alice
        .post(
            "hi",
            Audience::PublicList,
            Some("list2@example.org".to_string()),
        )
        .unwrap();
    list.sync().unwrap();
    let relay = list.redistribute(&post_id).unwrap();
    assert_eq!(relay.sent, 0);
    assert_eq!(relay.skipped, 1);
    assert_eq!(carol.sync().unwrap().accepted, 0);
}
