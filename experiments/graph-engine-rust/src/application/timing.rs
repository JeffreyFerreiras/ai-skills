use super::{
    error::{EngineError, EngineResult},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
    topology_history,
};
use crate::{
    compat::{datetime, PyString, Value},
    domain::{
        contract_values::{get, in_strings},
        errors::{iterable_values, string_item},
    },
};
use num_bigint::BigInt;
use num_traits::Zero;
use std::collections::{BTreeMap, BTreeSet};

pub type TimingEdges = BTreeSet<(PyString, PyString)>;
pub const TERMINAL: &[&str] = &["succeeded", "failed", "timed_out", "skipped"];

pub fn timestamp_microseconds(value: &Value) -> EngineResult<i64> {
    datetime::strict_utc_microseconds(value).ok_or_else(|| EngineError::state("TIMESTAMP_INVALID"))
}

fn identifier(value: &Value) -> EngineResult<PyString> {
    value.as_string().cloned().ok_or_else(|| {
        EngineError::python(
            "TypeError",
            format!("expected str, got {}", value.python_type()),
        )
    })
}

fn milliseconds(microseconds: BigInt) -> Value {
    let divisor = BigInt::from(1000);
    let remainder = &microseconds % &divisor;
    let quotient = &microseconds / &divisor;
    Value::Integer(if remainder < BigInt::zero() {
        quotient - 1
    } else {
        quotient
    })
}

pub fn attempt_intervals(
    connection: &dyn GraphLedgerReadPort,
    branch_id: &Value,
) -> EngineResult<Vec<(i64, i64)>> {
    let mut intervals = Vec::new();
    connection.visit(
        GraphRead::TimingAttemptIntervals,
        &[branch_id.clone()],
        &mut |attempt| {
            if !string_item(&attempt, "finished_at")?.is_null() {
                let start = timestamp_microseconds(string_item(&attempt, "started_at")?)?;
                let finish = timestamp_microseconds(string_item(&attempt, "finished_at")?)?;
                intervals.push((start, finish));
            }
            Ok(())
        },
    )?;
    Ok(intervals)
}

pub fn semantic_edges(
    connection: &dyn GraphLedgerReadPort,
    run_id: &Value,
) -> EngineResult<TimingEdges> {
    let mut edges = BTreeSet::new();
    for response in topology_history::operation_responses(connection, run_id)? {
        if !get(&response, "code")?.is_text("JOIN_ADVANCED") {
            continue;
        }
        let join_id = string_item(&response, "join_id")?;
        let join = connection
            .first(GraphRead::Join, &[join_id.clone()])?
            .ok_or_else(|| EngineError::state("TOPOLOGY_HISTORY_INVALID"))?;
        let members = connection.all(
            GraphRead::JoinMembers,
            &[string_item(&join, "join_id")?.clone()],
        )?;
        for member in members {
            let empty = Value::List(vec![]);
            for successor in
                iterable_values(response.get("successor_branch_ids").unwrap_or(&empty))?
            {
                edges.insert((
                    identifier(string_item(&member, "branch_id")?)?,
                    identifier(&successor)?,
                ));
            }
        }
    }
    connection.visit(
        GraphRead::FanoutDependencyEdges,
        &[run_id.clone()],
        &mut |row| {
            edges.insert((
                identifier(string_item(&row, "before_branch_id")?)?,
                identifier(string_item(&row, "after_branch_id")?)?,
            ));
            Ok(())
        },
    )?;
    Ok(edges)
}

pub fn critical_path(
    branch_ids: &[PyString],
    weights: &BTreeMap<PyString, BigInt>,
    edges: &TimingEdges,
) -> EngineResult<(Vec<PyString>, BigInt)> {
    let selected = branch_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut incoming: BTreeMap<_, BTreeSet<PyString>> = branch_ids
        .iter()
        .cloned()
        .map(|id| (id, BTreeSet::new()))
        .collect();
    let mut outgoing = incoming.clone();
    for (before, after) in edges {
        if selected.contains(before) && selected.contains(after) {
            incoming.get_mut(after).unwrap().insert(before.clone());
            outgoing.get_mut(before).unwrap().insert(after.clone());
        }
    }
    let mut ready = branch_ids
        .iter()
        .filter(|id| incoming[*id].is_empty())
        .cloned()
        .collect::<Vec<_>>();
    ready.sort();
    let mut order = Vec::new();
    let mut remaining = incoming.clone();
    while !ready.is_empty() {
        let current = ready.remove(0);
        order.push(current.clone());
        for successor in &outgoing[&current] {
            let predecessors = remaining.get_mut(successor).unwrap();
            predecessors.remove(&current);
            if predecessors.is_empty() && !order.contains(successor) && !ready.contains(successor) {
                ready.push(successor.clone());
                ready.sort();
            }
        }
    }
    if order.len() != branch_ids.len() {
        return Err(EngineError::state("TIMING_GRAPH_INVALID"));
    }
    let mut best: BTreeMap<PyString, (BigInt, Vec<PyString>)> = BTreeMap::new();
    for branch in order {
        let previous = incoming[&branch]
            .iter()
            .map(|id| &best[id])
            .min_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        let (mut weight, mut path) = previous
            .cloned()
            .unwrap_or_else(|| (BigInt::zero(), vec![]));
        weight += weights.get(&branch).cloned().unwrap_or_else(BigInt::zero);
        path.push(branch.clone());
        best.insert(branch, (weight, path));
    }
    Ok(best
        .into_values()
        .min_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)))
        .map(|(weight, path)| (path, weight))
        .unwrap_or_else(|| (vec![], BigInt::zero())))
}

