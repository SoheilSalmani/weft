use std::fmt;

use serde::de::{self, Deserializer, SeqAccess};
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};

use crate::id::AnswerId;
use crate::question::StarlarkExpr;

/// One piece of a path or content line. After `commit` abstraction, concrete
/// literals that matched answers become `Answer` references.
///
/// Serde forms: a plain JSON string is a `Literal`; `{"answer": "id"}` and
/// `{"expr": "..."}` are the abstracted variants. This keeps fixture patches
/// hand-writable while staying unambiguous.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Literal(String),
    Answer(AnswerId),
    Expr(StarlarkExpr),
}

impl Serialize for Segment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Segment::Literal(s) => serializer.serialize_str(s),
            Segment::Answer(id) => {
                use serde::ser::SerializeMap;
                let mut m = serializer.serialize_map(Some(1))?;
                m.serialize_entry("answer", id)?;
                m.end()
            }
            Segment::Expr(e) => {
                use serde::ser::SerializeMap;
                let mut m = serializer.serialize_map(Some(1))?;
                m.serialize_entry("expr", e)?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Segment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = Segment;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string, {\"answer\": id}, or {\"expr\": source}")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Segment, E> {
                Ok(Segment::Literal(v.to_owned()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Segment, E> {
                Ok(Segment::Literal(v))
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Segment, A::Error> {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| de::Error::custom("empty segment object"))?;
                let seg = match key.as_str() {
                    "answer" => Segment::Answer(map.next_value()?),
                    "expr" => Segment::Expr(map.next_value()?),
                    other => {
                        return Err(de::Error::custom(format!(
                            "unknown segment key {other:?} (expected \"answer\" or \"expr\")"
                        )))
                    }
                };
                if map.next_key::<String>()?.is_some() {
                    return Err(de::Error::custom(
                        "segment object must have exactly one key",
                    ));
                }
                Ok(seg)
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// One line of file content, as a sequence of segments (no newline included).
///
/// Serde: a fully-literal line is a plain string; mixed lines are arrays of
/// segments.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Line(pub Vec<Segment>);

impl Line {
    pub fn literal(s: &str) -> Self {
        Line(vec![Segment::Literal(s.to_owned())])
    }

    /// If the whole line is literal, return its text.
    pub fn as_literal(&self) -> Option<String> {
        let mut out = String::new();
        for seg in &self.0 {
            match seg {
                Segment::Literal(s) => out.push_str(s),
                _ => return None,
            }
        }
        Some(out)
    }
}

impl Serialize for Line {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Canonical compact form: plain string when fully literal.
        if let Some(text) = self.as_literal() {
            serializer.serialize_str(&text)
        } else {
            let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
            for seg in &self.0 {
                seq.serialize_element(seg)?;
            }
            seq.end()
        }
    }
}

impl<'de> Deserialize<'de> for Line {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = Line;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string or an array of segments")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Line, E> {
                Ok(Line::literal(v))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Line, E> {
                Ok(Line(vec![Segment::Literal(v)]))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Line, A::Error> {
                let mut segs = Vec::new();
                while let Some(seg) = seq.next_element()? {
                    segs.push(seg);
                }
                Ok(Line(segs))
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// File content: line-granular segments. Rendered files are normalized to end
/// with exactly one trailing newline (empty content renders to an empty file).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Content(pub Vec<Line>);

impl Content {
    /// Build fully-literal content from concrete text.
    pub fn from_text(text: &str) -> Self {
        Content(text.lines().map(Line::literal).collect())
    }

    /// If the whole content is literal, return its normalized text.
    pub fn as_literal_text(&self) -> Option<String> {
        let lines: Option<Vec<String>> = self.0.iter().map(Line::as_literal).collect();
        lines.map(|ls| join_lines(&ls))
    }
}

/// Join rendered lines into normalized file text (trailing newline, empty
/// content stays empty).
pub fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        let mut s = lines.join("\n");
        s.push('\n');
        s
    }
}

/// A path inside the rendered tree; segments may embed answer references.
///
/// Serde: a fully-literal path is a plain string; abstracted paths are arrays
/// of segments. Rendered paths are `/`-separated, relative, and must not
/// escape the tree root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplatePath(pub Vec<Segment>);

impl TemplatePath {
    pub fn literal(s: &str) -> Self {
        TemplatePath(vec![Segment::Literal(s.to_owned())])
    }

    pub fn as_literal(&self) -> Option<String> {
        let mut out = String::new();
        for seg in &self.0 {
            match seg {
                Segment::Literal(s) => out.push_str(s),
                _ => return None,
            }
        }
        Some(out)
    }
}

impl Serialize for TemplatePath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(text) = self.as_literal() {
            serializer.serialize_str(&text)
        } else {
            let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
            for seg in &self.0 {
                seq.serialize_element(seg)?;
            }
            seq.end()
        }
    }
}

impl<'de> Deserialize<'de> for TemplatePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = TemplatePath;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string or an array of segments")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<TemplatePath, E> {
                Ok(TemplatePath::literal(v))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<TemplatePath, E> {
                Ok(TemplatePath(vec![Segment::Literal(v)]))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<TemplatePath, A::Error> {
                let mut segs = Vec::new();
                while let Some(seg) = seq.next_element()? {
                    segs.push(seg);
                }
                Ok(TemplatePath(segs))
            }
        }
        deserializer.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_line_serializes_as_string() {
        let line = Line::literal("hello world");
        assert_eq!(serde_json::to_string(&line).unwrap(), r#""hello world""#);
    }

    #[test]
    fn mixed_line_serializes_as_array() {
        let line = Line(vec![
            Segment::Literal("Project: ".into()),
            Segment::Answer("project_name".into()),
        ]);
        let json = serde_json::to_string(&line).unwrap();
        assert_eq!(json, r#"["Project: ",{"answer":"project_name"}]"#);
        let back: Line = serde_json::from_str(&json).unwrap();
        assert_eq!(back, line);
    }

    #[test]
    fn content_from_text_round_trips() {
        let c = Content::from_text("a\nb\n");
        assert_eq!(c.as_literal_text().unwrap(), "a\nb\n");
        // no trailing newline in input gets normalized
        let c2 = Content::from_text("a\nb");
        assert_eq!(c2.as_literal_text().unwrap(), "a\nb\n");
        assert_eq!(Content::from_text("").as_literal_text().unwrap(), "");
    }

    #[test]
    fn segment_rejects_unknown_key() {
        let err = serde_json::from_str::<Segment>(r#"{"bogus": "x"}"#).unwrap_err();
        assert!(err.to_string().contains("unknown segment key"));
    }
}
