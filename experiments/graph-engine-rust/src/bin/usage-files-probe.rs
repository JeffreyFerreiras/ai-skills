use graph_engine::{
    compat::{self, SerializationProfile, Value},
    domain::{ids, usage::Limits},
    infrastructure::usage_files::{self, UsageFault, UsageReadStage},
};
use std::{
    cell::RefCell,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn contained(path: &Path, root: &Path) -> io::Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    if !path.starts_with(root) {
        return Err(io::Error::other("probe path escaped ART"));
    }
    Ok(path)
}
fn main() -> io::Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(io::Error::other(
            "expected reference report and output directory",
        ));
    }
    let artifact = fs::canonicalize(
        std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT")
            .ok_or_else(|| io::Error::other("ART is required"))?,
    )?;
    let reference = contained(Path::new(&args[0]), &artifact)?;
    let output = PathBuf::from(&args[1]);
    contained(
        output
            .parent()
            .ok_or_else(|| io::Error::other("output parent required"))?,
        &artifact,
    )?;
    fs::create_dir(&output)?;
    let fixtures = contained(&artifact.join("fixtures"), &artifact)?;
    let fixture = loop {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let candidate = fixtures.join(format!("usage-files-native-{}-{nonce}", std::process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => break contained(&candidate, &fixtures)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let reference_bytes = fs::read(&reference)?;
    let report = compat::loads_value(&Value::Bytes(reference_bytes.clone()))
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let append = report["append_before_read"].is_true();
    let records = report["records"]
        .as_list()
        .ok_or_else(|| io::Error::other("reference records required"))?;
    let mut results = Vec::new();
    let mut differences = 0_i64;
    for (index, record) in records.iter().enumerate() {
        let size = record["size"]
            .as_i64()
            .ok_or_else(|| io::Error::other("generated size required"))?
            as usize;
        let mutate = record["mutate_before_seek"].is_true();
        let path = fixture.join(format!("{index}.generated"));
        fs::write(&path, vec![b'A'; size])?;
        let events = RefCell::new(Vec::new());
        let observer = |stage: UsageReadStage, observed_path: &Path, flags: i32| {
            events.borrow_mut().push(Value::object([
                ("stage", format!("{stage:?}").into()),
                ("requested_flags", i64::from(flags).into()),
            ]));
            if stage == UsageReadStage::BeforeRead && append {
                fs::OpenOptions::new()
                    .append(true)
                    .open(observed_path)?
                    .write_all(&[b'C'; 16])?;
            }
            if stage == UsageReadStage::BeforeVerify && mutate && size != 0 {
                fs::OpenOptions::new()
                    .write(true)
                    .open(observed_path)?
                    .write_all(b"B")?;
            }
            Ok(UsageFault::default())
        };
        let sampled = usage_files::read_prefix_observed(&path, Limits::default(), &observer)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        let post = fs::read(&path)?;
        let actual = Value::object([
            ("returned_length", Value::Integer(sampled.data.len().into())),
            ("returned_sha256", ids::sha256_bytes(&sampled.data).into()),
            (
                "diagnostics",
                Value::list(
                    sampled
                        .diagnostics
                        .iter()
                        .map(|code| Value::from(code.as_str())),
                ),
            ),
            ("post_length", Value::Integer(post.len().into())),
            ("post_sha256", ids::sha256_bytes(&post).into()),
        ]);
        let expected = Value::Object(
            [
                "returned_length",
                "returned_sha256",
                "diagnostics",
                "post_length",
                "post_sha256",
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
            ("size", Value::Integer(size.into())),
            ("mutate_before_seek", mutate.into()),
            ("matches", matches.into()),
            ("expected", expected),
            ("native", actual),
            ("native_events", Value::list(events.into_inner())),
        ]));
        contained(&path, &fixture)?;
        fs::remove_file(&path)?;
    }
    contained(&fixture, &fixtures)?;
    fs::remove_dir(&fixture)?;
    let result = Value::object([
        ("comparisons", Value::Integer(records.len().into())),
        ("differences", differences.into()),
        (
            "reference_sha256",
            ids::sha256_bytes(&reference_bytes).into(),
        ),
        (
            "native_binary_sha256",
            ids::sha256_bytes(&fs::read(std::env::current_exe()?)?).into(),
        ),
        ("append_before_read", append.into()),
        ("records", Value::list(results)),
        (
            "teardown",
            "closed descriptors; contained generated files removed".into(),
        ),
    ]);
    fs::write(
        output.join("report.json"),
        compat::dumps(&result, SerializationProfile::SORTED_STORED)
            .map_err(|error| io::Error::other(format!("{error:?}")))?,
    )?;
    println!(
        "{{\"comparisons\":{},\"differences\":{differences}}}",
        records.len()
    );
    if differences != 0 {
        std::process::exit(1);
    }
    Ok(())
}
