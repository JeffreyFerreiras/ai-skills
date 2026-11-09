use super::DomainResult;
use std::collections::BTreeMap;

pub const DEFAULT_HOST: &str = "codex";
pub const LEGACY_HOST: &str = "codex";
pub const MUSE_MODEL: &str = "opencode-go/muse-spark-1.3-contributor";
pub const RETIRED_MODEL_PREFIX: &str = "gpt-5.6-";
pub const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
pub const MUSE_EFFORTS: &[&str] = &["minimal", "low", "medium", "high", "xhigh"];
pub const MUSE_DEFAULT_NODES: &[&str] = &[
    "impact_mapper",
    "design_research_architecture",
    "design_research_validation",
    "code_reviewer_naming",
    "test_engineer",
];
pub const HOSTS: &[&str] = &["claude", "codex", "codex-astra", "cursor"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row {
    pub class: &'static str,
    pub requested: &'static str,
    pub model: &'static str,
    pub effort: &'static str,
    pub dispatch: &'static str,
}
pub fn current_revision(host: &str) -> DomainResult<i64> {
    match host {
        "claude" => Ok(4),
        "codex" => Ok(11),
        "codex-astra" => Ok(10),
        "cursor" => Ok(5),
        _ => Err("HOST_UNSUPPORTED".into()),
    }
}
pub fn supported_revisions(host: &str) -> &'static [i64] {
    match host {
        "claude" => &[1, 3, 4],
        "codex" => &[2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        "codex-astra" => &[2, 3, 4, 5, 6, 7, 8, 9, 10],
        "cursor" => &[2, 3, 4, 5],
        _ => &[],
    }
}
pub fn is_retired_model(model: &str) -> bool {
    model.starts_with(RETIRED_MODEL_PREFIX)
}
pub fn require_active_model(model: &str) -> DomainResult<()> {
    if is_retired_model(model) {
        Err("MODEL_RETIRED".into())
    } else {
        Ok(())
    }
}
pub fn catalog_for(host: &str) -> DomainResult<Vec<Row>> {
    let (economy, economy_effort, reasoning) = match host {
        "claude" => ("claude-sonnet-5", "low", "claude-opus-5"),
        "codex" => ("gpt-5.6-luna", "max", "gpt-5.6-sol"),
        "codex-astra" => ("gpt-5.6-luna", "max", "gpt-6-astra"),
        "cursor" => ("composer-2.5", "high", "cursor-grok-4.6"),
        _ => return Err("HOST_UNSUPPORTED".into()),
    };
    let mut rows = vec![Row {
        class: "economy",
        requested: "max",
        model: economy,
        effort: economy_effort,
        dispatch: economy,
    }];
    for &requested in if host == "codex-astra" {
        &["low", "medium", "high", "xhigh", "max"][..]
    } else {
        &["medium", "high", "xhigh", "max"][..]
    } {
        let effort = if host == "cursor" && requested == "max" {
            "xhigh"
        } else {
            requested
        };
        let dispatch = if host == "cursor" {
            match effort {
                "medium" => "cursor-grok-4.6-medium",
                "high" => "cursor-grok-4.6-high",
                _ => "cursor-grok-4.6-xhigh",
            }
        } else {
            reasoning
        };
        rows.push(Row {
            class: "reasoning",
            requested,
            model: reasoning,
            effort,
            dispatch,
        });
    }
    rows.push(Row {
        class: "primary-thread",
        requested: "inherited",
        model: "primary-thread",
        effort: "inherited",
        dispatch: "primary-thread",
    });
    Ok(rows)
}
pub fn resolve_row(host: &str, class: &str, requested: &str) -> DomainResult<Row> {
    catalog_for(host)?
        .into_iter()
        .find(|row| row.class == class && row.requested == requested)
        .ok_or_else(|| "MODEL_ASSIGNMENT_INVALID".into())
}
pub fn resolve_assignment(
    host: &str,
    class: &str,
    requested: &str,
) -> DomainResult<(&'static str, &'static str)> {
    let row = resolve_row(host, class, requested)?;
    Ok((row.model, row.effort))
}
pub fn dispatch_model(host: &str, model: &str, effort: &str) -> DomainResult<String> {
    catalog_for(host)?
        .into_iter()
        .find(|row| row.model == model && row.effort == effort)
        .map(|row| row.dispatch.to_owned())
        .ok_or_else(|| "MODEL_ASSIGNMENT_INVALID".into())
}
pub fn classify(host: &str, model: &str) -> DomainResult<Option<&'static str>> {
    Ok(catalog_for(host)?
        .into_iter()
        .find(|row| row.model == model)
        .map(|row| row.class))
}
pub fn economy_effort(host: &str) -> DomainResult<&'static str> {
    catalog_for(host)?
        .into_iter()
        .find(|row| row.class == "economy")
        .map(|row| row.effort)
        .ok_or_else(|| "MODEL_ASSIGNMENT_INVALID".into())
}
pub fn supervisor_recommendation(host: &str) -> DomainResult<Row> {
    resolve_row(
        host,
        "reasoning",
        if host == "cursor" { "high" } else { "xhigh" },
    )
}
pub fn publication_assignment(host: &str) -> DomainResult<Row> {
    resolve_row(host, "economy", "max")
}
pub fn model_options(
    host: &str,
    revision: Option<i64>,
) -> DomainResult<BTreeMap<&'static str, Vec<&'static str>>> {
    catalog_for(host)?;
    let revision = revision.unwrap_or(current_revision(host)?);
    let mut result = BTreeMap::new();
    match host {
        "codex" | "codex-astra" => {
            for model in [
                "gpt-5.6-luna",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-6-astra",
            ] {
                result.insert(model, EFFORTS.to_vec());
            }
            if revision != 3 {
                for model in ["gpt-6-luna", "gpt-6-sol"] {
                    result.insert(model, EFFORTS.to_vec());
                }
            }
            if ![2, 3, 4, 5, 6].contains(&revision) {
                result.insert("gpt-6.1-sol", EFFORTS.to_vec());
            }
            if ![2, 3, 4, 5, 6, 7].contains(&revision) {
                result.retain(|model, _| !is_retired_model(model));
            }
            if [9, 10, 11].contains(&revision) {
                result.insert(MUSE_MODEL, MUSE_EFFORTS.to_vec());
            }
            if [10, 11].contains(&revision) {
                result.insert(
                    "gpt-6.1-terra",
                    vec!["low", "medium", "high", "xhigh", "max", "ultra"],
                );
            }
        }
        "claude" => {
            for model in ["claude-sonnet-5", "claude-opus-5", "claude-fable-5-1"] {
                result.insert(model, EFFORTS.to_vec());
            }
            if revision == 4 {
                for model in ["claude-sonnet-5-5", "claude-opus-5-5"] {
                    result.insert(model, EFFORTS.to_vec());
                }
            }
        }
        "cursor" => {
            result.insert("gemini-3.8-flash", vec!["low", "medium", "high"]);
            result.insert("grok-4.7", vec!["low", "medium", "high", "xhigh"]);
            result.insert("composer-2.5", vec!["high"]);
            for model in [
                "claude-sonnet-5",
                "claude-opus-5",
                "claude-fable-5-1",
                "gpt-5.6-sol",
            ] {
                result.insert(model, EFFORTS.to_vec());
            }
            if ![2, 3, 5, 6, 7].contains(&revision) {
                result.remove("gpt-5.6-sol");
            }
            if revision == 5 {
                result.remove("gpt-5.6-sol");
                for model in ["claude-sonnet-5-5", "claude-opus-5-5"] {
                    result.insert(model, EFFORTS.to_vec());
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(result)
}
pub fn selected_dispatch_model(
    host: &str,
    model: &str,
    effort: &str,
    revision: Option<i64>,
) -> DomainResult<String> {
    let current = current_revision(host)?;
    if revision.is_none() || revision == Some(current) {
        require_active_model(model)?;
    }
    if model_options(host, revision)?
        .get(model)
        .is_some_and(|efforts| efforts.contains(&effort))
    {
        return Ok(model.into());
    }
    dispatch_model(host, model, effort)
}
pub fn recommended_assignment(
    host: &str,
    workload: &str,
    revision: Option<i64>,
) -> DomainResult<(String, String)> {
    let revision = revision.unwrap_or(current_revision(host)?);
    catalog_for(host)?;
    if workload == "helper" {
        if host == "claude" && revision == 4 {
            return Ok(("claude-sonnet-5-5".into(), "low".into()));
        }
        if ["codex", "codex-astra"].contains(&host) {
            if [8, 9, 10, 11].contains(&revision) {
                return Ok(("gpt-6.1-sol".into(), "low".into()));
            }
            if [6, 7].contains(&revision) {
                return recommended_assignment(host, "implementation", Some(revision));
            }
        }
        let (model, effort) = match host {
            "claude" => ("claude-sonnet-5", "low"),
            "cursor" => ("gemini-3.8-flash", "low"),
            _ => (
                if revision == 3 {
                    "gpt-5.6-luna"
                } else {
                    "gpt-6-luna"
                },
                if [3, 4].contains(&revision) {
                    "low"
                } else {
                    "max"
                },
            ),
        };
        return Ok((model.into(), effort.into()));
    }
    if host == "codex" && revision == 11 {
        return Ok(("gpt-6.1-sol".into(), "xhigh".into()));
    }
    let model = if host == "claude" && revision == 4 {
        "claude-opus-5-5"
    } else if host == "codex" && [7, 8, 9, 10].contains(&revision) {
        "gpt-6.1-sol"
    } else {
        match host {
            "claude" => "claude-opus-5",
            "cursor" => "grok-4.7",
            "codex" => {
                if revision == 3 {
                    "gpt-5.6-sol"
                } else {
                    "gpt-6-sol"
                }
            }
            _ => "gpt-6-astra",
        }
    };
    Ok((
        model.into(),
        if workload == "review" {
            "high"
        } else {
            "medium"
        }
        .into(),
    ))
}
fn reasoning_weight(effort: &str) -> Option<i64> {
    match effort {
        "high" => Some(3),
        "xhigh" => Some(4),
        "max" => Some(5),
        _ => None,
    }
}
pub fn dispatch_weight_for(model: &str, effort: &str, historical: bool) -> Option<i64> {
    if is_retired_model(model) && !historical {
        return None;
    }
    if ["claude-opus-5-5", "gpt-6.1-sol", "gpt-6-sol"].contains(&model) {
        if let Some(weight) = reasoning_weight(effort) {
            return Some(weight);
        }
    }
    if (model == "gpt-6-astra" && effort == "medium") || (model == "gpt-6-luna" && effort == "max")
    {
        return Some(3);
    }
    if model == "primary-thread" {
        return None;
    }
    for host in HOSTS {
        if let Some(class) = classify(host, model).ok().flatten() {
            return match class {
                "economy" => {
                    if economy_effort(host).ok() == Some(effort) {
                        Some(3)
                    } else {
                        None
                    }
                }
                "reasoning" => reasoning_weight(effort),
                _ => None,
            };
        }
    }
    None
}
pub fn supported_dispatch_weights() -> BTreeMap<(String, String), i64> {
    let mut result = BTreeMap::new();
    for model in ["claude-opus-5-5", "gpt-6.1-sol", "gpt-6-sol"] {
        for effort in ["high", "xhigh", "max"] {
            result.insert(
                (model.into(), effort.into()),
                reasoning_weight(effort).unwrap(),
            );
        }
    }
    result.insert(("gpt-6-astra".into(), "medium".into()), 3);
    result.insert(("gpt-6-luna".into(), "max".into()), 3);
    for host in HOSTS {
        for row in catalog_for(host).unwrap() {
            if !is_retired_model(row.model) {
                if let Some(weight) = dispatch_weight_for(row.model, row.effort, false) {
                    result.insert((row.model.into(), row.effort.into()), weight);
                }
            }
        }
    }
    result
}
pub fn is_retired_python_model(
    model: &crate::compat::Value,
) -> super::errors::ValidationResult<bool> {
    if matches!(model, crate::compat::Value::Bytes(_)) {
        return Err(super::errors::ValidationFailure::python(
            "TypeError",
            "startswith first arg must be bytes or a tuple of bytes, not str",
        ));
    }
    let text = model
        .as_string()
        .ok_or_else(|| super::errors::attribute_error(model, "startswith"))?;
    Ok(text.starts_with(RETIRED_MODEL_PREFIX))
}
