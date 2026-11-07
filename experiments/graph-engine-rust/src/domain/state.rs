use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateError {
    pub code: String,
}
impl StateError {
    pub fn new(code: &str) -> Self {
        Self { code: code.into() }
    }
}
impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code)
    }
}
impl std::error::Error for StateError {}

pub const MUTATIONS: &[(&str, &[&str])] = &[
    ("evidence.enable", &["active"]),
    ("check.start", &["active"]),
    ("check.finish", &["active"]),
    ("check.abandon", &["active"]),
    (
        "record.usage",
        &["initialized", "active", "blocked", "complete", "aborted"],
    ),
    ("next.claim", &["initialized", "active"]),
    ("record.branch-result", &["initialized", "active"]),
    ("record.timeout", &["initialized", "active"]),
    ("record.skip", &["active"]),
    ("record.retry", &["initialized", "active"]),
    ("record.approval", &["initialized", "active"]),
    ("record.plan-approval", &["initialized"]),
    ("record.heartbeat", &["initialized", "active"]),
    ("record.budget-use", &["initialized", "active"]),
    ("record.acceptance-evidence", &["active"]),
    ("record.check-evidence", &["active"]),
    ("record.fanout-assessment", &["active"]),
    ("record.review-fanout", &["active"]),
    ("record.review-fanout-assessment", &["active"]),
    ("check.run", &["active"]),
    ("join.advance", &["active"]),
    ("complete", &["active"]),
    ("block", &["initialized", "active"]),
    ("abort", &["initialized", "active", "blocked"]),
    ("recover", &["blocked"]),
];
pub fn allowed_states(command: &str) -> Option<&'static [&'static str]> {
    MUTATIONS
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(_, states)| *states)
}
pub fn validate_mutation_state(command: &str, status: &str) -> Result<(), StateError> {
    let states = allowed_states(command).ok_or_else(|| StateError::new("UNKNOWN_MUTATION"))?;
    if states.contains(&status) {
        return Ok(());
    }
    Err(StateError::new(match status {
        "complete" | "aborted" => "TERMINAL_RUN",
        "blocked" => "GRAPH_BLOCKED",
        _ => "INVALID_RUN_TRANSITION",
    }))
}
