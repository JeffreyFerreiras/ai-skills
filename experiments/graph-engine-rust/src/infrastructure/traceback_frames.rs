use super::command_line::python_string;
use crate::compat::PyString;
use std::path::Path;

pub(super) fn append(output: &mut PyString, text: impl Into<PyString>) {
    output.0.extend(text.into().0);
}

pub(super) fn frame(
    output: &mut PyString,
    path: &Path,
    line: u16,
    function: &str,
    source: &str,
    highlight: Option<&str>,
) {
    append(output, "  File \"");
    append(output, python_string(path.as_os_str()));
    append(
        output,
        format!("\", line {line}, in {function}\n    {source}\n"),
    );
    if let Some(expression) = highlight {
        underline(output, source, expression, 0);
    }
}

pub(super) fn underline(output: &mut PyString, source: &str, expression: &str, receiver: usize) {
    let offset = source.find(expression).expect("frozen source expression");
    append(
        output,
        format!(
            "    {}{}{}\n",
            " ".repeat(offset),
            "~".repeat(receiver),
            "^".repeat(expression.len() - receiver)
        ),
    );
}
