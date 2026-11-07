use graph_engine::{
    compat::{self, PyString, SerializationProfile, Value},
    domain::{
        errors::ValidationFailure,
        usage::{self, Context, Counters, Limits, Source},
        usage_contracts::{self, Binding, UsageFailure},
    },
};
use num_traits::ToPrimitive;
use std::io::{self, BufRead, Write};

struct Failure {
    kind: &'static str,
    message: PyString,
}
impl From<ValidationFailure> for Failure {
    fn from(error: ValidationFailure) -> Self {
        match error {
            ValidationFailure::Contract(error) => Self {
                kind: "ContractError",
                message: error.to_string().into(),
            },
            ValidationFailure::Python { kind, message } => Self { kind, message },
        }
    }
}
impl From<UsageFailure> for Failure {
    fn from(error: UsageFailure) -> Self {
        match error {
            UsageFailure::State(code) => Self {
                kind: "StateError",
                message: code.into(),
            },
            UsageFailure::Validation(error) => error.into(),
        }
    }
}
impl From<compat::json::JsonError> for Failure {
    fn from(error: compat::json::JsonError) -> Self {
        Self {
            kind: error.kind,
            message: error.message.into(),
        }
    }
}

fn bytes(value: &Value) -> Vec<u8> {
    let text = value.text().expect("generated hexadecimal input");
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII hex"), 16)
                .expect("generated hex pair")
        })
        .collect()
}
fn source(request: &Value) -> Source {
    let failures = match request.get("failures") {
        Some(Value::List(items)) => items
            .iter()
            .map(|value| value.text().expect("generated diagnostic"))
            .collect(),
        _ => Vec::new(),
    };
    let mut limits = Limits::default();
    for (key, target) in [
        ("max_bytes", &mut limits.max_bytes),
        ("chunk_bytes", &mut limits.chunk_bytes),
        ("max_records", &mut limits.max_records),
    ] {
        if let Some(Value::Integer(value)) = request.get(key) {
            *target = value.to_usize().expect("generated nonnegative limit");
        }
    }
    usage::parse_source(
        bytes(&request["data_hex"]),
        &failures,
        usage::known_models(),
        limits,
    )
}
fn source_value(source: &Source) -> Value {
    Value::object([
        (
            "source_id",
            source
                .source_id
                .as_ref()
                .map_or(Value::Null, |value| value.as_str().into()),
        ),
        (
            "snapshots",
            Value::list(source.snapshots.iter().map(|snapshot| {
                Value::object([
                    ("offset", Value::Integer(snapshot.offset.into())),
                    ("counters", snapshot.counters.value()),
                    (
                        "last",
                        snapshot.last.as_ref().map_or(Value::Null, Counters::value),
                    ),
                    ("context", snapshot.context.value()),
                    ("ambiguous", snapshot.ambiguous.into()),
                ])
            })),
        ),
        (
            "diagnostics",
            Value::list(source.diagnostics.iter().map(|(offset, code)| {
                Value::list([Value::Integer((*offset).into()), code.as_str().into()])
            })),
        ),
    ])
}
fn run(request: &Value) -> Result<Value, Failure> {
    Ok(match request["op"].text().as_deref() {
        Some("integer") => usage_contracts::integer(&request["value"]).into(),
        Some("counters") => Counters::from_value(
            &request["value"],
            matches!(request["aggregate"], Value::Bool(true)),
        )
        .as_ref()
        .map_or(Value::Null, Counters::value),
        Some("source_counters") => Counters::from_source(&request["value"])
            .as_ref()
            .map_or(Value::Null, Counters::value),
        Some("context") => Context::from_value(&request["value"], usage::known_models())?.value(),
        Some("bounded_json") => usage::bounded_json(&bytes(&request["data_hex"]))?,
        Some("read_source") => source_value(&source(request)),
        Some("source_checkpoint") => source(request).checkpoint(request.get("offset"))?,
        Some("checkpoint") => source(request).usable_checkpoint()?,
        Some("validate_checkpoint") => {
            usage_contracts::validate_checkpoint(&source(request), &request["value"])?
        }
        Some("validate_binding") => {
            usage_contracts::validate_binding(&request["value"])?;
            Value::Null
        }
        Some("advance") => {
            let mut binding = Binding::from_value(request["value"].clone())?;
            binding.interval.advance(
                &source(request),
                matches!(request["closing"], Value::Bool(true)),
                &request["boundary_kind"].text().expect("generated boundary"),
            )?;
            binding.value()
        }
        Some("summary") => {
            let Value::List(values) = &request["values"] else {
                unreachable!("generated binding list")
            };
            let bindings = values
                .iter()
                .map(|value| Binding::from_value(value.clone()))
                .collect::<Result<Vec<_>, _>>()?;
            let intervals = bindings
                .iter()
                .map(|binding| &binding.interval)
                .collect::<Vec<_>>();
            let segments = bindings
                .iter()
                .flat_map(|binding| &binding.interval.segments)
                .collect::<Vec<_>>();
            usage::summarize(
                &intervals,
                &segments,
                matches!(request["missing"], Value::Bool(true)),
            )?
            .value()
        }
        _ => unreachable!("generated operation"),
    })
}
fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let request = compat::loads_value(&Value::Bytes(line?.into_bytes()));
        let result = request
            .map_err(Failure::from)
            .and_then(|request| run(&request));
        let response = match result {
            Ok(value) => {
                let encoded = compat::dumps(&value, SerializationProfile::STORED)
                    .expect("generated result is serializable");
                let hexadecimal = encoded
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                Value::object([
                    ("ok", true.into()),
                    ("value", value),
                    ("stored_hex", hexadecimal.into()),
                ])
            }
            Err(error) => Value::object([
                ("ok", false.into()),
                ("type", error.kind.into()),
                ("message", Value::String(error.message)),
            ]),
        };
        stdout.write_all(
            &compat::dumps(&response, SerializationProfile::CLI)
                .expect("generated response is serializable"),
        )?;
        stdout.write_all(b"\n")?;
    }
    stdout.flush()
}
