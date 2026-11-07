use super::PyString;

mod engine;
pub use engine::{ArgumentKind, ArgumentSpec, ArgumentValue, Namespace, ParserOutcome, ParserSpec};

pub struct Command {
    pub name: &'static str,
    pub fields: &'static [&'static str],
}

pub const HELPER_COMMANDS: &[Command] = &[
    Command {
        name: "initialize",
        fields: &[
            "state-root",
            "repo",
            "run-id",
            "plan",
            "allowance",
            "host-observation",
        ],
    },
    Command {
        name: "preflight",
        fields: &["register", "context", "request"],
    },
    Command {
        name: "reserve",
        fields: &["register", "context", "request"],
    },
    Command {
        name: "settle",
        fields: &["register", "context", "settlement"],
    },
    Command {
        name: "status",
        fields: &["register", "context"],
    },
];

pub struct Arguments {
    pub command: &'static str,
    pub fields: Vec<(&'static str, PyString)>,
}

impl Arguments {
    pub fn get(&self, name: &str) -> &PyString {
        &self
            .fields
            .iter()
            .find(|(key, _)| *key == name)
            .expect("required parser field")
            .1
    }
}

pub enum ParseOutcome {
    Arguments(Arguments),
    Exit {
        code: i32,
        stdout: PyString,
        stderr: PyString,
    },
}

pub fn helper_arguments(argv: &[PyString], width: usize) -> ParseOutcome {
    let mut parser = ParserSpec::new("helper-register");
    parser.subcommand_destination("command");
    for command in HELPER_COMMANDS {
        let mut child = ParserSpec::new(command.name);
        for field in command.fields {
            child.argument(ArgumentSpec::string(field).required());
        }
        parser.subcommand(command.name, None, child);
    }
    match parser.parse(argv, width) {
        ParserOutcome::Arguments(namespace) => {
            let command = HELPER_COMMANDS.iter().find(|command| {
                matches!(namespace.get("command"), Some(ArgumentValue::String(name)) if name.equals(command.name))
            }).expect("parsed helper command");
            let fields = command
                .fields
                .iter()
                .map(|field| {
                    let destination = field.replace('-', "_");
                    let Some(ArgumentValue::String(value)) = namespace.get(&destination) else {
                        unreachable!("required helper string argument")
                    };
                    (*field, value.clone())
                })
                .collect();
            ParseOutcome::Arguments(Arguments {
                command: command.name,
                fields,
            })
        }
        ParserOutcome::Exit {
            code,
            stdout,
            stderr,
        } => ParseOutcome::Exit {
            code,
            stdout,
            stderr,
        },
    }
}

fn text(value: &str) -> PyString {
    value.into()
}
fn append(target: &mut PyString, value: &PyString) {
    target.0.extend_from_slice(&value.0);
}
fn joined(values: &[PyString], separator: &str) -> PyString {
    let mut result = PyString::default();
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            append(&mut result, &text(separator));
        }
        append(&mut result, value);
    }
    result
}

fn negative_number(value: &PyString) -> bool {
    let trimmed = value
        .0
        .strip_suffix(&[10])
        .map(|points| PyString(points.to_vec()));
    let value = trimmed.as_ref().unwrap_or(value);
    if value.0.first() != Some(&45) || value.0.len() == 1 {
        return false;
    }
    let mut dot = None;
    for (index, point) in value.0.iter().copied().enumerate().skip(1) {
        if point == 46 && dot.is_none() {
            dot = Some(index);
        } else if !super::unicode::property(point, 3) {
            return false;
        }
    }
    dot.is_none_or(|index| index + 1 < value.0.len())
}
