//! `weft schema`: emit JSON Schemas for the on-disk template formats so any
//! editor (VSCode json.schemas, taplo/Even Better TOML) gets validation,
//! completion, and hover docs with no plugin.
//!
//! These are hand-mirrored schema structs rather than derives on the real
//! types: core/engine stay free of schemars, and the mirrors can document
//! the *file* formats (name-based deps, segment shorthands) rather than the
//! in-memory model.

#![allow(dead_code)] // mirrors exist for schema generation + validation, not field access

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use schemars::JsonSchema;
use serde::Deserialize;

/// One piece of a path or content line.
#[derive(JsonSchema, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Segment {
    /// Literal text.
    Literal(String),
    /// Substitute an answer's value at render time.
    Answer {
        /// The question id whose value is substituted.
        answer: String,
    },
    /// Evaluate a Starlark expression at render time.
    Expr {
        /// Starlark source; answers are in scope as variables.
        expr: String,
    },
}

/// One line of file content: a plain string when fully literal, or an array
/// of segments.
#[derive(JsonSchema, Deserialize)]
#[serde(untagged)]
pub enum Line {
    Literal(String),
    Segments(Vec<Segment>),
}

/// A path inside the rendered tree: a plain string when fully literal, or an
/// array of segments. Relative, `/`-separated, no `..`.
#[derive(JsonSchema, Deserialize)]
#[serde(untagged)]
pub enum TemplatePath {
    Literal(String),
    Segments(Vec<Segment>),
}

/// A context-anchored change: `context_before + removed + context_after`
/// must match exactly once in the rendered file; `removed` is replaced with
/// `added`. An entirely empty pattern appends at end of file.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hunk {
    #[serde(default)]
    pub context_before: Vec<Line>,
    #[serde(default)]
    pub removed: Vec<Line>,
    #[serde(default)]
    pub added: Vec<Line>,
    #[serde(default)]
    pub context_after: Vec<Line>,
}

/// A single tree operation, applied in order.
#[derive(JsonSchema, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    /// Create a file (errors if it already exists).
    CreateFile {
        path: TemplatePath,
        /// File content as an array of lines.
        content: Vec<Line>,
        /// Unix mode as decimal (420 = 0o644, 493 = 0o755). Default 420.
        #[serde(default)]
        mode: Option<u32>,
    },
    /// Modify an existing file with context-anchored hunks.
    ModifyFile {
        path: TemplatePath,
        hunks: Vec<Hunk>,
    },
    /// Delete an existing file.
    DeleteFile { path: TemplatePath },
    /// Rename/move a file (target must not exist).
    RenamePath {
        from: TemplatePath,
        to: TemplatePath,
    },
    /// Change a file's mode.
    SetMode {
        path: TemplatePath,
        /// Unix mode as decimal (493 = 0o755).
        mode: u32,
    },
}

/// A weft patch file (`patches/<name>.json`): one recorded unit of template
/// behavior. The file name (stem) is the patch's name; its content id is
/// computed on load and never stored.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
#[schemars(title = "weft patch")]
pub struct PatchFile {
    /// What this patch does — metadata, never part of the content hash.
    #[serde(default)]
    pub description: Option<String>,
    /// Free-form labels — metadata, never part of the content hash.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Names (file stems) of patches this one depends on. Patches with no
    /// dependency path between them must commute.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Starlark gate: apply this patch (and its dependents) only if truthy.
    #[serde(default)]
    pub when: Option<String>,
    /// Operations, applied in order. May be omitted for **action patches**
    /// that exist only to carry hooks (e.g. a gated deploy).
    #[serde(default)]
    pub ops: Vec<Op>,
    /// Pre/post-render side-effects owned by this patch. Metadata: never part
    /// of the content hash, so adding/editing a hook never changes patch ids.
    #[serde(default)]
    pub hooks: Vec<Hook>,
}

