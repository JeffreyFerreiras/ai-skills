use graph_engine::{
    compat::Value,
    domain::{
        execution, hosts,
        planner::{self, NodeSpec},
    },
};
use num_bigint::BigInt;

#[test]
fn research_envelope_splits_arbitrary_size_inspection_integers() {
    let amount: BigInt = (BigInt::from(1u64) << 180usize) + 1;
    let task = Value::object([
        ("schema_version", 1i64.into()),
        ("task_id", "TASK".into()),
        ("request_mode", "delivery".into()),
        ("minimum_route", "full_delivery".into()),
        ("risk_level", "low".into()),
        ("mandatory_impact_tags", Value::List(vec![])),
        (
            "scope",
            Value::object([
                ("included", Value::List(vec![])),
                ("excluded", Value::List(vec![])),
            ]),
        ),
        ("constraints", Value::List(vec![])),
        ("acceptance_ids", Value::List(vec!["AC-001".into()])),
        ("required_check_ids", Value::List(vec![])),
        ("required_human_decisions", Value::List(vec![])),
        (
            "authority",
            Value::object([("capabilities", Value::List(vec![]))]),
        ),
        (
            "inspection_budget",
            Value::object([
                ("file_reads", Value::Integer(amount.clone())),
                ("discovery_commands", Value::Integer(amount.clone())),
            ]),
        ),
    ]);
    let mut policy = Value::object([
        (
            "node_templates",
            Value::object([(
                "design_research_architecture",
                Value::object([
                    ("max_retries", 1i64.into()),
                    (
                        "output_contract",
                        Value::object([("artifact_kind", "evidence_manifest".into())]),
                    ),
                ]),
            )]),
        ),
        (
            "role_capabilities",
            Value::object([("software_architect", Value::List(vec![]))]),
        ),
    ]);
    let plan = execution::build_execution_plan("RUN", &task, None, hosts::DEFAULT_HOST).unwrap();
    let node = NodeSpec {
        key: "design_research_architecture".into(),
        role: "software_architect".into(),
        stage: "research".into(),
        generation: 0,
        mandatory: true,
        specialist_tag: None,
    };
    let envelope = planner::envelope(
        "RUN",
        &"a".repeat(64),
        &policy,
        &task,
        &node,
        "pending",
        &[],
        0,
        Some(&plan),
        cfg!(windows),
    )
    .unwrap();
    assert!(
        envelope["research_assignment"]["inspection_budget"]["file_reads"]
            .py_eq(&Value::Integer((&amount + 1) / 2))
    );
    assert!(
        envelope["research_assignment"]["inspection_budget"]["discovery_commands"]
            .py_eq(&Value::Integer((&amount + 1) / 2))
    );
    let mut templates = policy["node_templates"].clone();
    templates.set(
        "design_research_validation",
        templates["design_research_architecture"].clone(),
    );
    policy.set("node_templates", templates);
    let mut roles = policy["role_capabilities"].clone();
    roles.set("test_engineer", Value::List(vec![]));
    policy.set("role_capabilities", roles);
    for (input, first, second) in [
        (Value::Float(5.9), 3, 2),
        (Value::from("١_٢"), 6, 6),
        (Value::from(true), 1, 0),
        (Value::from(-4i64), -2, -2),
        (Value::from(-3i64), -1, -2),
        (Value::from("-1"), 0, -1),
        (Value::Bytes(b" 12 ".to_vec()), 6, 6),
    ] {
        let mut adjusted = task.clone();
        adjusted.set("inspection_budget", Value::object([("file_reads", input)]));
        for (key, role, expected) in [
            ("design_research_architecture", "software_architect", first),
            ("design_research_validation", "test_engineer", second),
        ] {
            let node = NodeSpec {
                key: key.into(),
                role: role.into(),
                ..node.clone()
            };
            let envelope = planner::envelope(
                "RUN",
                &"a".repeat(64),
                &policy,
                &adjusted,
                &node,
                "pending",
                &[],
                0,
                Some(&plan),
                cfg!(windows),
            )
            .unwrap();
            assert_eq!(
                envelope["research_assignment"]["inspection_budget"]["file_reads"].as_i64(),
                Some(i64::from(expected))
            );
        }
    }
    for (input, kind, message) in [
        (
            Value::Null,
            "TypeError",
            "int() argument must be a string, a bytes-like object or a real number, not 'NoneType'",
        ),
        (
            Value::Float(f64::INFINITY),
            "OverflowError",
            "cannot convert float infinity to integer",
        ),
        (
            Value::ConstantNaN,
            "ValueError",
            "cannot convert float NaN to integer",
        ),
    ] {
        let mut adjusted = task.clone();
        adjusted.set("inspection_budget", Value::object([("file_reads", input)]));
        let error = planner::envelope(
            "RUN",
            &"a".repeat(64),
            &policy,
            &adjusted,
            &node,
            "pending",
            &[],
            0,
            Some(&plan),
            cfg!(windows),
        )
        .unwrap_err();
        assert!(
            matches!(error,graph_engine::domain::errors::ValidationFailure::Python{kind:actual,message:actual_message}if actual==kind&&actual_message.equals(message))
        );
    }
}
