use graph_engine::{
    compat::{self, SerializationProfile, Value},
    infrastructure::command_line,
};
use std::io::{self, Write};

fn main() {
    let arguments = command_line::arguments();
    match arguments
        .first()
        .and_then(|value| value.utf8().ok())
        .as_deref()
    {
        Some("emit") => {
            io::stdout().write_all(b"out\0\xff").unwrap();
            io::stderr().write_all(b"err\0\xfe").unwrap();
            std::process::exit(7);
        }
        Some("delay") => {
            write!(io::stdout(), "ready:{}", std::process::id()).unwrap();
            io::stdout().flush().unwrap();
            io::stderr().write_all(b"waiting").unwrap();
            io::stderr().flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
        Some("exit-high") => std::process::exit(-1),
        Some("flood") => {
            let block = [b'x'; 65536];
            loop {
                if io::stdout().write_all(&block).is_err() {
                    return;
                }
            }
        }
        Some("arguments") => {
            let value = Value::list(arguments.into_iter().skip(1).map(Value::String));
            io::stdout()
                .write_all(&compat::dumps(&value, SerializationProfile::CLI).unwrap())
                .unwrap();
        }
        _ => std::process::exit(2),
    }
}
