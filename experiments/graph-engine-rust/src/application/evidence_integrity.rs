use super::{
    check_receipts::{validate_receipt_integrity, CheckProvenancePort},
    error::{EngineError, EngineResult},
    evidence_lifecycle::{evidence_activation, lifecycle_artifacts, reservation_states},
    graph_ledger::{GraphLedgerReadPort, GraphRead},
};
use crate::{
    compat::{self, repr, Value},
    domain::{
        check_receipts::configured_check,
        contract_values::{get, in_strings},
        contracts, errors, ids, python_operations,
        result_contract::is_actionable_finding,
        source_paths,
    },
};

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}

fn invalid(code: &str) -> EngineError {
    EngineError::state(code)
}

fn key_error(key: &Value) -> EngineResult<EngineError> {
    Ok(EngineError::python("KeyError", repr::value_repr(key)?))
}

fn equal_sets(left: &Value, right: &Value) -> EngineResult<bool> {
    let left = errors::iterable_set(left)?;
    let right = errors::iterable_set(right)?;
    Ok(left.len() == right.len() && left.iter().all(|key| errors::set_contains(&right, key)))
}

fn insert_set(set: &mut Vec<Value>, key: &Value) -> EngineResult<()> {
    errors::require_hashable(key)?;
    if !errors::set_contains(set, key) {
        set.push(key.clone());
    }
    Ok(())
}

fn insert_mapping(
    entries: &mut Vec<(Value, Value)>,
    key: &Value,
    value: &Value,
) -> EngineResult<()> {
    errors::require_hashable(key)?;
    if let Some((_, previous)) = entries
        .iter_mut()
        .find(|(candidate, _)| candidate.container_eq(key))
    {
        *previous = value.clone();
    } else {
        entries.push((key.clone(), value.clone()));
    }
    Ok(())
}

fn mapping<'a>(entries: &'a [(Value, Value)], key: &Value) -> EngineResult<Option<&'a Value>> {
    errors::require_hashable(key)?;
    Ok(entries
        .iter()
        .find(|(candidate, _)| candidate.container_eq(key))
        .map(|(_, value)| value))
}

fn add_consumed(consumed: &mut [(Value, Value)], key: &Value, amount: &Value) -> EngineResult<()> {
    errors::require_hashable(key)?;
    let Some((_, value)) = consumed
        .iter_mut()
        .find(|(candidate, _)| candidate.container_eq(key))
    else {
        return Err(key_error(key)?);
    };
    *value = python_operations::add(value, amount)?;
    Ok(())
}

fn spread(value: &Value) -> EngineResult<Value> {
    if value.as_object().is_none() {
        return Err(EngineError::python(
            "TypeError",
            format!("'{}' object is not a mapping", value.python_type()),
        ));
    }
    Ok(value.clone())
}

