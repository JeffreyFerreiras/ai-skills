use graph_engine::application::graph_arguments;
use graph_engine::compat::{
    self,
    argparse::{ArgumentValue, ParserOutcome},
    PyString, SerializationProfile, Value,
};
use graph_engine::infrastructure::command_line::columns_value;
use std::path::PathBuf;

fn typed(value: ArgumentValue) -> Value {
    let (kind, value) = match value {
        ArgumentValue::None => ("NoneType", Value::Null),
        ArgumentValue::Boolean(value) => ("bool", value.into()),
        ArgumentValue::Integer(value) => ("int", value.to_string().into()),
        ArgumentValue::String(value) => ("str", Value::String(value)),
    };
    Value::object([("kind", kind.into()), ("value", value)])
}

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let source = PathBuf::from(&arguments[0]);
    let output = PathBuf::from(&arguments[1]);
    let report = compat::loads(&std::fs::read(&source).unwrap()).unwrap();
    let inputs = &report["provenance"]["graph_spec_inputs"];
    let hosts = inputs["hosts"]
        .as_list()
        .unwrap()
        .iter()
        .map(|value| value.text().unwrap())
        .collect::<Vec<_>>();
    let phases = inputs["usage_phases"]
        .as_list()
        .unwrap()
        .iter()
        .map(|value| value.text().unwrap())
        .collect::<Vec<_>>();
    let parser = graph_arguments::parser(
        &hosts.iter().map(String::as_str).collect::<Vec<_>>(),
        &inputs["default_host"].text().unwrap(),
        &phases.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut rows = Vec::new();
    let mut differences = Vec::new();
    for (index, expected) in report["records"].as_list().unwrap().iter().enumerate() {
        let argv = expected["argv"]
            .as_list()
            .unwrap()
            .iter()
            .map(|value| value.as_string().unwrap().clone())
            .collect::<Vec<_>>();
        let width = columns_value(expected["columns"].as_string().unwrap());
        let (code, values, order, exception, stdout, stderr) = match parser.parse(&argv, width) {
            ParserOutcome::Arguments(namespace) => {
                let mut values = Value::object([]);
                let mut order = Vec::new();
                for (name, value) in namespace.fields {
                    values.set(&name, typed(value));
                    order.push(name.into());
                }
                (
                    0,
                    values,
                    Value::List(order),
                    Value::Null,
                    PyString::default(),
                    PyString::default(),
                )
            }
            ParserOutcome::Exit {
                code,
                stdout,
                stderr,
            } => (
                code,
                Value::Null,
                Value::Null,
                Value::object([
                    ("kind", "SystemExit".into()),
                    (
                        "code",
                        Value::object([("kind", "int".into()), ("value", code.to_string().into())]),
                    ),
                ]),
                stdout,
                stderr,
            ),
        };
        let native = Value::object([
            ("id", expected["id"].clone()),
            ("family", expected["family"].clone()),
            ("argv", expected["argv"].clone()),
            ("columns", expected["columns"].clone()),
            ("exit_code", i64::from(code).into()),
            ("arguments", values),
            ("namespace_order", order),
            ("exception", exception),
            ("stdout", Value::String(stdout)),
            ("stderr", Value::String(stderr)),
        ]);
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
