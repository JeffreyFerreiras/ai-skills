use graph_engine::{
    application::{
        artifacts::{persist_artifact, ArtifactLedger, ArtifactResolver},
        error::EngineResult,
        policy::PolicyFilePort,
    },
    compat::Value,
    domain::evidence::VerifiedArtifact,
    infrastructure::{policy_files::NativePolicyFiles, sqlite::SqliteSession},
};
struct FinalIdentityFault;
impl graph_engine::application::snapshots::IntegerFileSnapshotPort for FinalIdentityFault {
    fn snapshot_integer(
        &self,
        path: &std::path::Path,
        roots: &[std::path::PathBuf],
        maximum: &num_bigint::BigInt,
    ) -> Result<
        graph_engine::application::snapshots::Snapshot,
        graph_engine::application::snapshots::SnapshotFailure,
    > {
        graph_engine::infrastructure::filesystem::snapshot_integer_final_identity_fault(
            path, roots, maximum,
        )
    }
}

pub fn dispatch(request: &Value) -> EngineResult<Value> {
    let files = NativePolicyFiles {
        environment_override: Some(
            request["environment"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.utf8().unwrap(), value.as_string().unwrap().clone()))
                .collect(),
        ),
    };
    let path = |key: &str| files.from_string(request[key].as_string().unwrap());
    let repo = path("repo")?;
    let skill = path("skill")?;
    let installation = path("installation")?;
    let runtime = request
        .get("runtime")
        .filter(|value| !matches!(value, Value::Null))
        .map(|value| files.from_string(value.as_string().unwrap()))
        .transpose()?;
    let database = request
        .get("database")
        .filter(|value| !matches!(value, Value::Null))
        .map(|value| {
            files
                .from_string(value.as_string().unwrap())
                .and_then(|path| SqliteSession::connect_existing(&path))
        })
        .transpose()?;
    let fault = FinalIdentityFault;
    let resolver = ArtifactResolver {
        files: &files,
        snapshots: if request["fault_final_identity"].is_true() {
            &fault
        } else {
            &files
        },
        repository: &repo,
        skill: &skill,
        installation: &installation,
        policy: &request["policy"],
        runtime: runtime.as_deref(),
        ledger: database.as_ref().map(|value| value as &dyn ArtifactLedger),
    };
    match request["op"].text().as_deref() {
        Some("artifact_resolve") => resolver
            .resolve_reference(&request["reference"], &request["digest"], &request["kind"])
            .map(|artifact| artifact.record()),
        Some("artifact_unhashed") => resolver
            .resolve_unhashed_reference(&request["reference"], &request["kind"])
            .map(|artifact| artifact.record()),
        Some("artifact_reverify") => {
            let mut artifact = request["artifact"].clone();
            if let Some(hex) = request.get("content_hex").and_then(Value::text) {
                artifact.set(
                    "content_json",
                    Value::Bytes(
                        (0..hex.len())
                            .step_by(2)
                            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
                            .collect(),
                    ),
                );
            }
            resolver.reverify_artifact(&artifact).map(|_| Value::Null)
        }
        Some("artifact_persist") => persist_artifact(
            database.as_ref().unwrap(),
            "RUN-1",
            &VerifiedArtifact::from_record(&request["artifact"])?,
        )
        .map(|_| Value::Null),
        _ => unreachable!(),
    }
}
