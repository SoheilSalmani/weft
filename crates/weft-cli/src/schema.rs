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

/// One line of the lines a patch adds (`create_file` content, a hunk's
/// `added`): a line, or a slot declaration.
#[derive(JsonSchema, Deserialize)]
#[serde(untagged)]
pub enum AddedLine {
    Literal(String),
    Segments(Vec<Segment>),
    /// Declare a slot here: a named place other patches add lines to with
    /// `fill_slot`. It renders their contributions sorted by key.
    Slot(SlotDecl),
}

/// A slot declaration, a line of its own.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDecl {
    /// The slot's name, unique within the file: ASCII letters, digits, `-`,
    /// `_` and `.`.
    #[schemars(regex(pattern = r"^[A-Za-z0-9_.-]+$"))]
    pub slot: String,
    /// Appended to the last line of every contribution but the final one
    /// (e.g. `,` between JSON entries).
    #[serde(default)]
    pub separator: Option<String>,
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
    pub added: Vec<AddedLine>,
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
        /// Slots of `content` that, when every one of them is still empty at
        /// the end of the render, leave the file out altogether.
        #[serde(default)]
        omit_when_empty: Vec<String>,
        /// File content as an array of lines; a `{"slot": …}` line declares
        /// a slot.
        content: Vec<AddedLine>,
        /// Unix mode as decimal (420 = 0o644, 493 = 0o755). Default 420.
        #[serde(default)]
        mode: Option<u32>,
    },
    /// Create an opaque binary file (errors if it already exists). The
    /// content is never abstracted or hunk-modified — replacing a binary is
    /// `delete_file` + a new `create_binary_file` in the same patch.
    CreateBinaryFile {
        path: TemplatePath,
        /// Raw bytes, standard-alphabet base64.
        data: String,
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
    /// Add lines to a slot of an existing file as the contribution under
    /// `key`. Contributions render sorted by key, so patches that only fill
    /// slots commute; two with one key in one slot are an error.
    FillSlot {
        path: TemplatePath,
        /// The slot's name, as its file declares it.
        slot: String,
        /// Orders the contribution within the slot: a plain string when
        /// literal, or an array of segments (a foreach patch needs the
        /// instance `key` in it). Recording uses the patch's name.
        key: Line,
        /// The contribution's lines (at least one; slots do not nest).
        #[schemars(length(min = 1))]
        lines: Vec<Line>,
    },
}

/// A weft patch file (`patches/<name>.json`): one recorded unit of template
/// behavior. The file name (stem) is the patch's name; its content id is
/// computed on load and never stored.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
#[schemars(title = "weft patch")]
pub struct PatchFile {
    /// Display title, e.g. "Add Prisma support" — metadata, never hashed.
    #[serde(default)]
    pub title: Option<String>,
    /// What this patch does — metadata, never part of the content hash.
    #[serde(default)]
    pub description: Option<String>,
    /// Free-form labels — metadata, never part of the content hash.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Names of the graph nodes this patch depends on: another patch's file
    /// stem (own or inherited through `[template] extends`), or a single
    /// include's patch as `<include>/<patch>` (nested: `<include>/<include>/
    /// <patch>`). Depending on an include's node is what allows this patch
    /// to edit files under that include's mount. A repeat include has no
    /// nameable nodes — use `foreach`. Patches with no dependency path
    /// between them must commute.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Starlark gate: apply this patch (and its dependents) only if truthy.
    #[serde(default)]
    pub when: Option<String>,
    /// Integration patch: render once per instance of the named include.
    /// `key` and `instance_<child-question>` are in scope for segments and
    /// expressions. Behavioral — part of the content hash. Foreach patches
    /// must be graph leaves.
    #[serde(default)]
    pub foreach: Option<String>,
    /// Operations, applied in order. May be omitted for **action patches**
    /// that exist only to carry hooks (e.g. a gated deploy).
    #[serde(default)]
    pub ops: Vec<Op>,
    /// Pre/post-render side-effects owned by this patch. Metadata: never part
    /// of the content hash, so adding/editing a hook never changes patch ids.
    #[serde(default)]
    pub hooks: Vec<Hook>,
    /// Present when this patch was recorded from a command's output
    /// (`weft record --exec`). Metadata: never part of the content hash.
    #[serde(default)]
    pub generator: Option<Generator>,
}

