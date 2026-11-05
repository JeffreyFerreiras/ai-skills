#[allow(dead_code)]
#[path = "support/graph_case.rs"]
mod graph_case;

use graph_case::GraphFixture;
use graph_engine::{
    application::policy::profile_policy_path, domain::ids,
    infrastructure::policy_files::NativePolicyFiles,
};

#[test]
fn graphcase_setup_copies_policy_links_profile_and_cleanup_removes_owned_root() {
    let fixture = GraphFixture::new();
    let root = fixture.root.clone();
    assert!(fixture.repository.join(".codex").is_dir());
    assert!(fixture.repository.join("docs/artifacts").is_dir());
    assert!(fixture.skill_root.is_dir());
    assert_eq!(fixture.counter, 0);
    assert_eq!(fixture.store.state_root, fixture.runtime_root);
    let evidence = std::fs::read(fixture.repository.join("docs/engineering-graph.md")).unwrap();
    assert_eq!(
        evidence,
        if cfg!(windows) {
            b"# Test graph evidence\r\n".as_slice()
        } else {
            b"# Test graph evidence\n".as_slice()
        }
    );
    let task = fixture.task("delivery", "full_delivery", &[]);
    assert!(task["policy_approval"]["sha256"].is_text(&ids::sha256_bytes(&fixture.policy_bytes)));
    let profile = profile_policy_path(
        &NativePolicyFiles::default(),
        &fixture.repository,
        &fixture.runtime_root,
    )
    .unwrap();
    std::fs::write(&profile, b"same hard-linked file").unwrap();
    assert_eq!(
        std::fs::read(fixture.repository.join(".codex/engineering-graph.json")).unwrap(),
        b"same hard-linked file"
    );
    assert_ne!(fixture.policy_bytes, b"same hard-linked file");
    fixture.tear_down();
    assert!(!root.exists());
}
