#[path = "workload_binaries.rs"]
mod workload_binaries;

use graph_engine::domain::ids::sha256_bytes;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    time::Instant,
};
use workload_binaries::{capture, BinaryManifest, ProcessBoundary};

const COVERAGE: &str = include_str!("../../parity/coverage-map.json");
const HYGIENE: &str = "tests.test_standalone_acceptance.StandaloneAcceptanceTests.test_hygiene";
const TEST_BINARIES: &str = "GRAPH_RUST_TEST_BINARIES";

#[derive(Deserialize)]
struct Coverage {
    test_identities: BTreeMap<String, CoverageCase>,
}

#[derive(Deserialize)]
struct CoverageCase {
    native_dispatch: Option<Dispatch>,
}

#[derive(Deserialize)]
struct Dispatch {
    target: String,
    selector: String,
}

enum Case {
    Inline(fn()),
    Libtest(Dispatch),
}

type Registry = BTreeMap<String, Case>;

struct Selected<'a> {
    identity: &'a str,
    case: &'a Case,
}

#[derive(Serialize)]
struct Observed {
    selector: String,
    outcome: String,
}

#[derive(Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum Boundary {
    Inline {
        identity: String,
        passed: bool,
        elapsed_ns: u128,
    },
    Libtest {
        target: String,
        identities: Vec<String>,
        observed: Vec<Observed>,
        process: ProcessBoundary,
        exact_order: bool,
    },
}

#[derive(Serialize)]
struct RunReport {
    schema_version: u8,
    registry_sha256: String,
    test_binaries_sha256: Option<String>,
    identities: Vec<String>,
    boundaries: Vec<Boundary>,
    passed: bool,
}

fn registry(inline: Vec<(String, fn())>) -> Result<Registry, String> {
    let coverage: Coverage =
        serde_json::from_str(COVERAGE).map_err(|error| format!("invalid coverage map: {error}"))?;
    let mut registered = Registry::new();
    for (identity, run) in inline {
        if registered
            .insert(identity.clone(), Case::Inline(run))
            .is_some()
        {
            return Err(format!("duplicate inline identity {identity}"));
        }
    }
    let mut selectors = BTreeSet::new();
    for (identity, record) in &coverage.test_identities {
        match (identity.as_str(), &record.native_dispatch) {
            (HYGIENE, None) => (),
            (_, None) if registered.contains_key(identity) => (),
            (_, Some(dispatch)) if identity != HYGIENE => {
                if registered.contains_key(identity)
                    || !selectors.insert((dispatch.target.as_str(), dispatch.selector.as_str()))
                {
                    return Err(format!("duplicate native dispatch for {identity}"));
                }
            }
            _ => return Err(format!("missing or invalid native dispatch for {identity}")),
        }
    }
    for (identity, record) in coverage.test_identities {
        if let Some(dispatch) = record.native_dispatch {
            registered.insert(identity, Case::Libtest(dispatch));
        }
    }
    if registered.len() != 359 || registered.contains_key(HYGIENE) {
        return Err("native registry must contain 359 behavior identities".into());
    }
    Ok(registered)
}

fn registry_sha256() -> String {
    sha256_bytes(COVERAGE.as_bytes())
}

fn select<'a>(registry: &'a Registry, identities: &[&str]) -> Result<Vec<Selected<'a>>, String> {
    if identities.is_empty() {
        return Err("native selection is empty".into());
    }
    let mut seen = BTreeSet::new();
    identities
        .iter()
        .map(|identity| {
            if !seen.insert(*identity) {
                return Err(format!("duplicate selected native identity {identity}"));
            }
            let (identity, case) = registry
                .get_key_value(*identity)
                .ok_or_else(|| format!("unknown native test identity {identity}"))?;
            Ok(Selected { identity, case })
        })
        .collect()
}

fn same_phase(left: &str, right: &str) -> bool {
    let daily = |id: &str| id.starts_with("tests.test_daily_workflow.");
    daily(left) == daily(right)
}

fn can_batch(left: &Selected<'_>, right: &Selected<'_>) -> bool {
    match (left.case, right.case) {
        (Case::Libtest(before), Case::Libtest(after)) => {
            before.target == after.target
                && before.selector < after.selector
                && same_phase(left.identity, right.identity)
        }
        _ => false,
    }
}

