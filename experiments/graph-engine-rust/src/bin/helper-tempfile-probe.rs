use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        policy::PolicyFilePort,
    },
    compat::{self, SerializationProfile, Value},
    infrastructure::{
        filesystem,
        os_error::OsFailure,
        policy_files::NativePolicyFiles,
        python_temporary::{self, TemporaryDirectoryCache},
    },
};
use std::{
    io,
    path::{Path, PathBuf},
};

struct Names {
    values: Vec<String>,
    calls: usize,
}
impl Names {
    fn new(value: &Value) -> Self {
        Self {
            values: value
                .as_list()
                .unwrap()
                .iter()
                .map(|value| value.text().unwrap())
                .collect(),
            calls: 0,
        }
    }
    fn next(&mut self) -> io::Result<String> {
        let value = self.values[self.calls.min(self.values.len() - 1)].clone();
        self.calls += 1;
        Ok(value)
    }
}
fn outcome(result: EngineResult<PathBuf>) -> Value {
    match result {
        Ok(path) => Value::object([
            ("ok", true.into()),
            (
                "result",
                Value::String(filesystem::os_string(path.as_os_str())),
            ),
        ]),
        Err(EngineError::Io(error)) => {
            let metadata = error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<OsFailure>());
            let kind = match error.kind() {
                io::ErrorKind::NotFound => "FileNotFoundError",
                io::ErrorKind::AlreadyExists => "FileExistsError",
                io::ErrorKind::PermissionDenied => "PermissionError",
                _ => "OSError",
            };
            Value::object([
                ("ok", false.into()),
                (
                    "error",
                    Value::object([
                        ("kind", kind.into()),
                        ("message", error.to_string().into()),
                        (
                            "errno",
                            metadata
                                .and_then(|metadata| metadata.errno)
                                .map_or(Value::Null, |code| i64::from(code).into()),
                        ),
                        (
                            "winerror",
                            metadata
                                .and_then(|metadata| metadata.winerror)
                                .map_or(Value::Null, |code| i64::from(code).into()),
                        ),
                        (
                            "filename",
                            metadata
                                .and_then(|metadata| metadata.filename.as_ref())
                                .map_or(Value::Null, |filename| Value::String(filename.clone())),
                        ),
                    ]),
                ),
            ])
        }
        Err(error) => panic!("unexpected probe error: {error:?}"),
    }
}
fn paths(value: &Value, artifact: &Path) -> Vec<PathBuf> {
    let files = NativePolicyFiles::default();
    value
        .as_list()
        .unwrap()
        .iter()
        .map(|value| {
            let path = PathBuf::from(value.text().unwrap());
            let absolute = files.absolute(&path).unwrap();
            assert!(
                files.under(&absolute, artifact),
                "filesystem candidates must remain under ART"
            );
            path
        })
        .collect()
}
fn run(input: &Value, artifact: &Path) -> Value {
    if input["op"].is_text("candidates") {
        let mut previous = Vec::new();
        for (key, value) in input["environment"].as_object().unwrap() {
            let key = key.utf8().unwrap();
            previous.push((key.clone(), std::env::var_os(&key)));
            // This dedicated probe has one thread and restores every process-local override.
            unsafe {
                if matches!(value, Value::Null) {
                    std::env::remove_var(&key);
                } else {
                    std::env::set_var(&key, value.text().unwrap());
                }
            }
        }
        let candidates = python_temporary::candidates();
        for (key, value) in previous {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
        return Value::object([
            ("ok", true.into()),
            (
                "result",
                Value::List(
                    candidates
                        .iter()
                        .map(|path| Value::String(filesystem::os_string(path.as_os_str())))
                        .collect(),
                ),
            ),
            ("attempts", 0_i64.into()),
        ]);
    }
    let mut names = Names::new(&input["names"]);
    let mut result = if input["op"].is_text("default") {
        outcome(python_temporary::find_usable(
            &paths(&input["paths"], artifact),
            &mut || names.next(),
        ))
    } else {
        let cache = TemporaryDirectoryCache::default();
        let mut results = Vec::new();
        for group in input["path_groups"].as_list().unwrap() {
            let selected = paths(group, artifact);
            results.push(outcome(cache.get_or_try_init(|| {
                python_temporary::find_usable(&selected, &mut || names.next())
            })));
        }
        Value::object([("ok", true.into()), ("result", Value::List(results))])
    };
    result.set("attempts", (names.calls as i64).into());
    result
}
fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let reference = PathBuf::from(&arguments[0]);
    let output = PathBuf::from(&arguments[1]);
    let files = NativePolicyFiles::default();
    let artifact = files
        .resolve(
            &PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").unwrap()),
            true,
        )
        .unwrap();
    assert!(files.under(&files.absolute(&output).unwrap(), &artifact));
    assert!(files.under(&files.absolute(&reference).unwrap(), &artifact));
    let report = compat::loads(&std::fs::read(reference).unwrap()).unwrap();
    std::env::set_current_dir(&artifact).unwrap();
    let mut differences = Vec::new();
    let mut rows = Vec::new();
    for (index, record) in report["records"].as_list().unwrap().iter().enumerate() {
        let native = run(&record["input"], &artifact);
        if !native.container_eq(&record["expected"]) {
            differences.push(Value::object([
                ("index", (index as i64).into()),
                ("input", record["input"].clone()),
                ("expected", record["expected"].clone()),
                ("native", native.clone()),
            ]));
        }
        rows.push(native);
    }
    let failed = differences.len();
    let result = Value::object([
        ("count", (rows.len() as i64).into()),
        ("difference_count", (failed as i64).into()),
        ("differences", Value::List(differences)),
        ("native", Value::List(rows)),
    ]);
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(
        output,
        compat::dumps(&result, SerializationProfile::CLI).unwrap(),
    )
    .unwrap();
    println!(
        "cases={} differences={failed}",
        result["count"].as_i64().unwrap()
    );
    if failed != 0 {
        std::process::exit(1);
    }
}
