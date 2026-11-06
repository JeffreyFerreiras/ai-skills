use graph_engine::{
    application::{
        error::{EngineError, EngineResult},
        policy::PolicyFilePort,
        process::{ProcessOutcome, ProcessPort, ProcessRequest},
        source_capture,
    },
    compat::{self, PyString, SerializationProfile, Value},
    domain::ids,
    infrastructure::{
        policy_files::NativePolicyFiles, process::NativeProcess, source_files::NativeSourceFiles,
    },
};
use std::io::Read;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

struct RecordedProcess {
    reads: RefCell<Vec<Value>>,
}
impl ProcessPort for RecordedProcess {
    fn capture(&self, request: &ProcessRequest<'_>) -> EngineResult<ProcessOutcome> {
        let result = NativeProcess.capture(request);
        let mut record = Value::object([
            (
                "argv",
                Value::list(request.arguments.iter().cloned().map(Value::String)),
            ),
            (
                "cwd",
                request.directory.to_string_lossy().into_owned().into(),
            ),
            (
                "budget_before",
                request
                    .stdout_limit
                    .map(|size| Value::from(size as i64))
                    .unwrap_or(Value::Null),
            ),
        ]);
        record.set("capture_stderr", request.capture_stderr.into());
        match &result {
            Ok(ProcessOutcome::Completed {
                exit_code,
                stdout,
                stderr,
            }) => {
                record.set("kind", "completed".into());
                record.set("exit", (*exit_code).into());
                record.set("stdout_hex", hex(stdout).into());
                record.set("stderr_hex", hex(stderr).into());
            }
            Ok(ProcessOutcome::TimedOut { stdout, stderr }) => {
                record.set("kind", "timeout".into());
                record.set("stdout_hex", hex(stdout).into());
                record.set("stderr_hex", hex(stderr).into());
            }
            Ok(ProcessOutcome::OutputLimit) => record.set("kind", "output_limit".into()),
            Err(error) => {
                record.set("kind", "error".into());
                record.set("error", format!("{error:?}").into());
            }
        }
        self.reads.borrow_mut().push(record);
        result
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unhex(value: &Value) -> Result<Vec<u8>, String> {
    let text = value.text().ok_or("hex input is not a string")?;
    if text.len() % 2 != 0 {
        return Err("odd hex input length".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .map_err(|error| error.to_string())
                .and_then(|text| u8::from_str_radix(text, 16).map_err(|error| error.to_string()))
        })
        .collect()
}
fn relative_path(text: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(text);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err("unsafe corpus relative path".into());
    }
    Ok(path)
}
fn seed_files(
    root: &Path,
    files: &NativePolicyFiles,
) -> Result<BTreeMap<String, (u64, String)>, String> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if files
                .is_symlink(&path)
                .map_err(|error| format!("{error:?}"))?
                || files
                    .is_reparse(&path)
                    .map_err(|error| format!("{error:?}"))?
            {
                return Err("seed contains a reparse path".into());
            }
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
                result.insert(relative, (metadata.len(), ids::sha256_bytes(&bytes)));
            } else {
                return Err("seed contains an unsupported file type".into());
            }
        }
    }
    Ok(result)
}
fn seed_bytes(path: &Path, root: &Path, expected: &(u64, String)) -> Result<Vec<u8>, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("seed input is not a regular file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            FileAttributeTagInfo, GetFileInformationByHandleEx, GetFinalPathNameByHandleW,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_NAME_NORMALIZED,
        };
        let handle = file.as_raw_handle() as _;
        let mut attributes: FILE_ATTRIBUTE_TAG_INFO = unsafe { std::mem::zeroed() };
        if unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileAttributeTagInfo,
                &mut attributes as *mut _ as _,
                std::mem::size_of_val(&attributes) as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err("seed input is a reparse file".into());
        }
        let count = unsafe {
            GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, FILE_NAME_NORMALIZED)
        };
        if count == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut name = vec![0u16; count as usize + 1];
        let written = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                name.as_mut_ptr(),
                name.len() as u32,
                FILE_NAME_NORMALIZED,
            )
        };
        if written == 0 || written as usize >= name.len() {
            return Err("seed final handle path unavailable".into());
        }
        let final_path = PathBuf::from(std::ffi::OsString::from_wide(&name[..written as usize]));
        if !final_path.starts_with(root.canonicalize().map_err(|error| error.to_string())?) {
            return Err("seed handle escaped owned root".into());
        }
    }
    #[cfg(unix)]
    {
        if !path
            .canonicalize()
            .map_err(|error| error.to_string())?
            .starts_with(root.canonicalize().map_err(|error| error.to_string())?)
        {
            return Err("seed file escaped owned root".into());
        }
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(expected.0 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != expected.0 || ids::sha256_bytes(&bytes) != expected.1 {
        return Err("seed file changed during copy".into());
    }
    Ok(bytes)
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    root: &Path,
    files: &NativePolicyFiles,
    expected: &BTreeMap<String, (u64, String)>,
) -> Result<(), String> {
    files
        .ensure_safe(source, Some(root))
        .map_err(|error| format!("{error:?}"))?;
    std::fs::create_dir(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if files
            .is_symlink(&path)
            .map_err(|error| format!("{error:?}"))?
            || files
                .is_reparse(&path)
                .map_err(|error| format!("{error:?}"))?
        {
            return Err("seed reparse path appeared during copy".into());
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.is_dir() {
            copy_tree(&path, &target, root, files, expected)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let expected = expected
                .get(&relative)
                .ok_or("unmanifested seed file during copy")?;
            let bytes = seed_bytes(&path, root, expected)?;
            std::fs::write(&target, &bytes).map_err(|error| error.to_string())?;
            std::fs::set_permissions(&target, metadata.permissions())
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
fn strings(value: &Value) -> Result<Vec<PyString>, String> {
    value
        .as_list()
        .ok_or("expected corpus string list")?
        .iter()
        .map(|item| {
            if let Value::String(text) = item {
                Ok(text.clone())
            } else {
                Err("nonstring corpus root".into())
            }
        })
        .collect()
}
fn outcome(result: EngineResult<Value>) -> Value {
    match result {
        Ok(value) => Value::object([("kind", "returned".into()), ("value", value)]),
        Err(EngineError::State(error)) => Value::object([
            ("kind", "exception".into()),
            ("class", "StateError".into()),
            ("message", error.to_string().into()),
        ]),
        Err(EngineError::Contract(error)) => Value::object([
            ("kind", "exception".into()),
            ("class", "ContractError".into()),
            ("message", error.to_string().into()),
        ]),
        Err(EngineError::Python { kind, message }) => Value::object([
            ("kind", "exception".into()),
            ("class", kind.into()),
            ("message", Value::String(message)),
        ]),
        Err(error) => Value::object([
            ("kind", "exception".into()),
            ("class", "NativeError".into()),
            ("message", format!("{error:?}").into()),
        ]),
    }
}
fn run() -> Result<(), String> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if arguments.len() != 2 {
        return Err("usage: source-capture-probe REFERENCE OUTPUT".into());
    }
    let artifact =
        PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").ok_or("ART required")?);
    let artifact_canonical = artifact.canonicalize().map_err(|error| error.to_string())?;
    let reference_path = &arguments[0];
    if !reference_path
        .canonicalize()
        .map_err(|error| error.to_string())?
        .starts_with(&artifact_canonical)
    {
        return Err("reference outside ART".into());
    }
    let output = &arguments[1];
    if !output
        .parent()
        .ok_or("output parent missing")?
        .canonicalize()
        .map_err(|error| error.to_string())?
        .starts_with(&artifact_canonical)
    {
        return Err("output outside ART".into());
    }
    std::fs::create_dir(output).map_err(|error| error.to_string())?;
    let reference_bytes = std::fs::read(reference_path).map_err(|error| error.to_string())?;
    let reference = compat::loads_value(&Value::Bytes(reference_bytes.clone()))
        .map_err(|error| format!("{error:?}"))?;
    let seed = reference_path.parent().unwrap().join("seed-repository");
    if !seed
        .canonicalize()
        .map_err(|error| error.to_string())?
        .starts_with(&artifact_canonical)
    {
        return Err("seed outside ART".into());
    }
    let files = NativePolicyFiles::default();
    let actual_seed = seed_files(&seed, &files)?;
    let mut expected_seed = BTreeMap::new();
    for item in reference["seed_files"]
        .as_list()
        .ok_or("seed manifest absent")?
    {
        let name = item["path"].text().ok_or("seed path absent")?;
        relative_path(&name)?;
        expected_seed.insert(
            name,
            (
                item["size"].as_i64().ok_or("seed size absent")? as u64,
                item["sha256"].text().ok_or("seed digest absent")?,
            ),
        );
    }
    if actual_seed != expected_seed {
        return Err("frozen seed manifest mismatch".into());
    }
    let fixture = loop {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = artifact.join("fixtures").join(format!(
            "source-capture-native-{}-{nonce}",
            std::process::id()
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    let repository = fixture.join("repository");
    std::fs::create_dir(&repository).map_err(|error| error.to_string())?;
    let initial = &reference["initial_file"];
    let initial_path = repository.join(relative_path(
        &initial["path"].text().ok_or("initial path absent")?,
    )?);
    std::fs::create_dir_all(initial_path.parent().unwrap()).map_err(|error| error.to_string())?;
    std::fs::write(initial_path, unhex(&initial["bytes_hex"])?)
        .map_err(|error| error.to_string())?;
    let cases = reference["records"].as_list().ok_or("records absent")?;
    if cases.len() > 12 {
        return Err("source corpus bound exceeded".into());
    }
    let policy = Value::object([("denied_patterns", Value::list([]))]);
    let mut results = Vec::new();
    let mut differences = 0i64;
    for (index, record) in cases.iter().enumerate() {
        let case = &record["case"];
        if case["initialize_git"].py_eq(&true.into()) {
            copy_tree(
                &seed.join(".git"),
                &repository.join(".git"),
                &seed,
                &files,
                &expected_seed,
            )?;
        }
        for action in case.get("actions").and_then(Value::as_list).unwrap_or(&[]) {
            let path = repository.join(relative_path(
                &action["path"].text().ok_or("action path absent")?,
            )?);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|error| error.to_string())?;
            std::fs::write(path, unhex(&action["bytes_hex"])?)
                .map_err(|error| error.to_string())?;
        }
        let source = NativeSourceFiles::new(&repository, &policy, &files);
        let process = RecordedProcess {
            reads: RefCell::new(vec![]),
        };
        let roots = strings(&case["roots"])?;
        let known = case
            .get("known_inputs")
            .map(strings)
            .transpose()?
            .unwrap_or_default();
        let outputs = case.get("outputs").and_then(Value::as_list).unwrap_or(&[]);
        let actual = outcome(source_capture::capture_source(
            &repository,
            &roots,
            outputs,
            &known,
            &source,
            &process,
        ));
        let native_reads = process.reads.into_inner();
        let expected_reads = record["git_reads"]
            .as_list()
            .ok_or("Git observations absent")?;
        let git_equal = expected_reads.len() == native_reads.len()
            && expected_reads
                .iter()
                .zip(&native_reads)
                .all(|(expected, actual)| {
                    expected["argv"].container_eq(&actual["argv"])
                        && expected["budget_before"].py_eq(&actual["budget_before"])
                        && expected["stderr"].is_text("DEVNULL")
                        && actual["capture_stderr"].py_eq(&false.into())
                        && (!expected["kind"].is_text("returned")
                            || actual["exit"].py_eq(&0i64.into()))
                        && (matches!(&expected["stdout_hex"], Value::Null)
                            || expected["stdout_hex"].py_eq(&actual["stdout_hex"]))
                });
        let equal = compat::dumps(&actual, SerializationProfile::STORED)
            .map_err(|error| format!("{error:?}"))?
            == compat::dumps(&record["expected"], SerializationProfile::STORED)
                .map_err(|error| format!("{error:?}"))?;
        if !equal || !git_equal {
            differences += 1;
        }
        results.push(Value::object([
            ("index", (index as i64).into()),
            ("name", case["name"].clone()),
            ("outcome_equal", equal.into()),
            ("git_equal", git_equal.into()),
            ("expected", record["expected"].clone()),
            ("actual", actual),
            ("git_reads", Value::List(native_reads)),
        ]));
    }
    let report = Value::object([
        ("requests", (cases.len() as i64).into()),
        ("differences", differences.into()),
        (
            "reference_sha256",
            ids::sha256_bytes(&reference_bytes).into(),
        ),
        ("fixture", fixture.to_string_lossy().into_owned().into()),
        (
            "binary_sha256",
            ids::sha256_bytes(
                &std::fs::read(std::env::current_exe().map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?,
            )
            .into(),
        ),
        ("records", Value::List(results)),
    ]);
    std::fs::write(
        output.join("report.json"),
        compat::dumps(&report, SerializationProfile::STORED)
            .map_err(|error| format!("{error:?}"))?,
    )
    .map_err(|error| error.to_string())?;
    println!("requests={} differences={differences}", cases.len());
    if differences != 0 {
        return Err("native source capture differences retained".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