pub fn validate_lifecycle_integrity(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    policy: &Value,
    task: &Value,
    provenance: &dyn CheckProvenancePort,
    windows: bool,
) -> EngineResult<()> {
    let (activation_row, activation) = evidence_activation(ledger, run)?;
    let run_id = item(run, "run_id")?;
    let activations = ledger.all(GraphRead::EvidenceActivationEvents, &[run_id.clone()])?;
    if activations.len() != 1 {
        return Err(invalid("EVIDENCE_ACTIVATION_INVALID"));
    }
    let request = compat::loads_value(item(&activations[0], "detail_json")?)?;
    for key in ["coverage_manifest_digest", "checks", "roots"] {
        if !get(&request, key)?.py_eq(get(&activation, key)?) {
            return Err(invalid("EVIDENCE_ACTIVATION_INVALID"));
        }
    }
    if !get(&activation, "actor")?.py_eq(&Value::String(provenance.actor()?))
        || !get(&activation, "host_identity")?.is_text(&provenance.host_identity()?)
        || !equal_sets(
            item(&activation, "checks")?,
            item(task, "required_check_ids")?,
        )?
        || !matches!(get(&activation, "complete")?, Value::Bool(true))
    {
        return Err(invalid("EVIDENCE_ACTIVATION_INVALID"));
    }
    let reservations = reservation_states(ledger, run_id)?;
    let mut slots: Vec<Vec<Value>> = Vec::new();
    let mut pending = Vec::new();
    let mut consumed = vec![
        ("delivery_repairs".into(), 0_i64.into()),
        ("design_revisions".into(), 0_i64.into()),
    ];
    let mut selected_receipts = Vec::new();
    let mut finished_receipts = Vec::new();
    ledger.visit(
        GraphRead::EvidenceOperationEvents,
        &[run_id.clone()],
        &mut |event| {
            let request = compat::loads_value(item(&event, "detail_json")?)?;
            let response = compat::loads_value(item(&event, "response_json")?)?;
            let event_type = item(&event, "event_type")?;
            if in_strings(
                event_type,
                &[
                    "evidence.enable",
                    "check.start",
                    "check.finish",
                    "check.abandon",
                ],
            )? && !item(&event, "request_digest")?
                .is_text(&ids::sha256_bytes(&ids::canonical_bytes(&request)?))
            {
                return Err(invalid("CHECK_RESERVATION_INVALID"));
            }
            if event_type.is_text("check.start") {
                let reservation = reservations.get(get(&response, "reservation_id")?)?;
                let Some(reservation) = reservation else {
                    return Err(invalid("CHECK_RESERVATION_INVALID"));
                };
                if !item(reservation, "request_digest")?.py_eq(item(&event, "request_digest")?) {
                    return Err(invalid("CHECK_RESERVATION_INVALID"));
                }
                for key in [
                    "check_id",
                    "source_generation",
                    "predecessor_ref",
                    "coverage",
                    "activation_ref",
                ] {
                    if !item(reservation, key)?.py_eq(item(&request, key)?) {
                        return Err(invalid("CHECK_RESERVATION_INVALID"));
                    }
                }
                for key in ["executor", "source"] {
                    let fence = item(reservation, key)?;
                    let mut unbound = spread(fence)?;
                    unbound.set("result_digest", Value::Null);
                    if !unbound.py_eq(item(&request, key)?) {
                        return Err(invalid("CHECK_RESERVATION_INVALID"));
                    }
                    let historical = ledger.first(
                        GraphRead::AttemptClaimDigest,
                        &[
                            run_id.clone(),
                            item(fence, "branch_id")?.clone(),
                            item(fence, "attempt_id")?.clone(),
                        ],
                    )?;
                    if historical.is_none()
                        || !item(historical.as_ref().unwrap(), "claim_digest")?
                            .py_eq(item(fence, "claim_digest")?)
                    {
                        return Err(invalid("CHECK_RESERVATION_INVALID"));
                    }
                }
                let executor = item(reservation, "executor")?;
                let check = item(reservation, "check_id")?;
                let slot = vec![
                    item(executor, "branch_id")?.clone(),
                    item(executor, "attempt_id")?.clone(),
                    check.clone(),
                ];
                for key in &slot {
                    errors::require_hashable(key)?;
                }
                let repeated = slots
                    .iter()
                    .any(|previous| previous.iter().zip(&slot).all(|(a, b)| a.container_eq(b)));
                if !item(reservation, "repeat_cost")?.py_eq(&Value::from(i64::from(repeated))) {
                    return Err(invalid("CHECK_RESERVATION_INVALID"));
                }
                errors::require_hashable(check)?;
                if errors::set_contains(&pending, check) {
                    return Err(invalid("CHECK_RESERVATION_INVALID"));
                }
                if !repeated {
                    slots.push(slot);
                }
                insert_set(&mut pending, check)?;
                add_consumed(
                    &mut consumed,
                    item(reservation, "budget_id")?,
                    item(reservation, "repeat_cost")?,
                )?;
            } else if in_strings(event_type, &["check.finish", "check.abandon"])? {
                let identifier = item(&request, "reservation_id")?;
                let Some(reservation) = reservations.get(identifier)? else {
                    return Err(key_error(identifier)?);
                };
                let check = item(reservation, "check_id")?;
                errors::require_hashable(check)?;
                let Some(index) = pending.iter().position(|key| key.container_eq(check)) else {
                    return Err(key_error(check)?);
                };
                pending.remove(index);
                if event_type.is_text("check.finish") {
                    let reference = item(&response, "artifact_ref")?;
                    let row = ledger.first(GraphRead::Artifact, &[reference.clone()])?;
                    let Some(row) = row else {
                        return Err(invalid("CHECK_RESERVATION_INVALID"));
                    };
                    if !item(&row, "sha256")?.py_eq(item(&request, "receipt_digest")?) {
                        return Err(invalid("CHECK_RESERVATION_INVALID"));
                    }
                    let receipt = compat::loads_value(item(&row, "content_json")?)?;
                    validate_receipt_integrity(
                        &receipt,
                        run_id,
                        check,
                        configured_check(policy, check)?,
                        provenance,
                    )?;
                    for key in [
                        "executor",
                        "source",
                        "source_generation",
                        "coverage",
                        "predecessor_ref",
                        "before_source_digest",
                        "review_source_binding_ref",
                    ] {
                        if !item(&receipt, key)?.py_eq(item(reservation, key)?) {
                            return Err(invalid("CHECK_RESERVATION_INVALID"));
                        }
                    }
                    if !item(&receipt, "reservation_id")?.py_eq(identifier)
                        || !item(&receipt, "task_digest")?.py_eq(item(run, "task_digest")?)
                        || !item(&receipt, "policy_digest")?.py_eq(item(run, "policy_digest")?)
                        || !item(&receipt, "plan_digest")?.py_eq(item(&activation, "plan_digest")?)
                        || !item(&receipt, "activation_ref")?.py_eq(item(&activation_row, "ref")?)
                    {
                        return Err(invalid("CHECK_RESERVATION_INVALID"));
                    }
                    insert_mapping(&mut selected_receipts, check, reference)?;
                    insert_set(&mut finished_receipts, reference)?;
                }
            } else if event_type.is_text("join.advance") {
                let outcome = get(&response, "outcome")?;
                if outcome.is_text("REPAIR") {
                    add_consumed(&mut consumed, &"delivery_repairs".into(), &1_i64.into())?;
                } else if in_strings(outcome, &["REVISE", "REDESIGN"])? {
                    add_consumed(&mut consumed, &"design_revisions".into(), &1_i64.into())?;
                }
            } else if event_type.is_text("record.budget-use") {
                let budget = item(&request, "budget_id")?;
                if mapping(&consumed, budget)?.is_some() {
                    add_consumed(&mut consumed, budget, item(&request, "amount")?)?;
                }
            }
            Ok(())
        },
    )?;
    ledger.visit(
        GraphRead::BudgetsForValidation,
        &[run_id.clone()],
        &mut |budget| {
            if let Some(amount) = mapping(&consumed, item(&budget, "budget_id")?)? {
                if !item(&budget, "used")?.py_eq(amount) {
                    return Err(invalid("BUDGET_STATE_INVALID"));
                }
            }
            Ok(())
        },
    )?;
    for (check, reference) in selected_receipts {
        let selected = ledger.first(GraphRead::SelectedCheckArtifact, &[run_id.clone(), check])?;
        if selected.is_none()
            || !item(selected.as_ref().unwrap(), "artifact_ref")?.py_eq(&reference)
        {
            return Err(invalid("CHECK_PREDECESSOR_CONFLICT"));
        }
    }
    for (row, receipt) in lifecycle_artifacts(ledger, run_id, &"check_receipt".into())? {
        let check = item(&receipt, "check_id")?;
        validate_receipt_integrity(
            &receipt,
            run_id,
            check,
            configured_check(policy, check)?,
            provenance,
        )?;
        if item(&receipt, "schema_version")?.py_eq(&2_i64.into()) {
            let reference = item(&row, "ref")?;
            errors::require_hashable(reference)?;
            if !errors::set_contains(&finished_receipts, reference) {
                return Err(invalid("CHECK_RESERVATION_INVALID"));
            }
        }
    }
    validate_review_binding_history(ledger, run, &activation_row, &activation)?;
    validate_repair_and_acceptance_history(ledger, run, task, &activation, windows)
}