fn extremum_timestamp(
    current: &mut Option<(i64, Value)>,
    value: &Value,
    maximum: bool,
) -> EngineResult<()> {
    let timestamp = timestamp_microseconds(value)?;
    if current.as_ref().is_none_or(|(previous, _)| {
        if maximum {
            timestamp > *previous
        } else {
            timestamp < *previous
        }
    }) {
        *current = Some((timestamp, value.clone()));
    }
    Ok(())
}

pub fn group_timing(
    connection: &dyn GraphLedgerReadPort,
    nodes: &[Value],
    edges: &TimingEdges,
    complete: bool,
) -> EngineResult<Value> {
    let mut intervals = Vec::new();
    for node in nodes {
        intervals.extend(attempt_intervals(
            connection,
            string_item(node, "branch_id")?,
        )?);
    }
    let mut finishes = Vec::new();
    for node in nodes {
        let value = string_item(node, "finished_at")?;
        if value.truthy() {
            finishes.push(timestamp_microseconds(value)?);
        }
    }
    let mut started = None;
    for node in nodes {
        connection.visit(
            GraphRead::TimingAttemptStarts,
            &[string_item(node, "branch_id")?.clone()],
            &mut |row| extremum_timestamp(&mut started, string_item(&row, "started_at")?, false),
        )?;
    }
    let mut finished = None;
    for node in nodes {
        let value = string_item(node, "finished_at")?;
        if value.truthy() {
            extremum_timestamp(&mut finished, value, true)?;
        }
    }
    let mut result = Value::object([
        ("timing_complete", complete.into()),
        (
            "started_at",
            started.map(|(_, value)| value).unwrap_or(Value::Null),
        ),
        (
            "finished_at",
            finished.map(|(_, value)| value).unwrap_or(Value::Null),
        ),
        ("wall_time_ms", Value::Null),
        ("active_duration_ms", Value::Null),
        ("overlap_time_ms", Value::Null),
        ("slowest_branch", Value::Null),
        ("critical_path", Value::Null),
    ]);
    if !complete || intervals.is_empty() {
        return Ok(result);
    }
    let start = intervals.iter().map(|(start, _)| *start).min().unwrap();
    let finish = finishes
        .into_iter()
        .max()
        .ok_or_else(|| EngineError::python("ValueError", "max() iterable argument is empty"))?;
    let points = intervals
        .iter()
        .flat_map(|(start, finish)| [*start, *finish])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let overlap: BigInt = points
        .windows(2)
        .filter(|pair| {
            intervals
                .iter()
                .filter(|(begin, end)| *begin <= pair[0] && pair[0] < *end)
                .count()
                >= 2
        })
        .map(|pair| BigInt::from(pair[1] - pair[0]))
        .sum();
    let mut active = BTreeMap::new();
    let mut active_order = Vec::new();
    for node in nodes {
        let branch = string_item(node, "branch_id")?;
        let duration: BigInt = attempt_intervals(connection, branch)?
            .into_iter()
            .map(|(begin, end)| BigInt::from(end - begin))
            .sum();
        let branch = identifier(branch)?;
        if !active.contains_key(&branch) {
            active_order.push(branch.clone());
        }
        active.insert(branch, duration);
    }
    let (slowest, slowest_duration) = active
        .iter()
        .min_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)))
        .unwrap();
    let (path, duration) = critical_path(&active_order, &active, edges)?;
    result.set("wall_time_ms", milliseconds(BigInt::from(finish - start)));
    result.set(
        "active_duration_ms",
        milliseconds(active.values().cloned().sum()),
    );
    result.set("overlap_time_ms", milliseconds(overlap));
    result.set(
        "slowest_branch",
        Value::object([
            ("branch_id", Value::String(slowest.clone())),
            ("active_duration_ms", milliseconds(slowest_duration.clone())),
        ]),
    );
    result.set(
        "critical_path",
        Value::object([
            (
                "branch_ids",
                Value::List(path.into_iter().map(Value::String).collect()),
            ),
            ("active_duration_ms", milliseconds(duration)),
        ]),
    );
    Ok(result)
}

