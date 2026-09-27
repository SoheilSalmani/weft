use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer};
use serde::ser::{self, Serializer};
use serde::{Deserialize, Serialize};

use crate::id::AnswerId;

/// A concrete answer value.
///
/// `Secret` values are resolved at render time and are **never serialized**:
/// attempting to serialize one is an error, which is what guarantees secrets
/// can't leak into answer files, state files, or patches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    String(String),
    Bool(bool),
    Int(i64),
    /// An ordered list of values, produced by multi-choice questions and
    /// list-valued expressions. Lists are never written directly into file
    /// content — project them through an expression (e.g. join) first.
    List(Vec<Value>),
    Secret(SecretValue),
}

/// A resolved secret. Debug/Display redact the value.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Expose the secret for rendering. Call sites should be few and audited.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(<redacted>)")
    }
}

impl Value {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::List(_) => "list",
            Value::Secret(_) => "secret",
        }
    }

    /// Whether this value is a list. Rendering a list into file content is an
    /// error (see `render_segment`); callers project lists via expressions.
    pub fn is_list(&self) -> bool {
        matches!(self, Value::List(_))
    }

    /// Concrete text used when substituting this value into file content, or
    /// for human/agent display. Lists render as a `, `-joined display form;
    /// the content path rejects lists before reaching here.
    pub fn render_text(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Bool(b) => if *b { "True" } else { "False" }.to_owned(),
            Value::Int(i) => i.to_string(),
            Value::List(items) => items
                .iter()
                .map(Value::render_text)
                .collect::<Vec<_>>()
                .join(", "),
            Value::Secret(s) => s.expose().to_owned(),
        }
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::String(s) => serializer.serialize_str(s),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int(i) => serializer.serialize_i64(*i),
            Value::List(items) => serializer.collect_seq(items),
            Value::Secret(_) => Err(ser::Error::custom(
                "refusing to serialize a secret value; store the secret *reference* instead",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = Value;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string, bool, integer, or list")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
                Ok(Value::String(v.to_owned()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
                Ok(Value::String(v))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
                Ok(Value::Int(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
                i64::try_from(v)
                    .map(Value::Int)
                    .map_err(|_| E::custom("integer answer out of range"))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element::<Value>()? {
                    items.push(item);
                }
                Ok(Value::List(items))
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// Concrete values for some subset of questions. Partial by design —
/// presets are just `AnswerSet`s.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnswerSet(pub BTreeMap<AnswerId, Value>);

impl AnswerSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: &AnswerId) -> Option<&Value> {
        self.0.get(id)
    }

    pub fn contains(&self, id: &AnswerId) -> bool {
        self.0.contains_key(id)
    }

    pub fn insert(&mut self, id: AnswerId, value: Value) -> Option<Value> {
        self.0.insert(id, value)
    }

    /// Insert, erroring if the key already exists with a *different* value.
    /// Used when building a single precedence layer, where duplicate keys are
    /// conflicts rather than overrides.
    pub fn insert_strict(&mut self, id: AnswerId, value: Value) -> Result<(), LayerConflict> {
        match self.0.get(&id) {
            Some(existing) if *existing != value => Err(LayerConflict { id }),
            _ => {
                self.0.insert(id, value);
                Ok(())
            }
        }
    }

    /// Overlay `other` on top of `self` (later wins).
    pub fn overlay(&mut self, other: &AnswerSet) {
        for (k, v) in &other.0 {
            self.0.insert(k.clone(), v.clone());
        }
    }

    pub fn remove(&mut self, id: &AnswerId) -> Option<Value> {
        self.0.remove(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AnswerId, &Value)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl FromIterator<(AnswerId, Value)> for AnswerSet {
    fn from_iter<T: IntoIterator<Item = (AnswerId, Value)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Ordered precedence layers, lowest first. Later layers win; duplicates
/// *within* a layer must already have been rejected via `insert_strict`.
pub fn layer(layers: &[AnswerSet]) -> AnswerSet {
    let mut out = AnswerSet::new();
    for l in layers {
        out.overlay(l);
    }
    out
}

#[derive(Debug, thiserror::Error)]
#[error("conflicting values for answer {id} at the same precedence level")]
pub struct LayerConflict {
    pub id: AnswerId,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> AnswerId {
        AnswerId::from(s)
    }

    #[test]
    fn secret_refuses_serialization() {
        let v = Value::Secret(SecretValue::new("hunter2".into()));
        let err = serde_json::to_string(&v).unwrap_err();
        assert!(err.to_string().contains("refusing to serialize a secret"));
    }

    #[test]
    fn secret_redacted_in_debug() {
        let v = Value::Secret(SecretValue::new("hunter2".into()));
        assert!(!format!("{v:?}").contains("hunter2"));
    }

    #[test]
    fn layering_later_wins() {
        let base: AnswerSet = [(id("a"), Value::Int(1)), (id("b"), Value::Int(2))]
            .into_iter()
            .collect();
        let over: AnswerSet = [(id("b"), Value::Int(20))].into_iter().collect();
        let merged = layer(&[base, over]);
        assert_eq!(merged.get(&id("a")), Some(&Value::Int(1)));
        assert_eq!(merged.get(&id("b")), Some(&Value::Int(20)));
    }

    #[test]
    fn list_serde_round_trip() {
        let v = Value::List(vec![Value::String("a".into()), Value::String("b".into())]);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"["a","b"]"#);
        let back: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn list_render_text_is_comma_joined_display() {
        let v = Value::List(vec![Value::String("x".into()), Value::String("y".into())]);
        assert_eq!(v.render_text(), "x, y");
        assert!(v.is_list());
    }

    #[test]
    fn strict_insert_conflicts_on_different_value() {
        let mut set = AnswerSet::new();
        set.insert_strict(id("a"), Value::Bool(true)).unwrap();
        set.insert_strict(id("a"), Value::Bool(true)).unwrap(); // same value ok
        assert!(set.insert_strict(id("a"), Value::Bool(false)).is_err());
    }
}
