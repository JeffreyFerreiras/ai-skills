#[allow(dead_code)]
#[path = "../src/suite/state_originals.rs"]
mod originals;

#[test]
fn exact_original_concurrent_claims_fixture_and_assertion() {
    originals::concurrent_claims_have_one_winner_and_one_conflict();
}
