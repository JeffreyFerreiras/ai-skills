"""Commands and fixture requirements for the separate startup and build measurements."""

from public_cli import binary


def cost_plan(python, oracle, suite):
    native = suite.parent
    python_graphctl = [python, "-I", "-S", "-B", str(oracle / "scripts/graphctl.py")]
    native_graphctl = [str(binary(native, "graphctl"))]
    repository = ["--repo", "{fixture_repo}", "--state-root", "{state_root}"]
    run = ["--run-id", "{run_id}"]
    operation = ["--op-id", "{op_id}"]
    branch = ["--branch-id", "{branch_id}"]
    graph_steps = {
        "policy": ["policy"],
        "plan": ["plan", *run, "--task-brief", "{task_brief}"],
        "status": ["status", *run],
        "next": ["next", *run, "--all"],
        "check": [
            "check", "run", *run, "--check-id", "{check_id}", *operation,
            "--executor-branch-id", "{executor_branch_id}",
            "--executor-attempt-id", "{executor_attempt_id}",
            "--executor-claim-token", "{executor_claim_token}",
            "--source-branch-id", "{source_branch_id}",
            "--source-attempt-id", "{source_attempt_id}",
            "--source-claim-digest", "{source_claim_digest}",
        ],
        "retry": ["record", "retry", *run, *branch, "--reason-code", "{reason_code}",
                  "--repair-manifest", "{repair_manifest}", *operation],
        "recovery": ["recovery", "show", *run, *branch],
    }
    workflow = {
        name: {"python": [*python_graphctl, *repository, *tail],
               "rust": [*native_graphctl, *repository, *tail]}
        for name, tail in graph_steps.items()
    }
    workflow["status-edit-status"] = {
        "before": workflow["status"],
        "edit": "Use the same bound tracked-source replacement in each owned repository.",
        "after": workflow["status"],
        "expect": "A fresh source digest and newly ineligible prior evidence.",
    }
    manifest = "{disposable_worktree}/experiments/graph-engine-rust/Cargo.toml"
    build = ["cargo", "build", "--locked", "--manifest-path", manifest]
    release = [*build, "--release", "--bins"]
    develop = [*build, "--bin", "graphctl"]
    return {
        "scope": "Separate measurement interface. These commands are not launched by the workload driver.",
        "startup": {
            "minimum_warm_samples_per_case": 20,
            "help": {
                "graphctl": {"python": [*python_graphctl, "--help"], "rust": [*native_graphctl, "--help"]},
                "helper-register": {
                    "python": [python, "-I", "-S", "-B", str(oracle / "scripts/helper-register.py"), "--help"],
                    "rust": [str(binary(native, "helper-register")), "--help"],
                },
            },
            "workflow_argv_templates": workflow,
            "fixture_contract": (
                "Resolve placeholders only against paired fresh, owned fixtures. Bind initial bytes, "
                "expected exit and stable response, and restore an equivalent starting state before "
                "each mutating sample. Keep process startup separate from warmed library or NDJSON batches."
            ),
            "overhead_controls": {
                "python_empty_process": {"argv": [python, "-B", "-c", "pass"]},
                "python_empty_unittest": {"argv": [python, "-B", "-m", "unittest", "-v"], "cwd": "{owned_empty_directory}"},
                "rust_empty_adapter": {"argv": [str(binary(native, "graph-parity"))], "stdin": "closed without input"},
                "rust_registry_startup": {"argv": [str(suite), "--list"], "includes": "registry construction and 359 output rows"},
                "rust_empty_worker": "Requires a separately verified zero-work worker entry point. Do not use --list as zero work.",
                "fixture_and_evidence": "The workload driver measures fixture preparation and post-run evidence binding directly.",
            },
        },
        "build": {
            "minimum_samples_per_condition": 3,
            "clean_release": {"argv": release, "target": "fresh short isolated target for each sample"},
            "no_op_release": {"argv": release, "target": "the same target, sources and environment immediately after a clean build"},
            "warm_development_incremental": {
                "seed_argv": develop,
                "measured_argv": develop,
                "defined_edit": {
                    "path": "experiments/graph-engine-rust/src/application/graph_response.rs",
                    "before": '("schema_version", 1_i64.into()),',
                    "after": '("schema_version", Value::from(1_i64)),',
                    "apply_to": "a disposable copied-source fixture only",
                },
            },
            "build_and_focused_test": {
                "argv": ["cargo", "test", "--locked", "--manifest-path", manifest, "--test", "graph_constraints"],
                "target": "a separately seeded development target, with the same defined source edit",
            },
            "required_bindings": [
                "exact argv, cwd and toolchain hashes and versions",
                "source and lockfile hashes, before/after edit hashes and fixture tree",
                "short dedicated target, existing registry/dependency cache and actual network condition",
                "raw streams, exit, elapsed time, output binary hashes and failed samples",
            ],
        },
        "profiling": {
            "runs": "Separate instrumented work, outside the uninstrumented workload timing window.",
            "categories": ["Git launch and wait", "process creation and pipes", "filesystem", "SQLite", "fixture", "adapter"],
            "required": "Bind actual counters and profiler overhead. Leave unavailable measurements explicitly unmeasured.",
        },
    }
