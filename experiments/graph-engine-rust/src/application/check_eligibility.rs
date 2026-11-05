use super::error::{EngineError, EngineResult};
use crate::{compat::Value, domain::errors};

pub trait CheckEligibilityPort {
    fn verify_fence(&self, fence: &Value, executor: bool) -> EngineResult<()>;
    fn verify_generated_outputs(
        &self,
        outputs: &Value,
        registering: Option<&Value>,
    ) -> EngineResult<()>;
    fn capture_source(&self, roots: &Value, outputs: &Value) -> EngineResult<Value>;
    fn current_review_binding(&self) -> EngineResult<Option<(Value, Value)>>;
}
pub type SourceCaptureHook<'a> = dyn FnMut(&Value, &Value) -> EngineResult<Value> + 'a;

fn get<'a>(value: &'a Value, key: &str) -> EngineResult<&'a Value> {
    if value.as_object().is_none() {
        return Err(errors::attribute_error(value, "get").into());
    }
    Ok(value.get(key).unwrap_or(&crate::compat::value::NULL))
}

pub fn receipt_eligibility(
    receipt: &Value,
    checks: &dyn CheckEligibilityPort,
    registering: Option<&Value>,
    mut source_capture: Option<&mut SourceCaptureHook<'_>>,
) -> EngineResult<Value> {
    if !get(receipt, "schema_version")?.py_eq(&2i64.into()) {
        return Ok("legacy_unbound".into());
    }
    let result = (|| {
        checks.verify_fence(errors::string_item(receipt, "source")?, false)?;
        checks.verify_fence(errors::string_item(receipt, "executor")?, true)?;
        let validity = errors::string_item(receipt, "validity")?;
        if !validity.is_text("valid") {
            return Ok(validity.clone());
        }
        let coverage = errors::string_item(receipt, "coverage")?;
        let outputs = errors::string_item(coverage, "outputs")?;
        checks.verify_generated_outputs(outputs, registering)?;
        let roots = errors::string_item(coverage, "roots")?;
        let current = match &mut source_capture {
            Some(capture) => capture(roots, outputs)?,
            None => checks.capture_source(roots, outputs)?,
        };
        let current_digest = errors::string_item(&current, "sha256")?;
        if !current_digest.py_eq(errors::string_item(receipt, "after_source_digest")?) {
            return Ok("source_changed".into());
        }
        if let Some((row, binding)) = checks.current_review_binding()? {
            if !errors::string_item(receipt, "review_source_binding_ref")?
                .py_eq(errors::string_item(&row, "ref")?)
                || !errors::string_item(receipt, "coverage_digest")?
                    .py_eq(errors::string_item(&binding, "coverage_digest")?)
                || !current_digest.py_eq(errors::string_item(&binding, "source_digest")?)
            {
                return Ok("CHECK_REVIEW_BINDING_MISMATCH".into());
            }
        }
        Ok(
            if errors::string_item(receipt, "outcome")?.is_text("PASS") {
                "eligible"
            } else {
                "failed"
            }
            .into(),
        )
    })();
    match result {
        Err(EngineError::State(error)) => Ok(error.to_string().into()),
        Err(EngineError::Contract(error)) => Ok(error.to_string().into()),
        result => result,
    }
}