/// A patch-scoped side-effect that runs before (`pre`) or after (`post`) the
/// render.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    /// Unique slug across the template (referenced by `after`/`inputs`).
    pub id: String,
    /// `pre` runs before any file is written (a non-zero exit aborts);
    /// `post` runs after the tree is written.
    pub phase: HookPhase,
    /// `check` = read-only verification (safe to auto-run); `setup` =
    /// idempotent local mutation; `deploy` = external/irreversible side-effect.
    pub effect: HookEffect,
    /// Short human/agent label, e.g. "Verify uv is installed".
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Shell command: a plain string when literal, or an array of segments to
    /// interpolate answers/exprs (e.g. `["shadcn add ", {"expr": "..."}]`).
    pub action: Command,
    /// Starlark gate.
    #[serde(default)]
    pub when: Option<String>,
    /// Other hooks this one must run after (ordering edges).
    #[serde(default)]
    pub after: Vec<String>,
    /// Post-only: `glob:PATTERN`, `answer:ID`, or `hook:ID` — on `weft update`
    /// this hook re-fires only when one changed.
    #[serde(default)]
    pub inputs: Vec<String>,
}

#[derive(JsonSchema, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookPhase {
    Pre,
    Post,
}

#[derive(JsonSchema, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookEffect {
    Check,
    Setup,
    Deploy,
}

/// A shell command: a plain string, or an array of segments.
#[derive(JsonSchema, Deserialize)]
#[serde(untagged)]
pub enum Command {
    Shell(String),
    Segments(Vec<Segment>),
}

// ---- manifest (weft.toml) ----------------------------------------------

/// `[template]` metadata.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateMeta {
    pub name: String,
    /// Manifest format version (currently "0.1").
    #[serde(rename = "weft-version")]
    pub weft_version: String,
    /// What this template scaffolds — the first thing agents read.
    #[serde(default)]
    pub description: Option<String>,
}

/// Question kind plus its kind-specific fields.
#[derive(JsonSchema, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum QuestionKind {
    String,
    Bool,
    Int,
    Choice {
        /// The legal values.
        choices: Vec<String>,
    },
    MultiChoice {
        /// The legal values; the answer is any subset of these.
        choices: Vec<String>,
    },
    Secret {
        /// `env:VAR`, `cmd:...`, or `prompt` — resolved at render time,
        /// never persisted.
        source: String,
    },
}

/// A `[[question]]` the template asks.
#[derive(JsonSchema, Deserialize)]
pub struct Question {
    /// Answer id: a Starlark identifier (lowercase, digits, underscore).
    pub id: String,
    #[serde(flatten)]
    pub kind: QuestionKind,
    /// Shown when prompting (defaults to the id).
    #[serde(default)]
    pub prompt: Option<String>,
    /// Longer human/agent-facing explanation of what this answer controls.
    #[serde(default)]
    pub description: Option<String>,
    /// Example value (display form, not Starlark).
    #[serde(default)]
    pub example: Option<String>,
    /// Starlark default; may reference earlier questions. String literals
    /// need inner quotes: `default = "'api'"`.
    #[serde(default)]
    pub default: Option<String>,
    /// Ask only if this Starlark gate is truthy (earlier answers in scope).
    #[serde(default)]
    pub when: Option<String>,
    /// A derived value: never prompted, always taken from `default`. Use for
    /// values computed from other answers. Requires a `default`.
    #[serde(default)]
    pub computed: bool,
    /// Display grouping for UIs (sidebar sections, wizard groups). Pure
    /// presentation metadata.
    #[serde(default)]
    pub section: Option<String>,
}

/// A `[[preset]]` declaration: a named partial answer-set.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetDecl {
    pub name: String,
    /// Path to a plain TOML answer map, relative to the template root.
    pub file: String,
}

/// The weft template manifest (`weft.toml`). Side-effects live on patches
/// (`patches/<name>.json` `hooks`), not here.
#[derive(JsonSchema, Deserialize)]
#[schemars(title = "weft manifest")]
pub struct Manifest {
    pub template: TemplateMeta,
    #[serde(default, rename = "question")]
    pub questions: Vec<Question>,
    #[serde(default, rename = "preset")]
    pub presets: Vec<PresetDecl>,
}

/// Generate both schemas into `out` (created if needed).
pub fn write_schemas(out: &Utf8PathBuf) -> Result<Vec<Utf8PathBuf>> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {out}"))?;
    let mut written = Vec::new();
    for (name, schema) in [
        ("weft-patch.schema.json", schemars::schema_for!(PatchFile)),
        ("weft-manifest.schema.json", schemars::schema_for!(Manifest)),
    ] {
        let path = out.join(name);
        let mut json = serde_json::to_string_pretty(&schema)?;
        json.push('\n');
        std::fs::write(&path, json).with_context(|| format!("writing {path}"))?;
        written.push(path);
    }
    Ok(written)
}
