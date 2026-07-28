use serde::{Deserialize, Serialize};

use crate::hook::Hook;
use crate::id::PatchId;
use crate::question::StarlarkExpr;
use crate::segment::{Content, Line, TemplatePath};

pub const DEFAULT_FILE_MODE: u32 = 0o644;

fn default_mode() -> u32 {
    DEFAULT_FILE_MODE
}

/// One recorded unit of template behavior.
///
/// The id is the blake3 hash of the canonical serialization of the patch
/// *body* (deps as ids, sorted). Ids therefore form a Merkle DAG; the id is
/// never stored inside the canonical form itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub id: PatchId,
    /// Explicit dependencies. Patches with no dependency path between them
    /// must commute (checked by `weft check`).
    pub depends_on: Vec<PatchId>,
    /// Apply only if this evaluates to true.
    pub when: Option<StarlarkExpr>,
    /// Integration patch: render once per instance of the named include,
    /// with `key` and `instance.<id>` in scope. **Behavioral — part of the
    /// canonical hash** (absent serializes to nothing, so patches without it
    /// keep their existing ids). Foreach patches must be graph leaves.
    pub foreach: Option<String>,
    pub ops: Vec<Op>,
    /// Descriptive metadata. **Never part of the canonical hash**: editing a
    /// description or tag must not change this patch's id (or any
    /// descendant's), so documentation edits can't break pinned projects.
    pub meta: PatchMeta,
}

/// Human/agent-facing patch metadata, excluded from content addressing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchMeta {
    /// Display title, e.g. "Add Prisma support". UIs prefer this over the
    /// kebab-case file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Pre/post-render side-effects owned by this patch. Not hashed: adding or
    /// editing a hook never changes the patch id (like `description`/`tags`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Hook>,
    /// Present when this patch's content was produced by `weft session new
    /// --exec`: the command plus what `weft patch resync` needs to re-run
    /// it. Not hashed — like every other metadata field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<crate::generator::Generator>,
}

/// The hashed portion of a patch. Field order is the canonical key order.
/// `foreach` is skipped when absent so pre-existing patches keep their ids.
#[derive(Serialize)]
struct PatchBody<'a> {
    depends_on: &'a [PatchId],
    when: &'a Option<StarlarkExpr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    foreach: &'a Option<String>,
    ops: &'a [Op],
}

impl Patch {
    /// Build a patch, sorting dependencies and computing the content id.
    pub fn new(depends_on: Vec<PatchId>, when: Option<StarlarkExpr>, ops: Vec<Op>) -> Self {
        Self::new_foreach(depends_on, when, None, ops)
    }

    /// Build a patch with an optional `foreach` include binding.
    pub fn new_foreach(
        mut depends_on: Vec<PatchId>,
        when: Option<StarlarkExpr>,
        foreach: Option<String>,
        ops: Vec<Op>,
    ) -> Self {
        depends_on.sort();
        depends_on.dedup();
        let canonical = canonical_json(&depends_on, &when, &foreach, &ops);
        Patch {
            id: PatchId::from_canonical_bytes(canonical.as_bytes()),
            depends_on,
            when,
            foreach,
            ops,
            meta: PatchMeta::default(),
        }
    }

    /// Attach descriptive metadata (does not affect the id).
    pub fn with_meta(mut self, meta: PatchMeta) -> Self {
        self.meta = meta;
        self
    }

    /// Canonical serialization: compact JSON, sorted deps, fixed key order.
    /// Same bytes ⇒ same id.
    pub fn canonical_json(&self) -> String {
        canonical_json(&self.depends_on, &self.when, &self.foreach, &self.ops)
    }
}

fn canonical_json(
    deps: &[PatchId],
    when: &Option<StarlarkExpr>,
    foreach: &Option<String>,
    ops: &[Op],
) -> String {
    debug_assert!(deps.windows(2).all(|w| w[0] < w[1]), "deps must be sorted");
    serde_json::to_string(&PatchBody {
        depends_on: deps,
        when,
        foreach,
        ops,
    })
    .expect("patch body serialization cannot fail")
}

