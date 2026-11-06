use graph_engine::{
    application::{
        check_fences::CheckClockPort,
        claim_runtime::{lease_expired, ClaimRuntimePort},
        error::{EngineError, EngineResult},
    },
    compat::{self, Value},
    domain::ids,
};
use num_bigint::BigInt;
use std::{
    cell::Cell,
    io::{self, BufRead, Write},
};

struct Clock {
    now: i128,
    fail: bool,
    reads: Cell<usize>,
}
impl CheckClockPort for Clock {
    fn utc_now(&self) -> EngineResult<String> {
        panic!("wrong precision clock port")
    }
}
impl ClaimRuntimePort for Clock {
    fn utc_now_microseconds(&self) -> EngineResult<i128> {
        self.reads.set(self.reads.get() + 1);
        if self.fail {
            Err(EngineError::python("LeaseClockError", "clock sentinel"))
        } else {
            Ok(self.now)
        }
    }
    fn random_bytes(&self, _: usize) -> EngineResult<Vec<u8>> {
        panic!("unexpected entropy request")
    }
    fn utc_after(&self, _: &BigInt) -> EngineResult<String> {
        panic!("unexpected deadline construction")
    }
}

fn decode(value: &Value) -> Value {
    let payload = &value["v"];
    match value["t"].text().expect("typed tag").as_str() {
        "null" => Value::Null,
        "str" | "bool" => payload.clone(),
        "int" => Value::Integer(
            payload
                .text()
                .expect("integer text")
                .parse()
                .expect("decimal integer"),
        ),
        "float" => Value::Float(f64::from_bits(
            u64::from_str_radix(&payload.text().expect("float bits"), 16).expect("hex bits"),
        )),
        "bytes" => {
            let text = payload.text().expect("byte hex");
            Value::Bytes(
                (0..text.len())
                    .step_by(2)
                    .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex byte"))
                    .collect(),
            )
        }
        "list" => Value::List(
            payload
                .as_list()
                .expect("list items")
                .iter()
                .map(decode)
                .collect(),
        ),
        "tuple" => Value::Tuple(
            payload
                .as_list()
                .expect("tuple items")
                .iter()
                .map(decode)
                .collect(),
        ),
        "dict" => Value::Object(
            payload
                .as_list()
                .expect("dict pairs")
                .iter()
                .map(|pair| {
                    let pair = pair.as_list().expect("pair");
                    (
                        decode(&pair[0]).as_string().expect("string key").clone(),
                        decode(&pair[1]),
                    )
                })
                .collect(),
        ),
        tag => panic!("unsupported typed tag {tag}"),
    }
}

fn main() {
    let lines = io::stdin()
        .lock()
        .lines()
        .map(|line| line.expect("input line"))
        .collect::<Vec<_>>();
    assert!(lines.len() <= 192);
    let mut output = io::stdout().lock();
    for line in lines {
        let request = compat::loads(line.as_bytes()).expect("request JSON");
        let clock = Clock {
            now: request["now_microseconds"]
                .text()
                .expect("clock text")
                .parse()
                .expect("clock microseconds"),
            fail: request["clock"].is_text("raise"),
            reads: Cell::new(0),
        };
        let outcome = match lease_expired(&decode(&request["value"]), &clock) {
            Ok(value) => Value::object([("ok", value.into())]),
            Err(EngineError::Python { kind, message }) => Value::object([(
                "error",
                Value::object([("kind", kind.into()), ("message", Value::String(message))]),
            )]),
            Err(error) => {
                eprintln!("unexpected native failure: {error:?}");
                std::process::exit(1);
            }
        };
        let result = Value::object([
            ("id", request["id"].clone()),
            ("outcome", outcome),
            ("clock_reads", Value::Integer(clock.reads.get().into())),
        ]);
        output
            .write_all(&ids::canonical_bytes(&result).expect("result serialization"))
            .expect("stdout");
        output.write_all(b"\n").expect("line terminator");
    }
}
