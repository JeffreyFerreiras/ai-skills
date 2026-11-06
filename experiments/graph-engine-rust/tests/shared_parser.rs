use graph_engine::application::graph_arguments;
use graph_engine::compat::argparse::{ArgumentValue, Namespace, ParserOutcome};
use graph_engine::compat::PyString;
use num_bigint::BigInt;

fn parse(arguments: &[&str]) -> ParserOutcome {
    graph_arguments::parser(
        &["codex-astra", "codex", "cursor", "claude"],
        "codex-astra",
        &["main", "review"],
    )
    .parse(
        &arguments
            .iter()
            .map(|argument| PyString::from(*argument))
            .collect::<Vec<_>>(),
        80,
    )
}

fn namespace(arguments: &[&str]) -> Namespace {
    let ParserOutcome::Arguments(namespace) = parse(arguments) else {
        panic!("arguments rejected")
    };
    namespace
}

fn error(arguments: &[&str]) -> String {
    let ParserOutcome::Exit {
        code,
        stdout,
        stderr,
    } = parse(arguments)
    else {
        panic!("error expected")
    };
    assert_eq!(code, 2);
    assert!(stdout.0.is_empty());
    stderr.utf8().unwrap()
}

#[test]
fn suppressed_child_defaults_preserve_root_acknowledgements() {
    for command in ["init", "resume"] {
        let mut arguments = vec!["--ack-degraded-permissions", command, "--run-id", "R"];
        if command == "init" {
            arguments.extend(["--task-brief", "task", "--op-id", "O"]);
        }
        let namespace = namespace(&arguments);
        assert_eq!(
            namespace.get("ack_degraded_permissions"),
            Some(&ArgumentValue::Boolean(true))
        );
        assert_eq!(
            namespace.get("ack_degraded_durability"),
            Some(&ArgumentValue::Boolean(false))
        );
    }
    let namespace = namespace(&["resume", "--run-id", "R", "--ack-degraded-durability"]);
    assert_eq!(
        namespace.get("ack_degraded_durability"),
        Some(&ArgumentValue::Boolean(true))
    );
}

#[test]
fn nested_errors_keep_their_actual_parser_boundary() {
    let missing = error(&["record", "usage", "--unknown"]);
    assert!(missing.starts_with("usage: graphctl record usage "));
    assert!(missing.ends_with("graphctl record usage: error: the following arguments are required: --run-id, --action, --session-log, --op-id\n"));
    let extra = error(&[
        "--unknown-root",
        "status",
        "--run-id",
        "R",
        "--unknown-child",
    ]);
    assert!(extra.starts_with("usage: graphctl "));
    assert!(extra
        .ends_with("graphctl: error: unrecognized arguments: --unknown-root --unknown-child\n"));
    let child = error(&[
        "record", "usage", "--run-id", "R", "--action", "bad", "--help",
    ]);
    assert!(child.ends_with("graphctl record usage: error: argument --action: invalid choice: 'bad' (choose from bind, collect, close)\n"));
}

#[test]
fn option_errors_are_deferred_until_the_matching_parser_consumes_them() {
    let ambiguous = error(&["record", "usage", "--s"]);
    assert!(ambiguous.contains("graphctl record usage: error: ambiguous option: --s could match --session-log, --start-offset, --source-id\n"));
    assert!(matches!(
        parse(&["record", "usage", "--help", "--s"]),
        ParserOutcome::Exit { code: 0, .. }
    ));
    assert!(matches!(
        parse(&["--help", "--state"]),
        ParserOutcome::Exit { code: 0, .. }
    ));
}

#[test]
fn integer_options_keep_python_integer_ranges_and_unicode_grammar() {
    let huge = "999999999999999999999999999999999999999999999999999999999999999999999";
    let budget = namespace(&[
        "record",
        "budget-use",
        "--run-id",
        "R",
        "--budget-id",
        "B",
        "--amount",
        huge,
        "--source-branch-id",
        "S",
        "--op-id",
        "O",
    ]);
    assert_eq!(
        budget.get("amount"),
        Some(&ArgumentValue::Integer(huge.parse::<BigInt>().unwrap()))
    );
    let namespace = namespace(&[
        "record",
        "usage",
        "--run-id",
        "R",
        "--action",
        "bind",
        "--session-log",
        "L",
        "--op-id",
        "O",
        "--generation",
        "١_٢",
    ]);
    assert_eq!(
        namespace.get("generation"),
        Some(&ArgumentValue::Integer(BigInt::from(12)))
    );
    let bad = error(&["evidence", "enable", "--contract-version", "1.0"]);
    assert!(bad.ends_with(
        "graphctl evidence enable: error: argument --contract-version: invalid int value: '1.0'\n"
    ));
}

#[test]
fn argv_surrogates_survive_value_capture_and_error_repr() {
    let parser = graph_arguments::parser(&["codex-astra"], "codex-astra", &[]);
    let value = PyString(vec![0xd800]);
    let args = vec!["status".into(), "--run-id".into(), value.clone()];
    let ParserOutcome::Arguments(namespace) = parser.parse(&args, 80) else {
        panic!("surrogate value rejected")
    };
    assert_eq!(
        namespace.get("run_id"),
        Some(&ArgumentValue::String(value.clone()))
    );
    let ParserOutcome::Exit { stderr, code, .. } = parser.parse(&[value], 80) else {
        panic!("invalid selector accepted")
    };
    assert_eq!(code, 2);
    assert!(stderr.utf8().unwrap().contains("invalid choice: '\\ud800'"));
}

#[test]
fn short_help_tails_and_explicit_values_follow_action_arity() {
    assert!(matches!(
        parse(&["-hi there"]),
        ParserOutcome::Exit { code: 0, .. }
    ));
    assert!(error(&["-h-bad"])
        .ends_with("graphctl: error: argument -h/--help: ignored explicit argument '-bad'\n"));
    assert!(error(&["status", "--json=yes"])
        .ends_with("graphctl status: error: argument --json: ignored explicit argument 'yes'\n"));
    let namespace = namespace(&["status", "--run-id=--", "--json"]);
    assert_eq!(
        namespace.get("run_id"),
        Some(&ArgumentValue::String("--".into()))
    );
}
