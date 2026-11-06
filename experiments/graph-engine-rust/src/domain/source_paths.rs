use crate::compat::{unicode, PyString};

pub fn contains(root: &PyString, path: &PyString, windows: bool) -> bool {
    fn key(value: &PyString, windows: bool) -> PyString {
        let end = value
            .0
            .iter()
            .rposition(|point| *point != 47)
            .map_or(0, |index| index + 1);
        let value = PyString(value.0[..end].to_vec());
        let mut value = if windows {
            unicode::lower(&value)
        } else {
            value
        };
        for point in &mut value.0 {
            if *point == 92 {
                *point = 47;
            }
        }
        value
    }
    let root_key = key(root, windows);
    let path_key = key(path, windows);
    path_key == root_key
        || (root.ends_with("/") && path_key.0.starts_with(&[root_key.0, vec![47]].concat()))
}
