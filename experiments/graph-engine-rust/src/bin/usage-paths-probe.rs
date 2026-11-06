use graph_engine::{
    compat::{self, SerializationProfile, Value},
    domain::{ids, usage::Limits},
    infrastructure::{
        filesystem,
        usage_files::{self, UsageFault, UsageReadStage},
    },
};
use std::{
    cell::RefCell,
    fs, io,
    path::{Path, PathBuf},
};

struct WorkingDirectory(PathBuf);
impl Drop for WorkingDirectory {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

fn main() -> io::Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 2 {
        return Err(io::Error::other("expected reference and output directory"));
    }
    let artifact = fs::canonicalize(
        std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT")
            .ok_or_else(|| io::Error::other("ART required"))?,
    )?;
    let reference = PathBuf::from(&arguments[0]);
    if !fs::canonicalize(&reference)?.starts_with(&artifact) {
        return Err(io::Error::other("reference escaped ART"));
    }
    let output = PathBuf::from(&arguments[1]);
    if !fs::canonicalize(
        output
            .parent()
            .ok_or_else(|| io::Error::other("output parent required"))?,
    )?
    .starts_with(&artifact)
    {
        return Err(io::Error::other("output escaped ART"));
    }
    fs::create_dir(&output)?;
    let reference_bytes = fs::read(&reference)?;
    let report = compat::loads_value(&Value::Bytes(reference_bytes.clone()))
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let root = PathBuf::from(
        report["root"]
            .text()
            .ok_or_else(|| io::Error::other("owned root required"))?,
    );
    let canonical_root = fs::canonicalize(&root)?;
    if !canonical_root.starts_with(&artifact) {
        return Err(io::Error::other("fixture escaped ART"));
    }
    let cwd = PathBuf::from(
        report["cwd"]
            .text()
            .ok_or_else(|| io::Error::other("owned cwd required"))?,
    );
    if !fs::canonicalize(&cwd)?.starts_with(&canonical_root) {
        return Err(io::Error::other("cwd escaped fixture"));
    }
    let restore = WorkingDirectory(std::env::current_dir()?);
    std::env::set_current_dir(&cwd)?;
    let mut results = Vec::new();
    let mut differences = 0_i64;
    for (index, record) in report["records"]
        .as_list()
        .ok_or_else(|| io::Error::other("records required"))?
        .iter()
        .enumerate()
    {
        let input = PathBuf::from(
            record["input"]
                .text()
                .ok_or_else(|| io::Error::other("generated path required"))?,
        );
        let checked = std::path::absolute(if input.as_os_str().is_empty() {
            Path::new(".")
        } else {
            &input
        })?;
        let resolved = if checked.exists() {
            fs::canonicalize(&checked)?
        } else {
            fs::canonicalize(
                checked
                    .parent()
                    .ok_or_else(|| io::Error::other("path parent required"))?,
            )?
            .join(
                checked
                    .file_name()
                    .ok_or_else(|| io::Error::other("path name required"))?,
            )
        };
        if !resolved.starts_with(&canonical_root) {
            return Err(io::Error::other("generated path escaped fixture"));
        }
        let observed = RefCell::new(Vec::new());
        let observer = |stage: UsageReadStage, path: &Path, _| {
            if stage == UsageReadStage::SafeComponents {
                observed
                    .borrow_mut()
                    .push(Value::String(filesystem::os_string(path.as_os_str())));
            }
            Ok(UsageFault::default())
        };
        let sample = usage_files::read_prefix_observed(&input, Limits::default(), &observer)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        let actual = Value::object([
            ("returned_length", Value::Integer(sample.data.len().into())),
            ("returned_sha256", ids::sha256_bytes(&sample.data).into()),
            (
                "diagnostics",
                Value::list(
                    sample
                        .diagnostics
                        .iter()
                        .map(|code| Value::from(code.as_str())),
                ),
            ),
            (
                "safe_component_arguments",
                Value::list(observed.into_inner()),
            ),
        ]);
        let expected = Value::Object(
            [
                "returned_length",
                "returned_sha256",
                "diagnostics",
                "safe_component_arguments",
            ]
            .iter()
            .map(|key| ((*key).into(), record[key].clone()))
            .collect(),
        );
        let matches = actual.container_eq(&expected);
        if !matches {
            differences += 1;
        }
        results.push(Value::object([
            ("index", Value::Integer(index.into())),
            ("input", record["input"].clone()),
            ("matches", matches.into()),
            ("expected", expected),
            ("native", actual),
        ]));
    }
    drop(restore);
    let result = Value::object([
        ("comparisons", Value::Integer(results.len().into())),
        ("differences", differences.into()),
        ("records", Value::list(results)),
        (
            "reference_sha256",
            ids::sha256_bytes(&reference_bytes).into(),
        ),
        (
            "native_binary_sha256",
            ids::sha256_bytes(&fs::read(std::env::current_exe()?)?).into(),
        ),
    ]);
    fs::write(
        output.join("report.json"),
        compat::dumps(&result, SerializationProfile::SORTED_STORED)
            .map_err(|error| io::Error::other(format!("{error:?}")))?,
    )?;
    println!(
        "{{\"comparisons\":{},\"differences\":{differences}}}",
        result["comparisons"].as_i64().unwrap()
    );
    if differences != 0 {
        std::process::exit(1);
    }
    Ok(())
}
