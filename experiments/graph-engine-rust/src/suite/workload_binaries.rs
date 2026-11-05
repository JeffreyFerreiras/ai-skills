use graph_engine::domain::ids::sha256_bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const AUXILIARY: &[&str] = &["graph-suite", "graphctl", "helper-register"];

#[derive(Deserialize, Serialize)]
pub(super) struct Binary {
    pub(super) path: PathBuf,
    sha256: String,
}

#[derive(Deserialize)]
pub(super) struct BinaryManifest {
    pub(super) registry_sha256: String,
    pub(super) binaries: BTreeMap<String, Binary>,
}

#[derive(Serialize)]
pub(super) struct ProcessBoundary {
    argv: Vec<String>,
    pid: u32,
    exit_code: Option<i32>,
    started_unix_ns: u128,
    finished_unix_ns: u128,
    elapsed_ns: u128,
    stdout_sha256: String,
    stderr_sha256: String,
    stdout_bytes: usize,
    stderr_bytes: usize,
}

#[derive(Serialize)]
struct Listing {
    process: ProcessBoundary,
    stdout: String,
    stderr: String,
}

#[derive(Serialize)]
struct Discovery {
    schema_version: u8,
    registry_sha256: String,
    cargo_log_sha256: String,
    binaries: BTreeMap<String, Binary>,
    auxiliary: BTreeMap<String, Binary>,
    listings: BTreeMap<String, Listing>,
}

pub(super) fn capture(
    binary: &Path,
    arguments: &[&str],
) -> Result<(Output, ProcessBoundary), String> {
    let wall = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    };
    let started_unix_ns = wall();
    let start = Instant::now();
    let child = Command::new(binary)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", binary.display()))?;
    let pid = child.id();
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not collect {}: {error}", binary.display()))?;
    let boundary = ProcessBoundary {
        argv: std::iter::once(binary.to_string_lossy().into_owned())
            .chain(arguments.iter().map(|argument| (*argument).into()))
            .collect(),
        pid,
        exit_code: output.status.code(),
        started_unix_ns,
        finished_unix_ns: wall(),
        elapsed_ns: start.elapsed().as_nanos(),
        stdout_sha256: sha256_bytes(&output.stdout),
        stderr_sha256: sha256_bytes(&output.stderr),
        stdout_bytes: output.stdout.len(),
        stderr_bytes: output.stderr.len(),
    };
    Ok((output, boundary))
}

fn release_path(message: &Value, target_root: &Path, target: &str) -> Result<PathBuf, String> {
    let executable = message["executable"]
        .as_str()
        .ok_or_else(|| format!("no executable for Cargo target {target}"))?;
    let path = PathBuf::from(executable);
    if message["profile"]["opt_level"].as_str() != Some("3")
        || message["profile"]["debug_assertions"].as_bool() != Some(false)
        || !path.starts_with(target_root)
        || !path.is_file()
    {
        return Err(format!(
            "invalid release executable for Cargo target {target}"
        ));
    }
    Ok(path)
}

pub(super) fn discover(
    cargo_log: &Path,
    required: &BTreeMap<&str, BTreeSet<&str>>,
    registry_sha256: String,
) -> Result<(), String> {
    let target_root = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR").ok_or("CARGO_TARGET_DIR is required for discovery")?,
    );
    if !target_root.is_absolute() {
        return Err("CARGO_TARGET_DIR must be absolute".into());
    }
    let raw = fs::read_to_string(cargo_log)
        .map_err(|error| format!("could not read Cargo JSON: {error}"))?;
    let package = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut tests = BTreeMap::new();
    let mut auxiliary_paths = BTreeMap::new();
    let mut built = false;
    for line in raw.lines() {
        let message: Value = serde_json::from_str(line)
            .map_err(|error| format!("invalid Cargo JSON message: {error}"))?;
        match message["reason"].as_str() {
            Some("build-finished") => built = message["success"].as_bool() == Some(true),
            Some("compiler-artifact") if message["executable"].is_string() => {
                let target = &message["target"];
                let Some(source) = target["src_path"].as_str() else {
                    continue;
                };
                if !Path::new(source).starts_with(package) {
                    continue;
                }
                let name = target["name"].as_str().ok_or("Cargo target has no name")?;
                let test = message["profile"]["test"].as_bool() == Some(true);
                let paths = if test && required.contains_key(name) {
                    &mut tests
                } else if !test && AUXILIARY.contains(&name) {
                    &mut auxiliary_paths
                } else {
                    continue;
                };
                let path = release_path(&message, &target_root, name)?;
                if paths.insert(name.to_owned(), path).is_some() {
                    return Err(format!("duplicate executable for Cargo target {name}"));
                }
            }
            _ => (),
        }
    }
    if !built {
        return Err("Cargo did not report a successful build".into());
    }

    let bind = |path: PathBuf| -> Result<Binary, String> {
        let bytes = fs::read(&path)
            .map_err(|error| format!("could not hash {}: {error}", path.display()))?;
        Ok(Binary {
            path,
            sha256: sha256_bytes(&bytes),
        })
    };
    let mut binaries = BTreeMap::new();
    let mut listings = BTreeMap::new();
    for (&target, selectors) in required {
        let path = tests
            .remove(target)
            .ok_or_else(|| format!("missing Cargo test target {target}"))?;
        let (output, process) = capture(&path, &["--list", "--format", "terse"])?;
        let stdout = String::from_utf8(output.stdout)
            .map_err(|error| format!("invalid libtest listing for {target}: {error}"))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(format!("libtest listing failed for {target}: {stderr}"));
        }
        for selector in selectors {
            if stdout
                .lines()
                .filter(|line| line.strip_suffix(": test") == Some(*selector))
                .count()
                != 1
            {
                return Err(format!(
                    "expected exactly one test named {selector} in {target}"
                ));
            }
        }
        listings.insert(
            target.into(),
            Listing {
                process,
                stdout,
                stderr,
            },
        );
        binaries.insert(target.into(), bind(path)?);
    }
    let auxiliary = AUXILIARY
        .iter()
        .map(|name| {
            let path = auxiliary_paths
                .remove(*name)
                .ok_or_else(|| format!("missing auxiliary Cargo executable {name}"))?;
            Ok(((*name).into(), bind(path)?))
        })
        .collect::<Result<_, String>>()?;
    let report = Discovery {
        schema_version: 1,
        registry_sha256,
        cargo_log_sha256: sha256_bytes(raw.as_bytes()),
        binaries,
        auxiliary,
        listings,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    Ok(())
}
