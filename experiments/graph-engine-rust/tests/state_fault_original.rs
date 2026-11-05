#[allow(dead_code)]
#[path = "../src/suite/state_originals.rs"]
mod state_originals;

#[test]
fn original_fault_before_commit_leaves_claim_ready() {
    state_originals::fault_before_commit_leaves_claim_ready();
}
