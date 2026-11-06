use graph_engine::compat::{
    self,
    argparse::{helper_arguments, ParseOutcome},
    SerializationProfile, Value,
};
use std::path::PathBuf;

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let source = PathBuf::from(&arguments[0]);
    let output = PathBuf::from(&arguments[1]);
    let report = compat::loads(&std::fs::read(&source).unwrap()).unwrap();
    let width = report["environment"]["COLUMNS"]
        .text()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let mut rows = Vec::new();
    let mut differences = Vec::new();
    for (index, expected) in report["records"].as_list().unwrap().iter().enumerate() {
        let argv = expected["argv"]
            .as_list()
            .unwrap()
            .iter()
            .map(|value| value.as_string().unwrap().clone())
            .collect::<Vec<_>>();
        let selected_width = expected.get("columns").map_or(width, |value| {
            graph_engine::infrastructure::command_line::columns_value(value.as_string().unwrap())
        });
        let (code, namespace, stdout, stderr) = match helper_arguments(&argv, selected_width) {
            ParseOutcome::Arguments(arguments) => {
                let mut namespace = Value::object([("command", arguments.command.into())]);
                for (key, value) in arguments.fields {
                    namespace.set(&key.replace('-', "_"), Value::String(value));
                }
                (0, namespace, "".into(), "".into())
            }
            ParseOutcome::Exit {
                code,
                stdout,
                stderr,
            } => (code, Value::Null, stdout, stderr),
        };
        let mut native = Value::object([
            ("argv", expected["argv"].clone()),
            ("exit_code", i64::from(code).into()),
            ("arguments", namespace),
            ("stdout", Value::String(stdout)),
            ("stderr", Value::String(stderr)),
        ]);
        if let Some(columns) = expected.get("columns") {
            native.set("columns", columns.clone());
        }
        if !native.container_eq(expected) {
            differences.push(Value::object([
                ("index", (index as i64).into()),
                ("expected", expected.clone()),
                ("native", native.clone()),
            ]));
        }
        rows.push(native);
    }
    let result = Value::object([
        ("count", (rows.len() as i64).into()),
        ("difference_count", (differences.len() as i64).into()),
        ("differences", Value::List(differences)),
        ("native", Value::List(rows)),
    ]);
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(
        &output,
        compat::dumps(&result, SerializationProfile::CLI).unwrap(),
    )
    .unwrap();
    println!(
        "cases={} differences={}",
        result["count"].as_i64().unwrap(),
        result["difference_count"].as_i64().unwrap()
    );
    if result["difference_count"].as_i64().unwrap() != 0 {
        std::process::exit(1);
    }
}