fn test_binaries(selected: &[Selected<'_>]) -> Result<Option<(BinaryManifest, String)>, String> {
    let targets: BTreeSet<_> = selected
        .iter()
        .filter_map(|selected| match selected.case {
            Case::Inline(_) => None,
            Case::Libtest(dispatch) => Some(dispatch.target.as_str()),
        })
        .collect();
    if targets.is_empty() {
        return Ok(None);
    }
    let path = std::env::var_os(TEST_BINARIES)
        .ok_or_else(|| format!("{TEST_BINARIES} is required for selected libtest cases"))?;
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {TEST_BINARIES}: {error}"))?;
    let manifest: BinaryManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid {TEST_BINARIES} manifest: {error}"))?;
    if manifest.registry_sha256 != registry_sha256() {
        return Err("test executable manifest belongs to a different registry".into());
    }
    for target in targets {
        let binary = manifest
            .binaries
            .get(target)
            .ok_or_else(|| format!("missing test executable for {target}"))?;
        if !binary.path.is_absolute() || !binary.path.is_file() {
            return Err(format!("invalid test executable for {target}"));
        }
    }
    Ok(Some((manifest, sha256_bytes(&bytes))))
}

fn observed_tests(stdout: &[u8]) -> Vec<Observed> {
    String::from_utf8_lossy(stdout)
        .lines()
        .skip_while(|line| !line.starts_with("running "))
        .skip(1)
        .take_while(|line| {
            !matches!(*line, "successes:" | "failures:") && !line.starts_with("test result:")
        })
        .filter_map(|line| {
            let (selector, outcome) = line.strip_prefix("test ")?.split_once(" ... ")?;
            Some(Observed {
                selector: selector.into(),
                outcome: outcome.into(),
            })
        })
        .collect()
}

fn emit_result(identity: &str, passed: bool) {
    if passed {
        println!("PASS {identity}");
    } else {
        eprintln!("FAIL {identity}");
    }
}

fn run_libtest(
    target: &str,
    selected: &[Selected<'_>],
    manifest: &BinaryManifest,
) -> Result<(Boundary, bool), String> {
    let selectors: Vec<_> = selected
        .iter()
        .map(|selected| {
            let Case::Libtest(dispatch) = selected.case else {
                unreachable!()
            };
            dispatch.selector.as_str()
        })
        .collect();
    let mut arguments = vec!["--exact", "--test-threads", "1", "--show-output"];
    arguments.extend(&selectors);
    let (output, process) = capture(&manifest.binaries[target].path, &arguments)?;
    io::stdout()
        .write_all(&output.stdout)
        .map_err(|error| error.to_string())?;
    io::stderr()
        .write_all(&output.stderr)
        .map_err(|error| error.to_string())?;
    let observed = observed_tests(&output.stdout);
    let exact_order = observed
        .iter()
        .map(|test| test.selector.as_str())
        .eq(selectors);
    let all_passed = observed.iter().all(|test| test.outcome == "ok");
    let valid_exit = output.status.success() == all_passed;
    let passed = exact_order && valid_exit && all_passed;
    for (index, selected) in selected.iter().enumerate() {
        emit_result(
            selected.identity,
            exact_order && valid_exit && observed[index].outcome == "ok",
        );
    }
    if !exact_order {
        eprintln!("libtest returned an unexpected selection or order for {target}");
    }
    Ok((
        Boundary::Libtest {
            target: target.into(),
            identities: selected
                .iter()
                .map(|selected| selected.identity.into())
                .collect(),
            observed,
            process,
            exact_order,
        },
        passed,
    ))
}

fn create_report(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("could not create runner report: {error}"))
}

fn execute(selected: Vec<Selected<'_>>, report_path: Option<&Path>) -> Result<bool, String> {
    let test_binaries = test_binaries(&selected)?;
    let mut report_file = report_path.map(create_report).transpose()?;
    let mut report = RunReport {
        schema_version: 1,
        registry_sha256: registry_sha256(),
        test_binaries_sha256: test_binaries.as_ref().map(|(_, sha256)| sha256.clone()),
        identities: selected
            .iter()
            .map(|selected| selected.identity.into())
            .collect(),
        boundaries: Vec::new(),
        passed: true,
    };
    let mut remaining = selected.as_slice();
    while let Some(first) = remaining.first() {
        let (boundary, passed, consumed) = match first.case {
            Case::Inline(run) => {
                let start = Instant::now();
                let passed = std::panic::catch_unwind(run).is_ok();
                let elapsed_ns = start.elapsed().as_nanos();
                emit_result(first.identity, passed);
                (
                    Boundary::Inline {
                        identity: first.identity.into(),
                        passed,
                        elapsed_ns,
                    },
                    passed,
                    1,
                )
            }
            Case::Libtest(dispatch) => {
                let count = 1 + remaining
                    .windows(2)
                    .take_while(|pair| can_batch(&pair[0], &pair[1]))
                    .count();
                let (manifest, _) = test_binaries
                    .as_ref()
                    .expect("libtest manifest was preflighted");
                let (boundary, passed) =
                    run_libtest(&dispatch.target, &remaining[..count], manifest)?;
                (boundary, passed, count)
            }
        };
        report.boundaries.push(boundary);
        report.passed &= passed;
        remaining = &remaining[consumed..];
    }
    if let Some(file) = report_file.as_mut() {
        serde_json::to_writer_pretty(&mut *file, &report).map_err(|error| error.to_string())?;
        writeln!(file).map_err(|error| error.to_string())?;
    }
    Ok(report.passed)
}

pub(super) fn run(inline: Vec<(String, fn())>, arguments: &[String]) -> Result<bool, String> {
    let registry = registry(inline)?;
    match arguments {
        [mode] if mode == "--list" => {
            for identity in registry.keys() {
                println!("{identity}");
            }
        }
        [mode, identity] if mode == "--identity" => {
            return execute(select(&registry, &[identity])?, None);
        }
        [mode, cargo_log] if mode == "--discover-tests" => {
            let mut required: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
            for case in registry.values() {
                if let Case::Libtest(dispatch) = case {
                    required.entry(&dispatch.target).or_default().insert(&dispatch.selector);
                }
            }
            workload_binaries::discover(Path::new(cargo_log), &required, registry_sha256())?;
        }
        [mode, path, rest @ ..] if mode == "--selection" => {
            let report = match rest {
                [] => None,
                [flag, path] if flag == "--report" => Some(Path::new(path)),
                _ => return Err("expected --selection <file> [--report <file>]".into()),
            };
            let input = fs::read_to_string(path)
                .map_err(|error| format!("could not read native selection: {error}"))?;
            let identities = input.lines().collect::<Vec<_>>();
            return execute(select(&registry, &identities)?, report);
        }
        _ => return Err("graph-suite requires --list, --identity <id>, --selection <file> [--report <file>], or --discover-tests <cargo-jsonl>".into()),
    }
    Ok(true)
}