fn branch_timing(connection: &dyn GraphLedgerReadPort, node: &Value) -> EngineResult<Value> {
    let attempts = connection.all(
        GraphRead::TimingAttempts,
        &[string_item(node, "branch_id")?.clone()],
    )?;
    let settled = in_strings(string_item(node, "status")?, TERMINAL)?;
    let mut active = None;
    let mut wall = None;
    if settled && !attempts.is_empty() {
        let mut duration = BigInt::zero();
        for attempt in &attempts {
            if !string_item(attempt, "finished_at")?.is_null() {
                // The original subtraction evaluates its left operand first.
                let finish = timestamp_microseconds(string_item(attempt, "finished_at")?)?;
                let start = timestamp_microseconds(string_item(attempt, "started_at")?)?;
                duration += BigInt::from(finish - start);
            }
        }
        active = Some(duration);
        let finish = timestamp_microseconds(string_item(node, "finished_at")?)?;
        let start = timestamp_microseconds(string_item(node, "started_at")?)?;
        wall = Some(BigInt::from(finish - start));
    }
    Ok(Value::object([
        ("started_at", string_item(node, "started_at")?.clone()),
        ("finished_at", string_item(node, "finished_at")?.clone()),
        (
            "wall_time_ms",
            wall.map(milliseconds).unwrap_or(Value::Null),
        ),
        (
            "active_duration_ms",
            active.map(milliseconds).unwrap_or(Value::Null),
        ),
        ("attempt_count", attempts.len().into()),
        ("timing_complete", settled.into()),
    ]))
}

pub fn compute_timing(connection: &dyn GraphLedgerReadPort, run: &Value) -> EngineResult<Value> {
    let run_id = string_item(run, "run_id")?;
    let nodes = connection.all(GraphRead::NodesForStatus, &[run_id.clone()])?;
    let edges = semantic_edges(connection, run_id)?;
    let mut branches = Vec::new();
    for node in &nodes {
        branches.push((
            identifier(string_item(node, "branch_id")?)?,
            branch_timing(connection, node)?,
        ));
    }
    let mut groups = Vec::new();
    for node in &nodes {
        let group = (
            identifier(string_item(node, "stage")?)?,
            string_item(node, "generation")?.clone(),
        );
        super::super::domain::errors::require_hashable(&group.1)?;
        if !groups
            .iter()
            .any(|(stage, generation): &(PyString, Value)| {
                *stage == group.0 && generation.container_eq(&group.1)
            })
        {
            groups.push(group);
        }
    }
    groups.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.strict_integer().cmp(&right.1.strict_integer()))
    });
    let mut stages = Vec::new();
    for (stage, generation) in groups {
        let stage_nodes = nodes
            .iter()
            .filter(|node| {
                node["stage"].as_string() == Some(&stage) && node["generation"].py_eq(&generation)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut complete = true;
        for node in &stage_nodes {
            if !in_strings(string_item(node, "status")?, TERMINAL)? {
                complete = false;
                break;
            }
        }
        let mut timing = group_timing(connection, &stage_nodes, &edges, complete)?;
        timing.set("stage", Value::String(stage));
        timing.set("generation", generation);
        stages.push(timing);
    }
    let mut complete = in_strings(
        string_item(run, "status")?,
        &["blocked", "complete", "aborted"],
    )?;
    if complete {
        for node in &nodes {
            if !in_strings(string_item(node, "status")?, TERMINAL)? {
                complete = false;
                break;
            }
        }
    }
    let overall = group_timing(connection, &nodes, &edges, complete)?;
    let run_started = timestamp_microseconds(string_item(run, "started_at")?)?;
    let finish = string_item(run, "finished_at")?;
    let run_finished = if finish.truthy() {
        Some(timestamp_microseconds(finish)?)
    } else {
        None
    };
    Ok(Value::object([
        ("clock_basis", "utc_wall".into()),
        ("branches", Value::Object(branches)),
        (
            "run",
            Value::object([
                ("started_at", string_item(run, "started_at")?.clone()),
                ("finished_at", finish.clone()),
                (
                    "wall_time_ms",
                    run_finished
                        .map(|end| milliseconds(BigInt::from(end - run_started)))
                        .unwrap_or(Value::Null),
                ),
                ("timing_complete", run_finished.is_some().into()),
            ]),
        ),
        ("stages", Value::List(stages)),
        ("overall", overall),
    ]))
}
