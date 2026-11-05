#[path = "../src/suite/helper_fixture.rs"]
mod helper_fixture;
#[path = "../src/suite/helper_originals.rs"]
mod helper_originals;
#[path = "../src/suite/task_fixture.rs"]
mod task_fixture;

macro_rules! original {
    ($($name:ident),* $(,)?) => { $(#[test] fn $name() { helper_originals::$name(); })* };
}
original!(
    test_retired_allowances_are_auditable_but_new_allowances_reject_them,
    test_historical_register_reads_and_settles_but_rejects_new_retired_dispatch,
    test_plan_binds_allowance_and_register_context_is_immutable,
    test_runtime_allowance_and_checkpoint_are_bound_to_runtime_home,
    test_runtime_allowance_ref_rejects_traversal,
    test_reopened_register_rejects_mutated_immutable_payloads_and_identities,
    test_initialization_replay_compares_all_immutable_inputs,
    test_same_run_cannot_reset_budgets_with_a_replacement_plan,
    test_reservation_replay_precedes_checkpoint_and_authority_revalidation,
    test_state_root_and_register_location_are_bound_to_context,
    test_mutations_reject_external_register_paths_before_touching_locks,
    test_missing_observed_host_restriction_blocks_without_reservation,
    test_test_mode_reserves_and_labels_unenforced_isolation_without_faking_observation,
    test_test_mode_still_blocks_unverified_model_and_fresh_assignment_support,
    test_test_mode_does_not_relax_scope_commands_or_shared_budgets,
    test_test_mode_requires_explicit_versioned_acknowledgment,
    test_test_mode_requires_verified_fresh_model_selection,
    test_test_mode_rejects_wrong_repository_before_register_initialization,
    test_test_mode_cannot_change_after_approval,
    test_test_mode_rechecks_disposable_repository_binding,
    test_approved_host_supported_assignment_override_is_preserved_exactly,
    test_helper_allowances_accept_selected_models_from_each_harness,
    test_terra_helper_preflight_retains_capability_and_assignment_checks,
    test_revision_nine_helper_allowance_cannot_acquire_terra_authority,
    test_muse_graph_option_does_not_create_helper_runtime_authority,
    test_observed_assignment_mismatch_blocks_without_substitution,
    test_assignment_scope_command_and_mandatory_resource_keys_fail_closed,
    test_allowance_requires_declared_resources_and_assignment_keys,
    test_allowance_may_not_exceed_approved_parent_capabilities,
    test_settlement_releases_only_active_resources_and_never_refunds_budgets,
    test_interrupted_atomic_replace_preserves_prior_register,
    test_ledger_operates_through_public_persistence_port,
    test_malformed_nested_register_returns_structured_invalid_error,
    test_cross_process_lock_allows_only_one_atomic_active_reservation,
);

#[test]
fn graphcase_fixture_preserves_policy_hardlink_text_bytes_and_teardown() {
    use graph_engine::application::persistent_store::StatePlatform;
    let root;
    {
        let fixture = helper_fixture::Fixture::new(helper_fixture::Settings::default(), None);
        root = fixture.root.clone();
        assert!(fixture.repository.join(".codex").is_dir());
        assert!(fixture.repository.join("docs/artifacts").is_dir());
        assert!(root.join("installed-skill").is_dir());
        let repository_policy = fixture.repository.join(".codex/engineering-graph.json");
        let profile_policy = graph_engine::application::policy::profile_policy_path(
            &*helper_fixture::FILES,
            &fixture.repository,
            &root.join("codex"),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&repository_policy).unwrap(),
            std::fs::read(
                fixture
                    .installation
                    .join("tests/fixtures/engineering-graph.json")
            )
            .unwrap()
        );
        let original = helper_fixture::PLATFORM
            .info(&repository_policy, true)
            .unwrap();
        let link = helper_fixture::PLATFORM
            .info(&profile_policy, true)
            .unwrap();
        assert_eq!((original.device, original.inode), (link.device, link.inode));
        assert_eq!(
            std::fs::read(fixture.repository.join("docs/engineering-graph.md")).unwrap(),
            if cfg!(windows) {
                b"# Test graph evidence\r\n".as_slice()
            } else {
                b"# Test graph evidence\n".as_slice()
            }
        );
    }
    assert!(!root.exists());
}
