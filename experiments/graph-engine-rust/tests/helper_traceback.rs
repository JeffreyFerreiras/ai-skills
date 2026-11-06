#[cfg(windows)]
#[test]
fn frozen_json_input_tracebacks_preserve_all_frames_source_carets_and_raw_bytes() {
    use graph_engine::{
        application::policy::PolicyFilePort,
        compat::PyString,
        infrastructure::{
            command_line,
            helper_traceback::{input_json_traceback, HelperInputSite},
            policy_files::NativePolicyFiles,
            text_stream::Stream,
        },
    };
    use std::path::PathBuf;
    let artifacts =
        PathBuf::from(std::env::var_os("GRAPH_RUST_RUN_ARTIFACT_ROOT").expect("artifact root"));
    let files = NativePolicyFiles::default();
    let installation = files
        .resolve(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../skills/software-engineering-graph"),
            false,
        )
        .unwrap();
    for (index, kind) in [(23, "ValueError"), (24, "RecursionError")] {
        let expected =
            std::fs::read(artifacts.join(format!("parity/helper-cli-02/{index}.oracle.stderr")))
                .unwrap();
        let text = std::str::from_utf8(&expected).unwrap();
        let terminal = text.lines().last().unwrap();
        let message = terminal.strip_prefix(&format!("{kind}: ")).unwrap();
        let traceback = input_json_traceback(
            &installation,
            HelperInputSite::Context,
            kind,
            &PyString::from(message),
        );
        let mut actual = Vec::new();
        command_line::write_text(&mut actual, &traceback, Stream::Stderr).unwrap();
        if actual != expected {
            let directory = artifacts.join("parity/helper-traceback-renderer-regressions");
            std::fs::create_dir_all(&directory).unwrap();
            let digest = graph_engine::domain::ids::sha256_bytes(&actual);
            let path = directory.join(format!("{digest}.stderr"));
            if !path.exists() {
                std::fs::write(&path, &actual).unwrap();
            }
            let offset = actual
                .iter()
                .zip(&expected)
                .position(|(a, b)| a != b)
                .unwrap_or(actual.len().min(expected.len()));
            panic!("frozen traceback case {index}: first differing byte {offset}; complete actual at {}", path.display());
        }
    }
}
