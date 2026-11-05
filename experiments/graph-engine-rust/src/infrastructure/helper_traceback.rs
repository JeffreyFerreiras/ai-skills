use super::traceback_frames::{append, frame};
use crate::compat::PyString;
use std::path::Path;

#[derive(Clone, Copy)]
pub enum HelperInputSite {
    Context,
    PreflightRequest,
    ReserveRequest,
    SettleSettlement,
}

/// The frozen CPython 3.12.10 JSON input boundary includes these source frames.
pub fn input_json_traceback(
    installation: &Path,
    site: HelperInputSite,
    kind: &str,
    message: &PyString,
) -> PyString {
    let script = installation.join("scripts").join("helper-register.py");
    let mut output = PyString::from("Traceback (most recent call last):\n");
    frame(
        &mut output,
        &script,
        81,
        "<module>",
        "raise SystemExit(main())",
        Some("main()"),
    );
    frame(
        &mut output,
        &script,
        68,
        "main",
        "result = execute(argv)",
        Some("execute(argv)"),
    );
    let (line, source, expression) = match site {
        HelperInputSite::Context => (56, "context = _input(args.context)", "_input(args.context)"),
        HelperInputSite::PreflightRequest => (
            58,
            "return register.preflight(Path(args.register), context, _input(args.request))",
            "_input(args.request)",
        ),
        HelperInputSite::ReserveRequest => (
            60,
            "return register.reserve(Path(args.register), context, _input(args.request))",
            "_input(args.request)",
        ),
        HelperInputSite::SettleSettlement => (
            62,
            "return register.settle(Path(args.register), context, _input(args.settlement))",
            "_input(args.settlement)",
        ),
    };
    frame(
        &mut output,
        &script,
        line,
        "execute",
        source,
        Some(expression),
    );
    frame(
        &mut output,
        &script,
        20,
        "_input",
        "return safe_json_snapshot(candidate, [candidate.parent], MAX_RECORD_BYTES).parsed",
        Some("safe_json_snapshot(candidate, [candidate.parent], MAX_RECORD_BYTES)"),
    );
    frame(
        &mut output,
        &installation.join("graph_engine").join("contracts.py"),
        231,
        "safe_json_snapshot",
        "parsed = json.loads(snapshot.data.decode(\"utf-8\"))",
        Some("json.loads(snapshot.data.decode(\"utf-8\"))"),
    );
    // These diagnostic paths identify the selected frozen oracle, without reading it.
    frame(
        &mut output,
        Path::new(r"C:\Python312\Lib\json\__init__.py"),
        346,
        "loads",
        "return _default_decoder.decode(s)",
        Some("_default_decoder.decode(s)"),
    );
    frame(
        &mut output,
        Path::new(r"C:\Python312\Lib\json\decoder.py"),
        338,
        "decode",
        "obj, end = self.raw_decode(s, idx=_w(s, 0).end())",
        Some("self.raw_decode(s, idx=_w(s, 0).end())"),
    );
    frame(
        &mut output,
        Path::new(r"C:\Python312\Lib\json\decoder.py"),
        354,
        "raw_decode",
        "obj, end = self.scan_once(s, idx)",
        Some("self.scan_once(s, idx)"),
    );
    append(&mut output, format!("{kind}: "));
    append(&mut output, message.clone());
    append(&mut output, "\n");
    output
}
