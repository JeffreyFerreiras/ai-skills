#[path = "../src/suite/validator_originals.rs"]
mod originals;

#[test]
fn original_decision_precedence_is_order_independent() {
    originals::decision_precedence_is_order_independent();
}
#[test]
fn original_delivery_dominance() {
    originals::delivery_dominance();
}
#[test]
fn original_out_of_scope_findings_do_not_raise_delivery_outcome() {
    originals::out_of_scope_findings_do_not_raise_delivery_outcome();
}
#[test]
fn original_stored_cross_phase_dispositions_remain_valid_on_reverification() {
    originals::stored_cross_phase_dispositions_remain_valid_on_reverification();
}
#[test]
fn original_only_runs_from_earlier_engines_keep_shared_dispositions_on_reverification() {
    originals::only_runs_from_earlier_engines_keep_shared_dispositions_on_reverification();
}
#[test]
fn original_delivery_rejects_in_scope_approve_disposition() {
    originals::delivery_rejects_in_scope_approve_disposition();
}
#[test]
fn original_unknown_finding_scope_is_rejected() {
    originals::unknown_finding_scope_is_rejected();
}
#[test]
fn original_duplicate_and_conflicting_finding_ids_are_rejected() {
    originals::duplicate_and_conflicting_finding_ids_are_rejected();
}