fn validate_review_binding_history(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    activation_row: &Value,
    activation: &Value,
) -> EngineResult<()> {
    let run_id = item(run, "run_id")?;
    for (row, binding) in lifecycle_artifacts(ledger, run_id, &"review_source_binding".into())? {
        let fence = item(&binding, "source")?;
        let source = ledger.first(
            GraphRead::FenceNode,
            &[run_id.clone(), item(fence, "branch_id")?.clone()],
        )?;
        let dependency_id = item(&binding, "dependency_join_id")?;
        let dependency = ledger.first(
            GraphRead::SealedDeliveryDependency,
            &[run_id.clone(), dependency_id.clone()],
        )?;
        let (Some(source), Some(dependency)) = (source, dependency) else {
            return Err(invalid("REVIEW_BINDING_MISMATCH"));
        };
        if !item(&source, "result_digest")?.py_eq(item(fence, "result_digest")?)
            || !item(&dependency, "generation")?.py_eq(item(&binding, "source_generation")?)
            || ledger
                .first(
                    GraphRead::JoinHasMember,
                    &[dependency_id.clone(), item(&source, "branch_id")?.clone()],
                )?
                .is_none()
            || !item(
                &compat::loads_value(item(&source, "envelope_json")?)?,
                "attempt_id",
            )?
            .py_eq(item(fence, "attempt_id")?)
            || !item(
                &compat::loads_value(item(&source, "envelope_json")?)?,
                "claim_digest",
            )?
            .py_eq(item(fence, "claim_digest")?)
            || !item(&binding, "task_digest")?.py_eq(item(run, "task_digest")?)
            || !item(&binding, "policy_digest")?.py_eq(item(run, "policy_digest")?)
            || !item(&binding, "plan_digest")?.py_eq(item(activation, "plan_digest")?)
            || !item(item(&binding, "coverage")?, "roots")?.py_eq(item(activation, "roots")?)
            || !item(&source, "generation")?.py_eq(item(&binding, "source_generation")?)
            || !item(&source, "node_key")?.is_text("senior_engineer")
            || !item(&binding, "coverage_digest")?.is_text(&ids::sha256_bytes(
                &ids::canonical_bytes(item(&binding, "coverage")?)?,
            ))
            || !item(&binding, "outputs_digest")?.is_text(&ids::sha256_bytes(
                &ids::canonical_bytes(item(item(&binding, "coverage")?, "outputs")?)?,
            ))
            || !item(item(&binding, "coverage")?, "activation_ref")?
                .py_eq(item(activation_row, "ref")?)
        {
            return Err(invalid("REVIEW_BINDING_MISMATCH"));
        }
        let mut requests = Vec::new();
        ledger.visit(
            GraphRead::JoinAdvanceRequests,
            &[run_id.clone()],
            &mut |event| {
                requests.push(compat::loads_value(item(&event, "detail_json")?)?);
                Ok(())
            },
        )?;
        let mut origin = None;
        for request in &requests {
            if get(request, "join_id")?.py_eq(dependency_id) {
                origin = Some(request);
                break;
            }
        }
        let declared = if let Some(origin) = origin.filter(|origin| origin.truthy()) {
            let empty = Value::List(Vec::new());
            get(origin, "generated_outputs")?;
            let mut declared = Vec::new();
            for output in
                errors::iterable_values(origin.get("generated_outputs").unwrap_or(&empty))?
            {
                let mut output = spread(&output)?;
                output.set(
                    "path",
                    Value::String(contracts::lexical_relative(item(&output, "path")?, "ref")?),
                );
                declared.push(output);
            }
            python_operations::sort_by_less(&mut declared, |a, b| {
                python_operations::less(&a["path"], &b["path"])
            })?;
            Value::List(declared)
        } else {
            Value::Null
        };
        if !declared.py_eq(item(item(&binding, "coverage")?, "outputs")?) {
            return Err(invalid("REVIEW_BINDING_MISMATCH"));
        }
        ledger.visit(
            GraphRead::DeliveryReviewEnvelopes,
            &[run_id.clone(), item(&binding, "source_generation")?.clone()],
            &mut |member| {
                let envelope = compat::loads_value(item(&member, "envelope_json")?)?;
                let mut references = Vec::new();
                for input in errors::iterable_values(item(&envelope, "inputs")?)? {
                    insert_set(&mut references, item(&input, "ref")?)?;
                }
                let reference = item(&row, "ref")?;
                errors::require_hashable(reference)?;
                if !errors::set_contains(&references, reference) {
                    return Err(invalid("REVIEW_BINDING_MISMATCH"));
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}

pub fn validate_repair_and_acceptance_history(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    task: &Value,
    activation: &Value,
    windows: bool,
) -> EngineResult<()> {
    let run_id = item(run, "run_id")?;
    for (_, packet) in lifecycle_artifacts(ledger, run_id, &"repair_packet".into())? {
        let branch = ledger.first(
            GraphRead::FenceNode,
            &[run_id.clone(), item(&packet, "branch_id")?.clone()],
        )?;
        let Some(branch) = branch else {
            return Err(invalid("REPAIR_PACKET_INVALID"));
        };
        if !item(&packet, "task_digest")?.py_eq(item(run, "task_digest")?)
            || !item(&packet, "plan_digest")?.py_eq(item(activation, "plan_digest")?)
            || !item(&packet, "run_id")?.py_eq(run_id)
            || !item(&packet, "generation")?.py_eq(item(&branch, "generation")?)
            || (!get(&packet, "origin_kind")?.is_text("delivery_review")
                && !item(&packet, "origin_gate")?.py_eq(item(&branch, "node_key")?))
        {
            return Err(invalid("REPAIR_PACKET_INVALID"));
        }
        let failures = errors::iterable_set(item(&packet, "failed_criterion_ids")?)?;
        let acceptance = errors::iterable_set(item(task, "acceptance_ids")?)?;
        if failures
            .iter()
            .any(|key| !errors::set_contains(&acceptance, key))
        {
            return Err(invalid("REPAIR_PACKET_INVALID"));
        }
        if get(&packet, "origin_kind")?.is_text("delivery_review") {
            validate_delivery_repair_origin(ledger, run, &packet, &branch)?;
        }
        let envelope = compat::loads_value(item(&branch, "envelope_json")?)?;
        let allowed = item(&envelope, "effect_capabilities")?;
        for capability in errors::iterable_values(item(&packet, "authorized_scope")?)? {
            let mut authorized = false;
            for candidate in errors::iterable_values(allowed)? {
                let effect = item(&candidate, "effect")?;
                let effect = effect
                    .as_string()
                    .ok_or_else(|| errors::attribute_error(effect, "startswith"))?;
                if !effect.starts_with("filesystem") {
                    continue;
                }
                if item(&capability, "effect")?.py_eq(item(&candidate, "effect")?)
                    && item(&capability, "action")?.py_eq(item(&candidate, "action")?)
                {
                    let root = item(&candidate, "target_ref")?;
                    let path = item(&capability, "target_ref")?;
                    let root = root
                        .as_string()
                        .ok_or_else(|| errors::attribute_error(root, "rstrip"))?;
                    let path = path
                        .as_string()
                        .ok_or_else(|| errors::attribute_error(path, "rstrip"))?;
                    if source_paths::contains(root, path, windows) {
                        authorized = true;
                        break;
                    }
                }
            }
            if !authorized {
                return Err(invalid("REPAIR_PACKET_INVALID"));
            }
        }
        if item(&packet, "failure_result_ref")?.truthy() {
            let result = ledger.first(
                GraphRead::ArtifactInRunContent,
                &[run_id.clone(), item(&packet, "failure_result_ref")?.clone()],
            )?;
            let failure = match result {
                Some(row) => compat::loads_value(item(&row, "content_json")?)?,
                None => Value::object([]),
            };
            if !get(&failure, "branch_id")?.py_eq(item(&branch, "branch_id")?)
                || !(in_strings(get(&failure, "status")?, &["failed", "timed_out"])?
                    || get(&failure, "kind")?.is_text("timeout"))
                || !get(&failure, "attempt_id")?.py_eq(item(&packet, "attempt_id")?)
            {
                return Err(invalid("REPAIR_PACKET_INVALID"));
            }
        }
        if item(&packet, "attempt_id")?.truthy()
            && ledger
                .first(
                    GraphRead::AttemptExists,
                    &[
                        run_id.clone(),
                        item(&branch, "branch_id")?.clone(),
                        item(&packet, "attempt_id")?.clone(),
                    ],
                )?
                .is_none()
        {
            return Err(invalid("REPAIR_PACKET_INVALID"));
        }
    }
    let mut wrappers = Vec::new();
    for (row, wrapper) in lifecycle_artifacts(ledger, run_id, &"acceptance_binding".into())? {
        insert_mapping(&mut wrappers, item(&row, "ref")?, &wrapper)?;
    }
    for (_, wrapper) in &wrappers {
        if !item(wrapper, "run_id")?.py_eq(run_id)
            || !item(wrapper, "task_digest")?.py_eq(item(run, "task_digest")?)
            || !item(wrapper, "plan_digest")?.py_eq(item(activation, "plan_digest")?)
            || !crate::domain::contract_values::contains(
                item(task, "acceptance_ids")?,
                item(wrapper, "criterion_id")?,
            )?
            || !equal_sets(item(wrapper, "checks")?, item(task, "required_check_ids")?)?
        {
            return Err(invalid("ACCEPTANCE_BINDING_MISMATCH"));
        }
        let evidence = item(wrapper, "underlying_evidence")?;
        if ledger
            .first(
                GraphRead::UnderlyingAcceptanceArtifact,
                &[
                    run_id.clone(),
                    item(evidence, "ref")?.clone(),
                    item(evidence, "sha256")?.clone(),
                ],
            )?
            .is_none()
        {
            return Err(invalid("ACCEPTANCE_BINDING_MISMATCH"));
        }
        let checks = item(wrapper, "checks")?;
        let checks = checks
            .as_object()
            .ok_or_else(|| errors::attribute_error(checks, "items"))?;
        for (check, reference) in checks {
            let row = ledger.first(
                GraphRead::ArtifactInRunContent,
                &[run_id.clone(), reference.clone()],
            )?;
            let receipt = match row {
                Some(row) => compat::loads_value(item(&row, "content_json")?)?,
                None => Value::object([]),
            };
            if !get(&receipt, "check_id")?.py_eq(&Value::String(check.clone()))
                || !get(&receipt, "outcome")?.is_text("PASS")
                || !get(&receipt, "source")?.py_eq(item(wrapper, "source")?)
                || !get(&receipt, "after_source_digest")?.py_eq(item(wrapper, "source_digest")?)
                || !get(&receipt, "coverage_digest")?.py_eq(item(wrapper, "coverage_digest")?)
                || !get(&receipt, "review_source_binding_ref")?
                    .py_eq(item(wrapper, "review_source_binding_ref")?)
            {
                return Err(invalid("ACCEPTANCE_BINDING_MISMATCH"));
            }
        }
        let predecessor = item(wrapper, "predecessor_ref")?;
        if let Some(previous) = mapping(&wrappers, predecessor)? {
            if !item(previous, "criterion_id")?.py_eq(item(wrapper, "criterion_id")?) {
                return Err(invalid("ACCEPTANCE_PREDECESSOR_CONFLICT"));
            }
        }
    }
    Ok(())
}

fn validate_delivery_repair_origin(
    ledger: &dyn GraphLedgerReadPort,
    run: &Value,
    packet: &Value,
    branch: &Value,
) -> EngineResult<()> {
    let run_id = item(run, "run_id")?;
    let collection = ledger.first(
        GraphRead::RepairOriginCollection,
        &[
            run_id.clone(),
            item(packet, "origin_collection_id")?.clone(),
        ],
    )?;
    let reference = item(packet, "origin_consolidation_ref")?;
    let consolidation = ledger.first(
        GraphRead::ArtifactInRunDigest,
        &[
            run_id.clone(),
            item(reference, "ref")?.clone(),
            item(reference, "sha256")?.clone(),
        ],
    )?;
    let result = match consolidation {
        Some(row) => compat::loads_value(item(&row, "content_json")?)?,
        None => Value::object([]),
    };
    let Some(collection) = collection else {
        return Err(invalid("REPAIR_PACKET_INVALID"));
    };
    if !python_operations::add(item(&collection, "generation")?, &1_i64.into())?
        .py_eq(item(branch, "generation")?)
        || !item(branch, "node_key")?.is_text("senior_engineer")
        || !item(packet, "origin_gate")?.is_text("supervisor_delivery_consolidation")
        || !get(&result, "outcome")?.is_text("REPAIR")
        || !get(&result, "generation")?.py_eq(item(&collection, "generation")?)
    {
        return Err(invalid("REPAIR_PACKET_INVALID"));
    }
    let mut members = Vec::new();
    ledger.visit(
        GraphRead::JoinMemberBranches,
        &[item(&collection, "join_id")?.clone()],
        &mut |member| insert_set(&mut members, item(&member, "branch_id")?),
    )?;
    let reports = errors::iterable_values(item(packet, "origin_reports")?)?;
    let mut origins = Vec::new();
    for origin in &reports {
        insert_set(&mut origins, item(origin, "branch_id")?)?;
    }
    if origins.len() != members.len()
        || origins
            .iter()
            .any(|key| !errors::set_contains(&members, key))
    {
        return Err(invalid("REPAIR_PACKET_INVALID"));
    }
    for origin in reports {
        let member = ledger.first(GraphRead::Node, &[item(&origin, "branch_id")?.clone()])?;
        let member = member.as_ref().unwrap_or(&compat::value::NULL);
        let envelope = compat::loads_value(item(member, "envelope_json")?)?;
        let result = compat::loads_value(item(member, "result_json")?)?;
        get(&result, "findings")?;
        let empty = Value::List(Vec::new());
        let findings = errors::iterable_values(result.get("findings").unwrap_or(&empty))?;
        if !item(&origin, "attempt_id")?.py_eq(item(&envelope, "attempt_id")?)
            || !item(item(&origin, "result_ref")?, "sha256")?.py_eq(item(member, "result_digest")?)
            || !item(&origin, "artifact_ref")?.py_eq(item(&envelope, "artifact_ref")?)
            || !item(&origin, "evidence")?.py_eq(item(&envelope, "evidence")?)
        {
            return Err(invalid("REPAIR_PACKET_INVALID"));
        }
        let mut actionable = Vec::new();
        for finding in &findings {
            if is_actionable_finding(finding)? {
                actionable.push(finding.clone());
            }
        }
        if !item(&origin, "findings")?.py_eq(&Value::List(actionable)) {
            return Err(invalid("REPAIR_PACKET_INVALID"));
        }
        let mut observations = Vec::new();
        for finding in &findings {
            if !is_actionable_finding(finding)? {
                observations.push(finding.clone());
            }
        }
        get(&origin, "observations")?;
        if !origin
            .get("observations")
            .unwrap_or(&empty)
            .py_eq(&Value::List(observations))
        {
            return Err(invalid("REPAIR_PACKET_INVALID"));
        }
    }
    Ok(())
}
