use super::errors::{container_equal, dictionary_items, require_hashable, ValidationResult};
use crate::compat::Value;
use crate::compat::{self, JsonError, PyString, SerializationProfile};

#[derive(Clone, Debug)]
pub enum DictionaryValue {
    Value(Value),
    Dictionary(PythonDictionary),
    List(Vec<DictionaryValue>),
}

/// An ordered owned dictionary for private APIs whose keys need not be strings.
#[derive(Clone, Debug, Default)]
pub struct PythonDictionary {
    pub entries: Vec<(Value, DictionaryValue)>,
}

impl PythonDictionary {
    pub fn coerce(value: &Value) -> ValidationResult<Self> {
        Ok(Self {
            entries: dictionary_items(value)?
                .into_iter()
                .map(|(key, value)| (key, DictionaryValue::Value(value)))
                .collect(),
        })
    }

    pub fn insert(&mut self, key: Value, value: DictionaryValue) -> ValidationResult<()> {
        require_hashable(&key)?;
        if let Some((_, previous)) = self
            .entries
            .iter_mut()
            .find(|(candidate, _)| container_equal(candidate, &key))
        {
            *previous = value;
        } else {
            self.entries.push((key, value));
        }
        Ok(())
    }

    pub fn set(&mut self, key: &str, value: impl Into<DictionaryValue>) {
        // The caller supplies a Rust string, so this key is always hashable.
        self.insert(key.into(), value.into())
            .expect("a string dictionary key is hashable");
    }

    pub fn get(&self, key: &str) -> Option<&DictionaryValue> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate.is_text(key))
            .map(|(_, value)| value)
    }

    pub fn get_key_mut(&mut self, key: &Value) -> ValidationResult<Option<&mut DictionaryValue>> {
        require_hashable(key)?;
        Ok(self
            .entries
            .iter_mut()
            .find(|(candidate, _)| container_equal(candidate, key))
            .map(|(_, value)| value))
    }

    /// Validated task documents have string keys; general private results retain their entries.
    pub fn string_keyed_value(&self) -> Option<Value> {
        let mut entries = Vec::new();
        for (key, value) in &self.entries {
            let key = key.as_string()?.clone();
            let value = value.string_keyed_value()?;
            entries.push((key, value));
        }
        Some(Value::Object(entries))
    }

    pub fn serialize_insertion_order(
        &self,
        ensure_ascii: bool,
        spaced: bool,
    ) -> Result<Vec<u8>, JsonError> {
        let profile = SerializationProfile {
            ensure_ascii,
            spaced,
            sorted: false,
        };
        self.serialize(profile)
    }

    pub fn serialize(&self, profile: SerializationProfile) -> Result<Vec<u8>, JsonError> {
        let mut output = PyString::default();
        self.append_json(&mut output, profile)?;
        compat::encode_utf8(&output)
    }

    fn append_json(
        &self,
        output: &mut PyString,
        profile: SerializationProfile,
    ) -> Result<(), JsonError> {
        let order = if profile.sorted {
            let mut order = super::python_sort::identity_indices(self.entries.len())
                .map_err(serialization_failure)?;
            super::python_sort::sort_indices(&mut order, |left, right| {
                super::python_operations::less(&self.entries[left].0, &self.entries[right].0)
            })
            .map_err(serialization_failure)?;
            Some(order)
        } else {
            None
        };
        output.0.push(123);
        for index in 0..self.entries.len() {
            let (key, value) = &self.entries[order.as_ref().map_or(index, |order| order[index])];
            if index != 0 {
                output
                    .0
                    .extend(PyString::from(if profile.spaced { ", " } else { "," }).0);
            }
            let key = match key {
                Value::String(_) => key.clone(),
                Value::Integer(_)
                | Value::Bool(_)
                | Value::Null
                | Value::Float(_)
                | Value::ConstantNaN => Value::String(compat::dumps_text(key, profile)?),
                _ => {
                    return Err(JsonError {
                        kind: "TypeError",
                        message: format!(
                            "keys must be str, int, float, bool or None, not {}",
                            key.python_type()
                        ),
                        position: 0,
                    })
                }
            };
            output.0.extend(compat::dumps_text(&key, profile)?.0);
            output
                .0
                .extend(PyString::from(if profile.spaced { ": " } else { ":" }).0);
            value.append_json(output, profile)?;
        }
        output.0.push(125);
        Ok(())
    }
}

fn serialization_failure(error: super::errors::ValidationFailure) -> JsonError {
    let (kind, message) = match error {
        super::errors::ValidationFailure::Python { kind, message } => (kind, message),
        super::errors::ValidationFailure::Contract(error) => {
            ("ValueError", error.to_string().into())
        }
    };
    match compat::encode_utf8(&message) {
        Ok(bytes) => JsonError {
            kind,
            message: String::from_utf8(bytes).expect("the UTF-8 encoder returns UTF-8"),
            position: 0,
        },
        Err(error) => error,
    }
}

impl From<Value> for DictionaryValue {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<PythonDictionary> for DictionaryValue {
    fn from(value: PythonDictionary) -> Self {
        Self::Dictionary(value)
    }
}

impl DictionaryValue {
    fn string_keyed_value(&self) -> Option<Value> {
        match self {
            Self::Value(value) => Some(value.clone()),
            Self::Dictionary(value) => value.string_keyed_value(),
            Self::List(values) => Some(Value::List(
                values
                    .iter()
                    .map(Self::string_keyed_value)
                    .collect::<Option<Vec<_>>>()?,
            )),
        }
    }

    fn append_json(
        &self,
        output: &mut PyString,
        profile: SerializationProfile,
    ) -> Result<(), JsonError> {
        match self {
            Self::Value(value) => output.0.extend(compat::dumps_text(value, profile)?.0),
            Self::Dictionary(dictionary) => dictionary.append_json(output, profile)?,
            Self::List(values) => {
                output.0.push(91);
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        output
                            .0
                            .extend(PyString::from(if profile.spaced { ", " } else { "," }).0);
                    }
                    value.append_json(output, profile)?;
                }
                output.0.push(93);
            }
        }
        Ok(())
    }
}