/// The command a generated patch was recorded from, plus what
/// `weft patch resync` needs to re-run it deterministically.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    /// Shell command run in the recording worktree: a plain string when
    /// literal, or an array of segments to interpolate answers/exprs.
    pub command: Command,
    /// Record-time answers the base was rendered with (secrets excluded).
    #[serde(default)]
    pub answers: std::collections::BTreeMap<String, serde_json::Value>,
    /// Secret answers as source references (`env:…`, `cmd:…`, `prompt`),
    /// re-resolved at resync time — values never appear here.
    #[serde(default)]
    pub secrets: std::collections::BTreeMap<String, String>,
    /// Occurrences kept literal at commit (`ANSWER@PATH:LINE[:NTH]`),
    /// replayed on resync.
    #[serde(default)]
    pub keep_literal: Vec<String>,
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
    /// Other hooks this one must run before (the mirror of `after`; how an
    /// extender orders its hooks ahead of one it inherits).
    #[serde(default)]
    pub before: Vec<String>,
    /// Post-only update triggers: `glob:PATTERN`, `answer:ID`, or `hook:ID`. A
    /// post hook runs when its project or include instance is created; on a
    /// later `weft update` it runs only when one of these changed, so a post
    /// hook without inputs runs only at creation. `pre` hooks always run.
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

/// The base template a template extends: a directory path relative to this
/// template root (`../base`), or a `hub:` ref with a version requirement.
/// Its questions, includes, and patches are imported as-is (same names, same
/// ids, no mount); this template's patches may depend on them by name but
/// may not redeclare them.
#[derive(JsonSchema, Deserialize)]
#[serde(untagged)]
pub enum ExtendsDecl {
    /// Relative directory path (`../base`) or `hub:owner/name`.
    Path(String),
    Full {
        /// Relative directory path or `hub:owner/name`.
        template: String,
        /// Semver requirement for a `hub:` template (e.g. `^1.0`).
        #[serde(default)]
        version: Option<String>,
    },
}

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
    /// The base template this one builds on (see `ExtendsDecl`).
    #[serde(default)]
    pub extends: Option<ExtendsDecl>,
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
    /// Display grouping for UIs (sidebar sections). Pure presentation
    /// metadata.
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

/// A `[[include]]`: a child template mounted at a path prefix. Child answers
/// are namespaced `<name>.<id>` (or `<name>.<key>.<id>` for repeat).
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncludeDecl {
    /// Include slug; unique per template.
    pub name: String,
    /// The child template: a directory path relative to this template root
    /// (`../go-service`), a registry ref (`hub:owner/name`), or a git source
    /// (`gh:owner/repo//dir@rev`, any git URL; `@rev` pins a tag, branch,
    /// or commit, resolved to an exact commit in `weft.lock`).
    pub template: String,
    /// Semver requirement for a `hub:` template (e.g. `^1.0`). Required for
    /// hub refs, forbidden for path and git refs; resolved to an exact
    /// version in `weft.lock`.
    #[serde(default)]
    pub version: Option<String>,
    /// Mount prefix in the rendered tree; `{key}` substitutes the instance
    /// key (required when `repeat = true`). Omitted or empty mounts the
    /// child at the root: its files merge with the parent's, and a path both
    /// create is a render error.
    #[serde(default)]
    pub path: String,
    /// Whether the project may instantiate this include 0..N times.
    #[serde(default)]
    pub repeat: bool,
    /// Child answer id → Starlark expression over parent answers (+ `key`).
    #[serde(default)]
    pub bind: std::collections::BTreeMap<String, String>,
}

/// A `[refine.<id>]` table: how this template narrows a question it inherits
/// through `[template] extends`. A refinement can reword the question,
/// replace its default, lock it, or restrict its choices; it can never widen
/// what the base allows.
#[derive(JsonSchema, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefineDecl {
    /// Replaces the inherited prompt.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Replaces the inherited description.
    #[serde(default)]
    pub description: Option<String>,
    /// Replaces the inherited example (display form, not Starlark).
    #[serde(default)]
    pub example: Option<String>,
    /// Replaces the inherited default (a Starlark expression that may only
    /// mention questions declared before this one). The answer stays
    /// editable.
    #[serde(default)]
    pub default: Option<String>,
    /// The only value the question accepts (Starlark, same scope rules as
    /// `default`); it is never asked. Excludes every other value key.
    #[serde(default)]
    pub lock: Option<String>,
    /// Choice/multichoice allow-list: every choice not listed is blocked.
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    /// Choice/multichoice deny-list: these choices can't be picked.
    #[serde(default)]
    pub blocked: Vec<String>,
    /// Multichoice: always selected.
    #[serde(default)]
    pub fixed: Vec<String>,
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
    #[serde(default, rename = "include")]
    pub includes: Vec<IncludeDecl>,
    /// `[refine.<id>]` tables: how this template narrows questions it
    /// inherits through `[template] extends`, keyed by question id.
    #[serde(default)]
    pub refine: std::collections::BTreeMap<String, RefineDecl>,
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
