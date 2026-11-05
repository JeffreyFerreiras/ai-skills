use crate::compat::{unicode, PyString, Value};

pub const ROLES: &[&str] = &[
    "impact_mapper",
    "tech_lead",
    "software_architect",
    "senior_engineer",
    "code_reviewer",
    "test_engineer",
    "audio_realtime_specialist",
    "ios_platform_specialist",
    "release_operations_reviewer",
    "security_reviewer",
    "supervisor",
];
pub const IMPACT_TAGS: &[&str] = &[
    "audio_realtime_translation",
    "cross_layer",
    "dependency",
    "ios_webkit_native",
    "persistence",
    "production_behavior",
    "release_operations",
    "security_privacy",
];
pub const DENIALS: &[&str] = &[
    "**/*.key",
    "**/*.p12",
    "**/*.pem",
    "**/*.pfx",
    "**/*credential*",
    "**/*secret*",
    "**/.env",
    "**/.env.*",
    "**/.git/**",
    "**/.hg/**",
    "**/.ssh/**",
    "**/.svn/**",
    "**/graph-inbox/**",
    "**/graph-runs/**",
    "**/id_ed25519",
    "**/id_rsa",
];
pub const RESEARCH: &[(&str, &str)] = &[
    ("design_research_architecture", "architecture"),
    ("design_research_validation", "validation"),
];
pub const SPECIALISTS: &[(&str, &str)] = &[
    ("audio_realtime_translation", "audio_realtime_specialist"),
    ("ios_webkit_native", "ios_platform_specialist"),
    ("release_operations", "release_operations_reviewer"),
    ("security_privacy", "security_reviewer"),
];
pub const NODE_SPECS: &[(&str, &str, &[&str], &str, bool, &[&str], i64)] = &[
    (
        "impact_mapper",
        "impact_mapper",
        &["bootstrap"],
        "impact_map",
        false,
        &[],
        1,
    ),
    (
        "design_research_architecture",
        "impact_mapper",
        &["research"],
        "evidence_manifest",
        true,
        &[],
        1,
    ),
    (
        "design_research_validation",
        "impact_mapper",
        &["research"],
        "evidence_manifest",
        true,
        &[],
        1,
    ),
    (
        "advisory_reviewer",
        "code_reviewer",
        &["advisory"],
        "advisory_report",
        true,
        &[],
        1,
    ),
    (
        "tech_lead",
        "tech_lead",
        &["design"],
        "technical_design",
        true,
        &[],
        1,
    ),
    (
        "architect",
        "software_architect",
        &["design"],
        "design_review",
        true,
        &["APPROVE", "REVISE", "BLOCK"],
        1,
    ),
    (
        "senior_engineer",
        "senior_engineer",
        &["implementation"],
        "implementation_handoff",
        false,
        &["IMPLEMENTED", "REDESIGN_REQUIRED"],
        1,
    ),
    (
        "code_reviewer",
        "code_reviewer",
        &["delivery"],
        "delivery_review",
        true,
        &["APPROVE", "REVISE", "BLOCK"],
        1,
    ),
    (
        "test_engineer",
        "test_engineer",
        &["delivery"],
        "delivery_review",
        true,
        &["APPROVE", "REVISE", "BLOCK"],
        1,
    ),
    (
        "audio_realtime_specialist",
        "audio_realtime_specialist",
        &["design", "delivery"],
        "specialist_review",
        true,
        &["APPROVE", "REVISE", "BLOCK"],
        1,
    ),
    (
        "ios_platform_specialist",
        "ios_platform_specialist",
        &["design", "delivery"],
        "specialist_review",
        true,
        &["APPROVE", "REVISE", "BLOCK"],
        1,
    ),
    (
        "release_operations_reviewer",
        "release_operations_reviewer",
        &["design", "delivery"],
        "specialist_review",
        true,
        &["APPROVE", "REVISE", "BLOCK"],
        1,
    ),
    (
        "security_reviewer",
        "security_reviewer",
        &["design", "delivery"],
        "specialist_review",
        true,
        &["APPROVE", "REVISE", "BLOCK"],
        1,
    ),
    (
        "supervisor_design_consolidation",
        "supervisor",
        &["design"],
        "design_consolidation",
        false,
        &[],
        0,
    ),
    (
        "supervisor_delivery_consolidation",
        "supervisor",
        &["delivery"],
        "delivery_consolidation",
        false,
        &[],
        0,
    ),
];
pub const ARTIFACT_KINDS: &[(&str, usize, &[&str])] = &[
    ("task_brief", 131072, &[".json"]),
    ("evidence_manifest", 131072, &[".json"]),
    ("technical_design", 4194304, &[".json", ".md"]),
    ("impact_map", 262144, &[".json"]),
    ("advisory_report", 262144, &[".json", ".md"]),
    ("design_review", 262144, &[".json", ".md"]),
    ("implementation_handoff", 262144, &[".json", ".md"]),
    ("delivery_review", 262144, &[".json", ".md"]),
    ("specialist_review", 262144, &[".json", ".md"]),
    ("design_consolidation", 262144, &[".json"]),
    ("delivery_consolidation", 262144, &[".json"]),
    ("collection", 2097152, &[".json"]),
    ("acceptance_evidence", 262144, &[".json", ".md", ".txt"]),
    ("check_evidence", 262144, &[".json", ".md", ".txt"]),
    ("finding", 262144, &[".json", ".md", ".txt"]),
    ("failure", 262144, &[".json", ".md", ".txt"]),
    ("branch_result", 262144, &[".json"]),
    ("timeout", 131072, &[".json", ".md", ".txt"]),
    ("skip", 131072, &[".json", ".md", ".txt"]),
    ("block", 131072, &[".json", ".md", ".txt"]),
];
pub fn artifact_maximum(kind: &str) -> Option<usize> {
    ARTIFACT_KINDS
        .iter()
        .find(|(name, _, _)| *name == kind)
        .map(|(_, maximum, _)| *maximum)
}
pub fn capability(effect: &str, action: &str, target: &str) -> Value {
    Value::object([
        ("effect", effect.into()),
        ("action", action.into()),
        ("target_ref", target.into()),
    ])
}
pub fn role_ceiling(role: &str) -> Vec<Value> {
    let mut values = vec![capability("filesystem_read", "read", "repo:docs/")];
    if [
        "tech_lead",
        "software_architect",
        "senior_engineer",
        "code_reviewer",
        "test_engineer",
        "audio_realtime_specialist",
        "ios_platform_specialist",
        "security_reviewer",
    ]
    .contains(&role)
    {
        values.push(capability("filesystem_read", "read", "repo:src/"));
    }
    if ["impact_mapper", "release_operations_reviewer"].contains(&role) {
        values.push(capability("external_read", "inspect", "andromeda"));
    }
    if role == "tech_lead" {
        values.push(capability(
            "filesystem_write",
            "edit",
            "repo:docs/technical-designs/",
        ));
    }
    if role == "senior_engineer" {
        for target in ["repo:docs/", "repo:scripts/", "repo:src/"] {
            values.push(capability("filesystem_write", "edit", target));
        }
    }
    if ["senior_engineer", "code_reviewer", "test_engineer"].contains(&role) {
        values.push(capability("command", "run", "npm-run-check"));
    }
    if !ROLES.contains(&role) {
        values.clear();
    }
    values
}
pub fn routes() -> Value {
    Value::Object(
        [
            ("advisory", "advisory_reviewer", false, false),
            ("design_only", "tech_lead", true, false),
            ("delivery_only", "senior_engineer", false, true),
            ("fast_path", "senior_engineer", false, true),
            ("full_delivery", "tech_lead", true, true),
        ]
        .into_iter()
        .map(|(key, entry, design, delivery)| {
            (
                key.into(),
                Value::object([
                    ("entry_node", entry.into()),
                    ("design_gates", design.into()),
                    ("delivery_gates", delivery.into()),
                    ("closure", "closure".into()),
                ]),
            )
        })
        .collect(),
    )
}
pub fn node_templates() -> Value {
    let mut result = Value::Object(vec![]);
    for (key, role, stages, kind, required, decisions, retries) in NODE_SPECS {
        let mut output = Value::object([
            ("artifact_kind", (*kind).into()),
            ("schema_version", 1_i64.into()),
            ("artifact_required", (*required).into()),
        ]);
        if !decisions.is_empty() {
            output.set(
                "decision_values",
                Value::list(decisions.iter().map(|&d| d.into())),
            );
        }
        if *key == "senior_engineer" {
            output.set(
                "artifact_required_for_decisions",
                Value::list(["IMPLEMENTED".into()]),
            );
            output.set(
                "evidence_required_for_decisions",
                Value::list(["REDESIGN_REQUIRED".into()]),
            );
        }
        if RESEARCH.iter().any(|(research, _)| research == key) {
            output.set("evidence_required", true.into());
            output.set("decision_forbidden", true.into());
            output.set("findings_forbidden", true.into());
        }
        let mut ordered_stages = stages.to_vec();
        ordered_stages.sort();
        result.set(
            key,
            Value::object([
                ("role", (*role).into()),
                (
                    "stages",
                    Value::list(ordered_stages.into_iter().map(Value::from)),
                ),
                ("mandatory", true.into()),
                ("max_retries", (*retries).into()),
                ("output_contract", output),
            ]),
        );
    }
    result
}
fn location_key(path: &PyString, windows: bool) -> PyString {
    let mut path = path.clone();
    while path.ends_with("/") {
        path.0.pop();
    }
    unicode::normcase(&path, windows)
}
pub fn under_configured_root(target: &PyString, roots: &[Value], windows: bool) -> bool {
    if !target.starts_with("repo:") && !target.starts_with("runtime:") {
        return false;
    }
    let relative = target.split_once(":").unwrap().1;
    let target_key = location_key(&relative, windows);
    roots.iter().filter_map(Value::as_string).any(|root| {
        let mut key = location_key(root, windows);
        if target_key == key {
            return true;
        }
        key.0.push(if windows { 92 } else { 47 });
        root.ends_with("/") && target_key.0.starts_with(&key.0)
    })
}
pub fn roots_overlap(left: &PyString, right: &PyString, windows: bool) -> bool {
    let left_key = location_key(left, windows);
    let right_key = location_key(right, windows);
    let sep = if windows { 92 } else { 47 };
    let mut left_prefix = left_key.clone();
    left_prefix.0.push(sep);
    let mut right_prefix = right_key.clone();
    right_prefix.0.push(sep);
    left_key == right_key
        || (left.ends_with("/") && right_key.0.starts_with(&left_prefix.0))
        || (right.ends_with("/") && left_key.0.starts_with(&right_prefix.0))
}
pub fn has_location_duplicates(paths: &[Value], windows: bool) -> bool {
    let keys: Vec<_> = paths
        .iter()
        .filter_map(Value::as_string)
        .map(|p| location_key(p, windows))
        .collect();
    keys.iter()
        .enumerate()
        .any(|(i, key)| keys[..i].contains(key))
}
pub fn default_profile_policy(repository_id: &str, implementation_roots: &[Value]) -> Value {
    let artifact = format!("artifacts/{repository_id}/");
    let mut capabilities = Value::Object(vec![]);
    let prefixed_capability = |effect: &str, root: &Value| {
        let mut target = PyString::from("repo:");
        target.0.extend(&root.as_string().unwrap().0);
        Value::object([
            ("effect", effect.into()),
            (
                "action",
                if effect == "filesystem_read" {
                    "read"
                } else {
                    "edit"
                }
                .into(),
            ),
            ("target_ref", Value::String(target)),
        ])
    };
    for role in ROLES {
        let mut values: Vec<_> = implementation_roots
            .iter()
            .map(|root| prefixed_capability("filesystem_read", root))
            .collect();
        values.push(capability(
            "filesystem_read",
            "read",
            &format!("runtime:{artifact}"),
        ));
        if *role == "senior_engineer" {
            values.extend(
                implementation_roots
                    .iter()
                    .map(|root| prefixed_capability("filesystem_write", root)),
            );
        }
        if ["senior_engineer", "tech_lead"].contains(role) {
            values.push(capability(
                "filesystem_write",
                "edit",
                &format!("runtime:{artifact}"),
            ));
        }
        if ["senior_engineer", "code_reviewer", "test_engineer"].contains(role) {
            values.push(capability("command", "run", "git-diff-check"));
        }
        capabilities.set(role, Value::List(values));
    }
    Value::object([
        ("schema_version", 3_i64.into()),
        ("repository_id", repository_id.into()),
        (
            "compatible_engine",
            Value::object([("min", "2.7.0".into()), ("max_exclusive", "3.0.0".into())]),
        ),
        (
            "artifact_roots",
            Value::object([
                ("repo", Value::List(vec![])),
                ("profile", Value::list(["references/".into()])),
                ("runtime", Value::list([artifact.into()])),
            ]),
        ),
        (
            "implementation_roots",
            Value::List(implementation_roots.to_vec()),
        ),
        (
            "artifact_kinds",
            Value::Object(
                ARTIFACT_KINDS
                    .iter()
                    .map(|(kind, max, extensions)| {
                        (
                            (*kind).into(),
                            Value::object([
                                (
                                    "extensions",
                                    Value::list(extensions.iter().map(|&e| e.into())),
                                ),
                                ("max_bytes", (*max).into()),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "impact_tags",
            Value::list(IMPACT_TAGS.iter().map(|&tag| tag.into())),
        ),
        ("routes", routes()),
        ("node_templates", node_templates()),
        (
            "specialists",
            Value::Object(
                SPECIALISTS
                    .iter()
                    .map(|(tag, node)| {
                        (
                            (*tag).into(),
                            Value::object([
                                ("node_key", (*node).into()),
                                ("role", (*node).into()),
                                ("stages", Value::list(["design".into(), "delivery".into()])),
                                ("mandatory", true.into()),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "limits",
            Value::object([
                ("design_revisions", 2_i64.into()),
                ("delivery_repairs", 3_i64.into()),
                (
                    "inspection",
                    Value::object([
                        ("file_reads", 12_i64.into()),
                        ("discovery_commands", 8_i64.into()),
                    ]),
                ),
                ("manifest_bytes", 262144_i64.into()),
                ("artifact_bytes", 4194304_i64.into()),
            ]),
        ),
        (
            "required_checks",
            Value::object([(
                "diff-check",
                Value::object([
                    ("command_id", "git-diff-check".into()),
                    ("mandatory", true.into()),
                    (
                        "argv",
                        Value::list(["git".into(), "diff".into(), "--check".into()]),
                    ),
                    ("timeout_seconds", 60_i64.into()),
                ]),
            )]),
        ),
        ("role_capabilities", capabilities),
        (
            "denied_patterns",
            Value::list(DENIALS.iter().map(|&d| d.into())),
        ),
    ])
}