/// A single tree operation. Paths are tree-relative, `/`-separated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    CreateFile {
        path: TemplatePath,
        content: Content,
        #[serde(default = "default_mode")]
        mode: u32,
    },
    /// An opaque binary file (e.g. a favicon from a generator). The bytes
    /// are standard-alphabet base64; the content is never abstracted and
    /// never hunk-modified — changing a binary is `delete_file` + a new
    /// `create_binary_file` in the same patch. The path is a normal
    /// `TemplatePath` (path abstraction still applies).
    CreateBinaryFile {
        path: TemplatePath,
        data: String,
        #[serde(default = "default_mode")]
        mode: u32,
    },
    ModifyFile {
        path: TemplatePath,
        hunks: Vec<Hunk>,
    },
    DeleteFile {
        path: TemplatePath,
    },
    RenamePath {
        from: TemplatePath,
        to: TemplatePath,
    },
    SetMode {
        path: TemplatePath,
        mode: u32,
    },
}

/// A context-anchored change to a file: never line-numbered. The hunk matches
/// where `context_before + removed + context_after` occurs (exactly once) in
/// the rendered file and replaces `removed` with `added`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::Segment;

    fn sample_patch() -> Patch {
        Patch::new(
            vec![],
            Some(StarlarkExpr::from("use_docker")),
            vec![
                Op::CreateFile {
                    path: TemplatePath::literal("Dockerfile"),
                    content: Content(vec![
                        Line::literal("FROM python:3.12-slim"),
                        Line(vec![
                            Segment::Literal("LABEL project=".into()),
                            Segment::Answer("project_name".into()),
                        ]),
                    ]),
                    mode: DEFAULT_FILE_MODE,
                },
                Op::SetMode {
                    path: TemplatePath::literal("scripts/run.sh"),
                    mode: 0o755,
                },
            ],
        )
    }

    #[test]
    fn id_is_stable_across_rebuilds() {
        assert_eq!(sample_patch().id, sample_patch().id);
    }

    #[test]
    fn id_changes_when_ops_change() {
        let a = sample_patch();
        let b = Patch::new(vec![], a.when.clone(), vec![]);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn deps_are_order_insensitive() {
        let d1 = sample_patch().id;
        let d2 = Patch::new(vec![], None, vec![]).id;
        let p1 = Patch::new(vec![d1, d2], None, vec![]);
        let p2 = Patch::new(vec![d2, d1], None, vec![]);
        assert_eq!(p1.id, p2.id);
    }

    #[test]
    fn metadata_never_changes_the_id() {
        use crate::generator::Generator;
        use crate::hook::{Command, Hook, HookEffect, HookPhase};
        let plain = sample_patch();
        let documented = sample_patch().with_meta(PatchMeta {
            title: Some("Add Docker support".into()),
            description: Some("adds docker support".into()),
            tags: vec!["docker".into(), "infra".into()],
            hooks: vec![Hook {
                id: "verify-docker".into(),
                phase: HookPhase::Pre,
                effect: HookEffect::Check,
                label: "Verify docker is installed".into(),
                description: None,
                action: Command::literal("command -v docker"),
                when: None,
                after: vec![],
                inputs: vec![],
            }],
            generator: Some(Generator {
                command: Command::literal("npx shadcn@latest add button"),
                answers: crate::AnswerSet::new(),
                secrets: Default::default(),
                keep_literal: vec![],
            }),
        });
        assert_eq!(plain.id, documented.id);
        assert_eq!(plain.canonical_json(), documented.canonical_json());
    }

    #[test]
    fn absent_foreach_keeps_existing_ids() {
        // A patch built without foreach must hash identically to the
        // pre-foreach format (the field is skipped when None).
        let p = sample_patch();
        assert!(!p.canonical_json().contains("foreach"));
        let with = Patch::new_foreach(vec![], None, Some("connector".into()), vec![]);
        let without = Patch::new(vec![], None, vec![]);
        assert_ne!(with.id, without.id, "foreach is behavioral and hashed");
    }

    #[test]
    fn canonical_round_trip_is_byte_identical() {
        let p = sample_patch();
        let json = p.canonical_json();
        let body: serde_json::Value = serde_json::from_str(&json).unwrap();
        let ops: Vec<Op> = serde_json::from_value(body["ops"].clone()).unwrap();
        let rebuilt = Patch::new(p.depends_on.clone(), p.when.clone(), ops);
        assert_eq!(rebuilt.canonical_json(), json);
        assert_eq!(rebuilt.id, p.id);
    }
}
