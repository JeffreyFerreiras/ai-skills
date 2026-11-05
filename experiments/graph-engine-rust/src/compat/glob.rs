use super::PyString;

enum Atom {
    Literal(u32),
    Any,
    Star {
        nonempty: bool,
    },
    Class {
        ranges: Vec<(u32, u32)>,
        negated: bool,
    },
}

fn character_class(pattern: &[u32], start: usize) -> Option<(Atom, usize)> {
    let mut end = start + 1;
    if pattern.get(end) == Some(&33) {
        end += 1;
    }
    if pattern.get(end) == Some(&93) {
        end += 1;
    }
    while end < pattern.len() && pattern[end] != 93 {
        end += 1;
    }
    if end == pattern.len() {
        return None;
    }
    let mut index = start + 1;
    let negated = pattern.get(index) == Some(&33);
    if negated {
        index += 1;
    }
    let mut ranges = Vec::new();
    while index < end {
        if index + 2 < end && pattern[index + 1] == 45 {
            if pattern[index] <= pattern[index + 2] {
                ranges.push((pattern[index], pattern[index + 2]));
            }
            index += 3;
        } else {
            ranges.push((pattern[index], pattern[index]));
            index += 1;
        }
    }
    Some((Atom::Class { ranges, negated }, end + 1))
}

fn append_atoms(pattern: &[u32], nonempty_star: bool, atoms: &mut Vec<Atom>) {
    let mut index = 0;
    while index < pattern.len() {
        let atom = match pattern[index] {
            42 => Atom::Star {
                nonempty: nonempty_star,
            },
            63 => Atom::Any,
            91 => {
                if let Some((atom, end)) = character_class(pattern, index) {
                    atoms.push(atom);
                    index = end;
                    continue;
                }
                Atom::Literal(91)
            }
            point => Atom::Literal(point),
        };
        atoms.push(atom);
        index += 1;
    }
}

fn matches(text: &[u32], atoms: &[Atom], path_lines: bool, search: bool) -> bool {
    let mut next = vec![false; atoms.len() + 1];
    let mut found = false;
    for i in (0..=text.len()).rev() {
        let mut current = vec![false; atoms.len() + 1];
        current[atoms.len()] = i == text.len();
        let any = i < text.len() && (!path_lines || text[i] != 10);
        for j in (0..atoms.len()).rev() {
            current[j] = match &atoms[j] {
                Atom::Star { nonempty: true } => any && (next[j] || next[j + 1]),
                Atom::Star { nonempty: false } => current[j + 1] || (any && next[j]),
                Atom::Any => any && next[j + 1],
                Atom::Literal(point) => i < text.len() && text[i] == *point && next[j + 1],
                Atom::Class { ranges, negated } => {
                    if i == text.len() {
                        false
                    } else if *negated && ranges.is_empty() {
                        // fnmatch translates an empty negated range to a regex dot.
                        any && next[j + 1]
                    } else {
                        let included = ranges
                            .iter()
                            .any(|&(low, high)| low <= text[i] && text[i] <= high);
                        (included != *negated) && next[j + 1]
                    }
                }
            };
        }
        if current[0] && (i == 0 || (search && text[i - 1] == 10)) {
            found = true;
        }
        next = current;
    }
    found
}

pub fn fnmatchcase(text: &PyString, pattern: &PyString) -> bool {
    let mut atoms = Vec::new();
    append_atoms(&pattern.0, false, &mut atoms);
    matches(&text.0, &atoms, false, false)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlobError {
    EmptyPattern,
}

fn posix_lines(path: &PyString) -> Vec<u32> {
    let mut lines = Vec::new();
    if path.starts_with("/") {
        lines.push(10);
        if path.starts_with("//") && !path.starts_with("///") {
            lines.push(10);
        }
    }
    let mut first = true;
    for part in path
        .0
        .split(|&point| point == 47)
        .filter(|part| !part.is_empty() && *part != [46])
    {
        if !first {
            lines.push(10);
        }
        first = false;
        lines.extend(
            part.iter()
                .map(|&point| if point == 10 { 47 } else { point }),
        );
    }
    lines
}

fn line_break(point: u32) -> bool {
    matches!(point, 10..=13 | 28..=30 | 133 | 0x2028 | 0x2029)
}

pub fn posix_path_match(text: &PyString, pattern: &PyString) -> Result<bool, GlobError> {
    let pattern_lines = posix_lines(pattern);
    if pattern_lines.is_empty() {
        return Err(GlobError::EmptyPattern);
    }
    let mut atoms = Vec::new();
    let mut start = 0;
    while start < pattern_lines.len() {
        let mut end = start;
        while end < pattern_lines.len() && !line_break(pattern_lines[end]) {
            end += 1;
        }
        if end < pattern_lines.len() {
            end += 1;
            if pattern_lines[end - 1] == 13 && pattern_lines.get(end) == Some(&10) {
                end += 1;
            }
        }
        let part = &pattern_lines[start..end];
        // CPython's compiler treats a component exactly '*' as one-or-more.
        // Other star spellings retain fnmatch's zero-or-more semantics.
        append_atoms(part, part == [42] || part == [42, 10], &mut atoms);
        start = end;
    }
    Ok(matches(
        &posix_lines(text),
        &atoms,
        true,
        !pattern.starts_with("/"),
    ))
}
