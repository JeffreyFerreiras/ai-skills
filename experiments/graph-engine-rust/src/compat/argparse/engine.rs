use super::{append, joined, negative_number, text, PyString};
use num_bigint::BigInt;

#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentValue {
    None,
    Boolean(bool),
    Integer(BigInt),
    String(PyString),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Namespace {
    pub fields: Vec<(String, ArgumentValue)>,
}

impl Namespace {
    pub fn get(&self, destination: &str) -> Option<&ArgumentValue> {
        self.fields
            .iter()
            .find(|(name, _)| name == destination)
            .map(|(_, value)| value)
    }

    fn set(&mut self, destination: &str, value: ArgumentValue) {
        if let Some((_, previous)) = self.fields.iter_mut().find(|(name, _)| name == destination) {
            *previous = value;
        } else {
            self.fields.push((destination.to_owned(), value));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentKind {
    Help,
    String,
    Integer,
    Flag,
}

#[derive(Clone, Debug)]
pub struct ArgumentSpec {
    pub name: String,
    pub destination: String,
    pub kind: ArgumentKind,
    pub required: bool,
    pub default: Option<ArgumentValue>,
    pub choices: Vec<String>,
    pub help: Option<String>,
}

impl ArgumentSpec {
    pub fn string(name: &str) -> Self {
        Self {
            name: format!("--{name}"),
            destination: name.replace('-', "_"),
            kind: ArgumentKind::String,
            required: false,
            default: Some(ArgumentValue::None),
            choices: Vec::new(),
            help: None,
        }
    }

    pub fn integer(name: &str) -> Self {
        let mut result = Self::string(name);
        result.kind = ArgumentKind::Integer;
        result
    }
    pub fn flag(name: &str) -> Self {
        let mut result = Self::string(name);
        result.kind = ArgumentKind::Flag;
        result.default = Some(ArgumentValue::Boolean(false));
        result
    }
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
    pub fn suppressed_default(mut self) -> Self {
        self.default = None;
        self
    }
    pub fn default_string(mut self, value: &str) -> Self {
        self.default = Some(ArgumentValue::String(value.into()));
        self
    }
    pub fn choices(mut self, choices: &[&str]) -> Self {
        self.choices = choices.iter().map(|choice| (*choice).to_owned()).collect();
        self
    }
    pub fn help(mut self, help: &str) -> Self {
        self.help = Some(help.to_owned());
        self
    }

    fn takes_value(&self) -> bool {
        matches!(self.kind, ArgumentKind::String | ArgumentKind::Integer)
    }
    fn action_name(&self) -> &str {
        if self.kind == ArgumentKind::Help {
            "-h/--help"
        } else {
            &self.name
        }
    }
    fn metavar(&self) -> String {
        if self.choices.is_empty() {
            self.destination.to_ascii_uppercase()
        } else {
            format!("{{{}}}", self.choices.join(","))
        }
    }
    fn invocation(&self) -> String {
        if self.kind == ArgumentKind::Help {
            "-h, --help".to_owned()
        } else if self.takes_value() {
            format!("{} {}", self.name, self.metavar())
        } else {
            self.name.clone()
        }
    }
}

#[derive(Clone, Debug)]
struct Subcommand {
    name: String,
    help: Option<String>,
    parser: ParserSpec,
}

#[derive(Clone, Debug)]
pub struct ParserSpec {
    pub program: String,
    arguments: Vec<ArgumentSpec>,
    subcommand_destination: Option<String>,
    subcommands: Vec<Subcommand>,
}

#[derive(Debug, PartialEq)]
pub enum ParserOutcome {
    Arguments(Namespace),
    Exit {
        code: i32,
        stdout: PyString,
        stderr: PyString,
    },
}

impl ParserSpec {
    pub fn new(program: &str) -> Self {
        Self {
            program: program.to_owned(),
            arguments: vec![ArgumentSpec {
                name: "--help".to_owned(),
                destination: "help".to_owned(),
                kind: ArgumentKind::Help,
                required: false,
                default: None,
                choices: Vec::new(),
                help: Some("show this help message and exit".to_owned()),
            }],
            subcommand_destination: None,
            subcommands: Vec::new(),
        }
    }

    pub fn argument(&mut self, argument: ArgumentSpec) {
        self.arguments.push(argument);
    }
    pub fn subcommand_destination(&mut self, destination: &str) {
        self.subcommand_destination = Some(destination.to_owned());
    }
    pub fn subcommand(&mut self, name: &str, help: Option<&str>, mut parser: ParserSpec) {
        parser.set_program(&format!("{} {name}", self.program));
        self.subcommands.push(Subcommand {
            name: name.to_owned(),
            help: help.map(str::to_owned),
            parser,
        });
    }

    fn set_program(&mut self, program: &str) {
        self.program = program.to_owned();
        for child in &mut self.subcommands {
            child
                .parser
                .set_program(&format!("{program} {}", child.name));
        }
    }

    pub fn parse(&self, argv: &[PyString], columns: usize) -> ParserOutcome {
        match self.parse_known(argv, columns) {
            Ok((namespace, extras)) if extras.is_empty() => ParserOutcome::Arguments(namespace),
            Ok((_, extras)) => {
                let mut message = text("unrecognized arguments: ");
                append(&mut message, &joined(&extras, " "));
                self.error(message, columns)
            }
            Err(outcome) => outcome,
        }
    }

    fn parse_known(
        &self,
        argv: &[PyString],
        columns: usize,
    ) -> Result<(Namespace, Vec<PyString>), ParserOutcome> {
        let mut namespace = Namespace::default();
        for argument in &self.arguments {
            if let Some(value) = &argument.default {
                namespace.set(&argument.destination, value.clone());
            }
        }
        if let Some(destination) = &self.subcommand_destination {
            namespace.set(destination, ArgumentValue::None);
        }
        // Classification precedes consumption, but ambiguity is raised only when its option is consumed.
        let mut after_separator = false;
        let tokens: Vec<_> = argv
            .iter()
            .map(|token| {
                if after_separator {
                    Token::Positional
                } else if token.equals("--") {
                    after_separator = true;
                    Token::Separator
                } else {
                    self.classify(token)
                }
            })
            .collect();
        let mut seen = vec![false; self.arguments.len()];
        let mut extras = Vec::new();
        let mut child_extras = Vec::new();
        let mut child_seen = false;
        let mut cursor = 0;
        while cursor < argv.len() {
            match &tokens[cursor] {
                Token::Optional(matches) => {
                    if matches.len() > 1 {
                        let mut message = text("ambiguous option: ");
                        append(&mut message, &argv[cursor]);
                        append(
                            &mut message,
                            &text(&format!(
                                " could match {}",
                                matches
                                    .iter()
                                    .map(|option| option.spelling.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )),
                        );
                        return Err(self.error(message, columns));
                    }
                    let option = &matches[0];
                    let Some(index) = option.argument else {
                        extras.push(argv[cursor].clone());
                        cursor += 1;
                        continue;
                    };
                    let argument = &self.arguments[index];
                    seen[index] = true;
                    if argument.takes_value() {
                        let value = if let Some(explicit) = &option.explicit {
                            explicit.clone()
                        } else if matches!(tokens.get(cursor + 1), Some(Token::Positional)) {
                            cursor += 1;
                            argv[cursor].clone()
                        } else {
                            return Err(self.argument_error(
                                argument,
                                text("expected one argument"),
                                columns,
                            ));
                        };
                        let converted = if argument.kind == ArgumentKind::Integer {
                            match crate::compat::integer::coerce(&crate::compat::Value::String(
                                value.clone(),
                            )) {
                                Ok(value) => ArgumentValue::Integer(value),
                                Err(_) => {
                                    let mut message = text("invalid int value: ");
                                    append(&mut message, &crate::compat::repr::string_repr(&value));
                                    return Err(self.argument_error(argument, message, columns));
                                }
                            }
                        } else {
                            ArgumentValue::String(value.clone())
                        };
                        if !argument.choices.is_empty()
                            && !argument.choices.iter().any(|choice| value.equals(choice))
                        {
                            return Err(self.argument_error(
                                argument,
                                invalid_choice(&value, &argument.choices),
                                columns,
                            ));
                        }
                        namespace.set(&argument.destination, converted);
                    } else {
                        if let Some(explicit) = &option.explicit {
                            let short_help_tail = argument.kind == ArgumentKind::Help
                                && option.spelling == "-h"
                                && !option.equals
                                && !explicit.0.is_empty()
                                && explicit.0.first() != Some(&45);
                            if !short_help_tail {
                                let mut message = text("ignored explicit argument ");
                                append(&mut message, &crate::compat::repr::string_repr(explicit));
                                return Err(self.argument_error(argument, message, columns));
                            }
                        }
                        if argument.kind == ArgumentKind::Help {
                            return Err(ParserOutcome::Exit {
                                code: 0,
                                stdout: self.help(columns),
                                stderr: PyString::default(),
                            });
                        }
                        namespace.set(&argument.destination, ArgumentValue::Boolean(true));
                    }
                    cursor += 1;
                }
                Token::Positional | Token::Separator
                    if self.subcommand_destination.is_some() && !child_seen =>
                {
                    let selector = cursor + usize::from(matches!(tokens[cursor], Token::Separator));
                    if selector == argv.len() {
                        extras.push(argv[cursor].clone());
                        cursor += 1;
                        continue;
                    }
                    let destination = self.subcommand_destination.as_deref().unwrap();
                    let name = &argv[selector];
                    let Some(child) = self
                        .subcommands
                        .iter()
                        .find(|child| name.equals(&child.name))
                    else {
                        let choices = self
                            .subcommands
                            .iter()
                            .map(|child| child.name.clone())
                            .collect::<Vec<_>>();
                        let mut message = text(&format!("argument {destination}: "));
                        append(&mut message, &invalid_choice(name, &choices));
                        return Err(self.error(message, columns));
                    };
                    namespace.set(destination, ArgumentValue::String(name.clone()));
                    let (child_namespace, remaining) =
                        child.parser.parse_known(&argv[selector + 1..], columns)?;
                    for (name, value) in child_namespace.fields {
                        namespace.set(&name, value);
                    }
                    child_extras = remaining;
                    child_seen = true;
                    cursor = argv.len();
                }
                _ => {
                    extras.push(argv[cursor].clone());
                    cursor += 1;
                }
            }
        }
        let mut required = self
            .arguments
            .iter()
            .enumerate()
            .filter(|(index, argument)| argument.required && !seen[*index])
            .map(|(_, argument)| argument.action_name().to_owned())
            .collect::<Vec<_>>();
        if !child_seen {
            if let Some(destination) = &self.subcommand_destination {
                required.push(destination.clone());
            }
        }
        if !required.is_empty() {
            return Err(self.error(
                text(&format!(
                    "the following arguments are required: {}",
                    required.join(", ")
                )),
                columns,
            ));
        }
        extras.extend(child_extras);
        Ok((namespace, extras))
    }

    fn classify(&self, token: &PyString) -> Token {
        if token.0.first() != Some(&45) || token.0.len() == 1 {
            return Token::Positional;
        }
        if let Some(index) = self.exact_option(token) {
            return Token::Optional(vec![OptionMatch {
                argument: Some(index),
                spelling: token.utf8().unwrap(),
                explicit: None,
                equals: false,
            }]);
        }
        let equal = token.0.iter().position(|point| *point == 61);
        let option = equal.map_or_else(
            || token.clone(),
            |position| PyString(token.0[..position].to_vec()),
        );
        let explicit = equal.map(|position| PyString(token.0[position + 1..].to_vec()));
        if let Some(index) = self.exact_option(&option) {
            return Token::Optional(vec![OptionMatch {
                argument: Some(index),
                spelling: option.utf8().unwrap(),
                explicit,
                equals: true,
            }]);
        }
        let mut matches = Vec::new();
        if token.0.starts_with(&[45, 45]) {
            for (index, argument) in self.arguments.iter().enumerate() {
                if PyString::from(argument.name.as_str())
                    .0
                    .starts_with(&option.0)
                {
                    matches.push(OptionMatch {
                        argument: Some(index),
                        spelling: argument.name.clone(),
                        explicit: explicit.clone(),
                        equals: equal.is_some(),
                    });
                }
            }
        } else if token.0.starts_with(&[45, 104]) {
            matches.push(OptionMatch {
                argument: Some(0),
                spelling: "-h".to_owned(),
                explicit: Some(PyString(token.0[2..].to_vec())),
                equals: false,
            });
        }
        if !matches.is_empty() {
            return Token::Optional(matches);
        }
        if negative_number(token) || token.0.contains(&32) {
            Token::Positional
        } else {
            Token::Optional(vec![OptionMatch {
                argument: None,
                spelling: String::new(),
                explicit: None,
                equals: false,
            }])
        }
    }

    fn exact_option(&self, token: &PyString) -> Option<usize> {
        if token.equals("-h") {
            Some(0)
        } else {
            self.arguments
                .iter()
                .position(|argument| token.equals(&argument.name))
        }
    }

    fn argument_error(
        &self,
        argument: &ArgumentSpec,
        detail: PyString,
        columns: usize,
    ) -> ParserOutcome {
        let mut message = text(&format!("argument {}: ", argument.action_name()));
        append(&mut message, &detail);
        self.error(message, columns)
    }

    fn error(&self, message: PyString, columns: usize) -> ParserOutcome {
        let mut stderr = text(&self.usage(columns));
        append(&mut stderr, &text(&format!("{}: error: ", self.program)));
        append(&mut stderr, &message);
        append(&mut stderr, &text("\n"));
        ParserOutcome::Exit {
            code: 2,
            stdout: PyString::default(),
            stderr,
        }
    }

    fn positional_choices(&self) -> String {
        format!(
            "{{{}}}",
            self.subcommands
                .iter()
                .map(|child| child.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn usage_parts(&self) -> (Vec<String>, Vec<String>) {
        let mut options = Vec::new();
        for argument in &self.arguments {
            let name = if argument.kind == ArgumentKind::Help {
                "-h"
            } else {
                &argument.name
            };
            if !argument.required {
                let invocation = if argument.takes_value() {
                    format!("{name} {}", argument.metavar())
                } else {
                    name.to_owned()
                };
                options.push(format!("[{invocation}]"));
            } else {
                options.push(name.to_owned());
                if argument.takes_value() {
                    options.push(argument.metavar());
                }
            }
        }
        let positionals = if self.subcommand_destination.is_some() {
            vec![self.positional_choices(), "...".to_owned()]
        } else {
            Vec::new()
        };
        (options, positionals)
    }

    pub fn usage(&self, columns: usize) -> String {
        let width = columns.saturating_sub(2);
        let prefix = "usage: ";
        let (options, positionals) = self.usage_parts();
        let all = options
            .iter()
            .chain(&positionals)
            .cloned()
            .collect::<Vec<_>>();
        let usage = if all.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, all.join(" "))
        };
        if prefix.len() + usage.len() <= width {
            return format!("{prefix}{usage}\n");
        }
        let mut lines;
        if (prefix.len() + self.program.len()) * 4 <= width * 3 {
            let indent = prefix.len() + self.program.len() + 1;
            let mut first = vec![self.program.clone()];
            if options.is_empty() {
                first.extend(positionals);
                lines = wrap_usage(&first, indent, Some(prefix.len()), width);
            } else {
                first.extend(options);
                lines = wrap_usage(&first, indent, Some(prefix.len()), width);
                lines.extend(wrap_usage(&positionals, indent, None, width));
            }
        } else {
            let indent = prefix.len();
            lines = wrap_usage(&all, indent, None, width);
            if lines.len() > 1 {
                lines = wrap_usage(&options, indent, None, width);
                lines.extend(wrap_usage(&positionals, indent, None, width));
            }
            lines.insert(0, self.program.clone());
        }
        format!("{prefix}{}\n", lines.join("\n"))
    }

    pub fn help(&self, columns: usize) -> PyString {
        let width = columns.saturating_sub(2);
        let mut rows = Vec::new();
        if self.subcommand_destination.is_some() {
            rows.push(HelpRow {
                invocation: self.positional_choices(),
                help: None,
                indent: 2,
            });
            for child in &self.subcommands {
                if let Some(help) = &child.help {
                    rows.push(HelpRow {
                        invocation: child.name.clone(),
                        help: Some(help.clone()),
                        indent: 4,
                    });
                }
            }
        }
        let positional_count = rows.len();
        for argument in &self.arguments {
            rows.push(HelpRow {
                invocation: argument.invocation(),
                help: argument.help.clone(),
                indent: 2,
            });
        }
        let action_max = rows
            .iter()
            .map(|row| row.invocation.len() + row.indent)
            .max()
            .unwrap_or(0);
        let help_position = (action_max + 2).min(24.min(width.saturating_sub(20).max(4)));
        let mut result = self.usage(columns);
        if positional_count != 0 {
            result.push_str("\npositional arguments:\n");
            for row in &rows[..positional_count] {
                format_help_row(&mut result, row, help_position, width);
            }
        }
        result.push_str("\noptions:\n");
        for row in &rows[positional_count..] {
            format_help_row(&mut result, row, help_position, width);
        }
        text(&result)
    }
}

struct OptionMatch {
    argument: Option<usize>,
    spelling: String,
    explicit: Option<PyString>,
    equals: bool,
}
enum Token {
    Positional,
    Separator,
    Optional(Vec<OptionMatch>),
}

fn invalid_choice(value: &PyString, choices: &[String]) -> PyString {
    let mut result = text("invalid choice: ");
    append(&mut result, &crate::compat::repr::string_repr(value));
    append(
        &mut result,
        &text(&format!(" (choose from {})", choices.join(", "))),
    );
    result
}

fn wrap_usage(
    parts: &[String],
    indent: usize,
    prefix_length: Option<usize>,
    width: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut length = prefix_length.unwrap_or(indent) as i128 - 1;
    for part in parts {
        if length + 1 + part.len() as i128 > width as i128 && !line.is_empty() {
            lines.push(format!("{}{}", " ".repeat(indent), line.join(" ")));
            line.clear();
            length = indent as i128 - 1;
        }
        line.push(part.clone());
        length += part.len() as i128 + 1;
    }
    if !line.is_empty() {
        lines.push(format!("{}{}", " ".repeat(indent), line.join(" ")));
    }
    if prefix_length.is_some() {
        if let Some(first) = lines.first_mut() {
            first.drain(..indent);
        }
    }
    lines
}

struct HelpRow {
    invocation: String,
    help: Option<String>,
    indent: usize,
}

fn format_help_row(output: &mut String, row: &HelpRow, help_position: usize, width: usize) {
    output.push_str(&" ".repeat(row.indent));
    output.push_str(&row.invocation);
    let Some(help) = row.help.as_deref().filter(|help| !help.is_empty()) else {
        output.push('\n');
        return;
    };
    let action_width = help_position.saturating_sub(row.indent + 2);
    let inline = row.invocation.len() <= action_width;
    if inline {
        output.push_str(&" ".repeat(help_position - row.indent - row.invocation.len()));
    } else {
        output.push('\n');
        output.push_str(&" ".repeat(help_position));
    }
    let help_width = width.saturating_sub(help_position).max(11);
    for (index, line) in wrap_help(help, help_width).iter().enumerate() {
        if index != 0 {
            output.push('\n');
            output.push_str(&" ".repeat(help_position));
        }
        output.push_str(line);
    }
    output.push('\n');
}

fn help_space(point: char) -> bool {
    matches!(point, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}')
}

// Parser descriptions are fixed ASCII source strings. Input argv never enters this wrapper.
fn help_chunks(help: &str) -> Vec<String> {
    let mut normalized = String::new();
    let mut space = false;
    for point in help.chars() {
        if help_space(point) {
            space = true;
        } else {
            if space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(point);
            space = false;
        }
    }
    let points = normalized.chars().collect::<Vec<_>>();
    let letter = |index: usize| points.get(index).is_some_and(char::is_ascii_alphabetic);
    let word = |point: char| point.is_ascii_alphanumeric() || point == '_';
    let punctuation =
        |point: char| word(point) || matches!(point, '!' | '"' | '\'' | '&' | '.' | ',' | '?');
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < points.len() {
        let mut end = start + 1;
        if points[start] == ' ' {
            while points.get(end) == Some(&' ') {
                end += 1;
            }
        } else {
            let mut dash_end = start;
            while points.get(dash_end) == Some(&'-') {
                dash_end += 1;
            }
            if dash_end >= start + 2
                && start > 0
                && punctuation(points[start - 1])
                && points.get(dash_end).copied().is_some_and(word)
            {
                end = dash_end;
            } else {
                while end < points.len() {
                    if points[end] == ' ' {
                        break;
                    }
                    if points[end - 1] == '-' {
                        let behind = (end >= 3 && letter(end - 3) && letter(end - 2))
                            || (end >= 4
                                && letter(end - 4)
                                && points[end - 3] == '-'
                                && letter(end - 2));
                        let ahead = letter(end)
                            && (letter(end + 1)
                                || (points.get(end + 1) == Some(&'-') && letter(end + 2)));
                        if behind && ahead {
                            break;
                        }
                    }
                    if punctuation(points[end - 1])
                        && points[end] == '-'
                        && points.get(end + 1) == Some(&'-')
                    {
                        let mut after = end + 2;
                        while points.get(after) == Some(&'-') {
                            after += 1;
                        }
                        if points.get(after).copied().is_some_and(word) {
                            break;
                        }
                    }
                    end += 1;
                }
            }
        }
        chunks.push(points[start..end].iter().collect());
        start = end;
    }
    chunks
}

fn wrap_help(help: &str, width: usize) -> Vec<String> {
    let mut chunks = std::collections::VecDeque::from(help_chunks(help));
    let mut lines = Vec::new();
    while !chunks.is_empty() {
        if !lines.is_empty()
            && chunks.front().is_some_and(|chunk| {
                chunk
                    .chars()
                    .all(|point| crate::compat::unicode::whitespace(point as u32))
            })
        {
            chunks.pop_front();
        }
        let mut line = String::new();
        while chunks
            .front()
            .is_some_and(|chunk| line.len() + chunk.len() <= width)
        {
            line.push_str(&chunks.pop_front().unwrap());
        }
        if chunks.front().is_some_and(|chunk| chunk.len() > width) {
            let chunk = chunks.front_mut().unwrap();
            let space = width.saturating_sub(line.len());
            let mut end = space;
            if let Some(hyphen) = chunk[..space].rfind('-') {
                if hyphen > 0 && chunk[..hyphen].chars().any(|point| point != '-') {
                    end = hyphen + 1;
                }
            }
            line.push_str(&chunk[..end]);
            chunk.drain(..end);
        }
        let length = line
            .trim_end_matches(|point: char| crate::compat::unicode::whitespace(point as u32))
            .len();
        line.truncate(length);
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}
