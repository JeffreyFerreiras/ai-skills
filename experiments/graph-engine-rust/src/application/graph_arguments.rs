use crate::compat::argparse::{ArgumentSpec, ParserSpec};
use crate::domain::{hosts, usage};
use std::sync::OnceLock;

pub fn shared_parser() -> &'static ParserSpec {
    static PARSER: OnceLock<ParserSpec> = OnceLock::new();
    PARSER.get_or_init(|| parser(hosts::HOSTS, hosts::DEFAULT_HOST, &usage::PHASES))
}

fn fields(parser: &mut ParserSpec, names: &[&str]) {
    for name in names {
        parser.argument(ArgumentSpec::string(name).required());
    }
}

fn leaf(name: &str, required: &[&str]) -> ParserSpec {
    let mut parser = ParserSpec::new(name);
    fields(&mut parser, required);
    parser
}

fn subgroup(name: &str, destination: &str) -> ParserSpec {
    let mut parser = ParserSpec::new(name);
    parser.subcommand_destination(destination);
    parser
}

pub fn parser(hosts: &[&str], default_host: &str, usage_phases: &[&str]) -> ParserSpec {
    let mut root = ParserSpec::new("graphctl");
    root.argument(ArgumentSpec::string("repo"));
    root.argument(ArgumentSpec::string("state-root").help("absolute runtime home for policy, ledger, lessons, and artifacts; defaults to SOFTWARE_ENGINEERING_GRAPH_HOME, then the legacy state/XDG overrides, then a sibling <profile>.local/software-engineering-graph directory"));
    root.argument(ArgumentSpec::flag("ack-degraded-permissions"));
    root.argument(ArgumentSpec::flag("ack-degraded-durability"));
    root.subcommand_destination("command");
    root.subcommand(
        "policy",
        Some("Load or create the runtime-home policy and show its digest"),
        ParserSpec::new("policy"),
    );

    let mut usage = subgroup("usage", "usage_kind");
    usage.subcommand("checkpoint", None, leaf("checkpoint", &["session-log"]));
    root.subcommand("usage", None, usage);
    let mut constraints = subgroup("constraints", "constraint_kind");
    constraints.subcommand("select", None, leaf("select", &["bundle", "context"]));
    root.subcommand("constraints", None, constraints);
    let mut plan = leaf("plan", &["run-id", "task-brief"]);
    plan.argument(ArgumentSpec::string("size").choices(&["small", "medium", "large"]));
    plan.argument(
        ArgumentSpec::string("host")
            .choices(hosts)
            .default_string(default_host),
    );
    root.subcommand(
        "plan",
        Some("Preview adjustable model recommendations without initializing state"),
        plan,
    );
    let mut initialize = leaf("init", &["run-id", "task-brief", "op-id"]);
    initialize.argument(ArgumentSpec::string("size").choices(&["small", "medium", "large"]));
    initialize.argument(ArgumentSpec::string("host").choices(hosts).default_string(default_host).help("model catalog: codex (default Sol/Muse recommendations), codex-astra, cursor, or claude"));
    initialize.argument(ArgumentSpec::flag("ack-degraded-permissions").suppressed_default());
    initialize.argument(ArgumentSpec::flag("ack-degraded-durability").suppressed_default());
    root.subcommand("init", None, initialize);

    let mut records = subgroup("record", "record_kind");
    let mut record_usage = leaf("usage", &["run-id"]);
    record_usage.argument(
        ArgumentSpec::string("action")
            .choices(&["bind", "collect", "close"])
            .required(),
    );
    fields(&mut record_usage, &["session-log", "op-id"]);
    for name in ["binding-id", "branch-id", "attempt-id"] {
        record_usage.argument(ArgumentSpec::string(name));
    }
    record_usage.argument(ArgumentSpec::string("phase").choices(usage_phases));
    for name in ["generation", "start-offset"] {
        record_usage.argument(ArgumentSpec::integer(name));
    }
    for name in ["source-id", "prefix-sha256"] {
        record_usage.argument(ArgumentSpec::string(name));
    }
    records.subcommand("usage", None, record_usage);
    records.subcommand(
        "branch-result",
        None,
        leaf(
            "branch-result",
            &[
                "run-id",
                "branch-id",
                "attempt-id",
                "claim-token",
                "result-manifest",
                "op-id",
            ],
        ),
    );
    for name in ["timeout", "skip"] {
        let mut parser = leaf(
            name,
            &[
                "run-id",
                "branch-id",
                "reason-code",
                "evidence-manifest",
                "op-id",
            ],
        );
        if name == "timeout" {
            fields(&mut parser, &["attempt-id", "claim-token"]);
        }
        records.subcommand(name, None, parser);
    }
    let mut retry = leaf("retry", &["run-id", "branch-id", "reason-code", "op-id"]);
    retry.argument(ArgumentSpec::string("repair-manifest"));
    records.subcommand("retry", None, retry);
    records.subcommand(
        "heartbeat",
        None,
        leaf(
            "heartbeat",
            &["run-id", "branch-id", "attempt-id", "claim-token", "op-id"],
        ),
    );
    let mut approval = leaf("approval", &["run-id", "approval-id", "scope-ref"]);
    approval.argument(
        ArgumentSpec::string("decision")
            .choices(&["APPROVE", "REJECT"])
            .required(),
    );
    fields(&mut approval, &["authority-ref", "artifact-sha256"]);
    approval.argument(ArgumentSpec::string("actor"));
    fields(&mut approval, &["op-id"]);
    records.subcommand("approval", None, approval);
    let mut approval = leaf("plan-approval", &["run-id", "plan-digest"]);
    approval.argument(
        ArgumentSpec::string("decision")
            .choices(&["APPROVE", "REJECT"])
            .required(),
    );
    fields(&mut approval, &["authority-ref"]);
    approval.argument(ArgumentSpec::string("actor"));
    fields(&mut approval, &["op-id"]);
    records.subcommand("plan-approval", None, approval);
    records.subcommand(
        "fanout-assessment",
        None,
        leaf(
            "fanout-assessment",
            &[
                "run-id",
                "fanout-id",
                "assessment-manifest",
                "authority-ref",
                "op-id",
            ],
        ),
    );
    records.subcommand(
        "review-fanout",
        None,
        leaf(
            "review-fanout",
            &[
                "run-id",
                "branch-id",
                "attempt-id",
                "claim-token",
                "preliminary-manifest",
                "request-manifest",
                "authority-ref",
                "op-id",
            ],
        ),
    );
    records.subcommand(
        "review-fanout-assessment",
        None,
        leaf(
            "review-fanout-assessment",
            &[
                "run-id",
                "request-slot-id",
                "assessment-manifest",
                "authority-ref",
                "op-id",
            ],
        ),
    );
    let mut budget = leaf("budget-use", &["run-id", "budget-id"]);
    budget.argument(ArgumentSpec::integer("amount").required());
    fields(&mut budget, &["source-branch-id", "op-id"]);
    records.subcommand("budget-use", None, budget);
    let mut acceptance = leaf(
        "acceptance-evidence",
        &[
            "run-id",
            "criterion-id",
            "artifact-ref",
            "artifact-sha256",
            "op-id",
        ],
    );
    acceptance.argument(ArgumentSpec::string("replace-ref"));
    records.subcommand("acceptance-evidence", None, acceptance);
    let mut check = leaf("check-evidence", &["run-id", "check-id"]);
    check.argument(
        ArgumentSpec::string("outcome")
            .choices(&["PASS", "FAIL", "NOT_RUN"])
            .required(),
    );
    fields(&mut check, &["artifact-ref", "artifact-sha256", "op-id"]);
    records.subcommand("check-evidence", None, check);
    root.subcommand("record", None, records);

    let mut checks = subgroup("check", "check_kind");
    let mut run = leaf("run", &["run-id", "check-id", "op-id"]);
    for name in [
        "executor-branch-id",
        "executor-attempt-id",
        "executor-claim-token",
        "source-branch-id",
        "source-attempt-id",
        "source-claim-digest",
        "replace-ref",
    ] {
        run.argument(ArgumentSpec::string(name));
    }
    checks.subcommand("run", None, run);
    checks.subcommand(
        "abandon",
        None,
        leaf("abandon", &["run-id", "reservation-id", "reason", "op-id"]),
    );
    root.subcommand("check", None, checks);
    let mut evidence = subgroup("evidence", "evidence_kind");
    let mut enable = leaf("enable", &["run-id"]);
    enable.argument(ArgumentSpec::integer("contract-version").required());
    fields(&mut enable, &["coverage-manifest", "op-id"]);
    evidence.subcommand("enable", None, enable);
    root.subcommand("evidence", None, evidence);
    let mut recovery = subgroup("recovery", "recovery_kind");
    let mut show = leaf("show", &["run-id"]);
    show.argument(ArgumentSpec::string("branch-id"));
    recovery.subcommand("show", None, show);
    root.subcommand("recovery", None, recovery);
    root.subcommand(
        "recover",
        None,
        leaf(
            "recover",
            &[
                "run-id",
                "branch-id",
                "reason-code",
                "authority-ref",
                "recovery-manifest",
                "op-id",
            ],
        ),
    );
    let mut consolidation = subgroup("consolidation", "consolidation_kind");
    consolidation.subcommand("draft", None, leaf("draft", &["run-id", "join-id"]));
    root.subcommand("consolidation", None, consolidation);
    for name in ["next", "ready"] {
        let mut next = leaf(name, &["run-id"]);
        next.argument(ArgumentSpec::flag("all"));
        next.argument(ArgumentSpec::flag("claim"));
        next.argument(ArgumentSpec::string("op-id"));
        root.subcommand(name, None, next);
    }
    let mut joins = subgroup("join", "join_kind");
    joins.subcommand("validate", None, leaf("validate", &["run-id", "join-id"]));
    let mut advance = leaf("advance", &["run-id", "join-id", "op-id"]);
    advance.argument(ArgumentSpec::string("generated-output-plan"));
    joins.subcommand("advance", None, advance);
    root.subcommand("join", None, joins);
    let mut status = leaf("status", &["run-id"]);
    status.argument(ArgumentSpec::flag("json"));
    root.subcommand("status", None, status);
    root.subcommand("complete", None, leaf("complete", &["run-id", "op-id"]));
    root.subcommand(
        "block",
        None,
        leaf(
            "block",
            &["run-id", "reason-code", "evidence-manifest", "op-id"],
        ),
    );
    root.subcommand(
        "abort",
        None,
        leaf(
            "abort",
            &["run-id", "reason-code", "authority-ref", "op-id"],
        ),
    );
    let mut resume = leaf("resume", &["run-id"]);
    resume.argument(ArgumentSpec::flag("ack-degraded-permissions").suppressed_default());
    resume.argument(ArgumentSpec::flag("ack-degraded-durability").suppressed_default());
    root.subcommand("resume", None, resume);
    root
}
