#[path = "../src/suite/merge_readiness.rs"]
mod cases;

#[test]
fn test_matching_snapshot_is_dry_run_only_and_does_not_mutate_evidence() {
    cases::matching_snapshot();
}
#[test]
fn test_head_change_invalidates_review_and_ci_even_with_identical_tree() {
    cases::changed_heads();
}
#[test]
fn test_required_ci_must_exist_and_pass() {
    cases::required_checks();
}
#[test]
fn test_review_blocks_and_incomplete_graph_prevent_readiness() {
    cases::review_and_graph_blockers();
}
