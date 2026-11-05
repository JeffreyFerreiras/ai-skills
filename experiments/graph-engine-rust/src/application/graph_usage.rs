use super::{
    error::{EngineResult, TaskValidationMutation},
    persistent_store::PersistentSession,
    policy,
    semantic_state::{mutation_task_validator, SemanticContext},
    state::{mutate_persistent, RunRecord},
    usage_record::{self, UsageArguments},
    usage_source::UsageSourcePort,
};
use crate::{compat::Value, domain::errors};

fn item<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    Ok(errors::string_item(value, key)?)
}

pub fn record(
    session: &dyn PersistentSession,
    run_id: &str,
    run: &RunRecord,
    arguments: &Value,
    context: &SemanticContext<'_>,
    files: &dyn UsageSourcePort,
) -> EngineResult<Value> {
    // These source checks precede even conversion of the supplied OS path.
    let validated = usage_record::prelude(
        item(arguments, "op_id")?,
        [
            item(arguments, "start_offset")?,
            item(arguments, "source_id")?,
            item(arguments, "prefix_sha256")?,
        ],
    )?;
    let path = policy::path_from_value(context.files, item(arguments, "session_log")?)?;
    let prepared = usage_record::prepare_after_prelude(
        UsageArguments {
            operation_id: item(arguments, "op_id")?,
            action: item(arguments, "action")?,
            session_log: &path,
            start_offset: item(arguments, "start_offset")?,
            source_id: item(arguments, "source_id")?,
            prefix_sha256: item(arguments, "prefix_sha256")?,
            binding_id: item(arguments, "binding_id")?,
            branch_id: item(arguments, "branch_id")?,
            attempt_id: item(arguments, "attempt_id")?,
            phase: item(arguments, "phase")?,
            generation: item(arguments, "generation")?,
        },
        session,
        run_id,
        run,
        files,
        validated,
    )?;
    mutate_persistent(
        session,
        run_id,
        &prepared.operation_id,
        &prepared.request,
        |session, current, revision| prepared.apply(session, current, revision),
        mutation_task_validator(context, TaskValidationMutation::Usage),
        context.store.fault,
    )
}
