# Weft — Bootstrap Plan for Claude Code

## Context

Weft is a project scaffolding/template engine that replaces Copier. Instead of authoring Jinja templates by hand, template authors **record** changes against a rendered base state. Patches are stored as **inputs** (answer-set + operations), not output diffs. Composition is patch composition (Pijul-flavored), presets are layered partial answer-sets, tasks are nodes in a dependency graph keyed to file changes, and secrets resolve through providers at render time and are never persisted.

Language: **Rust**. Deliverable: a single static binary `weft`.

Reference design docs (read first if present in repo): `weft-mvp-design.md`, `weft-record-mvp.md`.

## Non-goals (MVP)

- No Jinja compatibility layer, no Copier migration tooling.
- No remote template registries; templates are local paths or git URLs cloned by the user.
- No GUI/TUI beyond plain interactive prompts.
- No structural/AST-aware merge for specific file formats (post-MVP; design for it, don't build it).
- No Windows support guarantees in MVP (don't gratuitously break it, but test on Linux/macOS only).

## Guiding principles

1. **Patches store inputs, not outputs.** A patch = (answer-schema delta, operation list). Rendering is deterministic: `render(base, answers, patches) -> tree`.
2. **Base-state isolation during `record`.** Recording happens in a scratch worktree materialized from a pinned base state; the author's dirty working directory never leaks in.
3. **Value abstraction happens at `commit`, not during recording.** The author edits concrete files; on commit, Weft proposes which concrete literals correspond to which answers (exact-match first, interactive confirmation).
4. **Starlark is the declarative escape hatch** for conditions, derived values, and task predicates — no arbitrary shell in the manifest, shell only inside task actions.
5. **Determinism and idempotence everywhere.** Same inputs → byte-identical output tree. `weft update` twice in a row is a no-op the second time.

## Workspace layout

```
weft/
├── Cargo.toml            # workspace
├── crates/
│   ├── weft-core/        # data model, patch algebra, rendering, no I/O policy decisions
│   ├── weft-engine/      # record/commit/apply/update orchestration, worktree management
│   ├── weft-lang/        # Starlark integration (conditions, derived values, task predicates)
│   └── weft-cli/         # clap-based binary
├── tests/
│   └── e2e/              # full-binary integration tests on fixture templates
└── fixtures/
    └── templates/        # small fixture templates used by unit + e2e tests
```

## Key crates

- `clap` (derive) — CLI
- `serde`, `serde_json`, `toml` — manifests and answer files
- `starlark` (facebook/starlark-rust) — declarative logic
- `similar` — text diffing for `record` change detection and update conflict display
- `blake3` — content addressing of base states and patches
- `tempfile` — scratch worktrees
- `dialoguer` — interactive prompts
- `insta` — snapshot testing
- `assert_cmd` + `predicates` — e2e CLI tests
- `camino` — UTF-8 paths throughout

Avoid: any Jinja crate, `git2` in MVP (shell out to `git` only where unavoidable, prefer not at all).

## Core data model (weft-core)

Define these types first; everything else depends on them.

```rust
/// A question the template asks. Serialized in weft.toml.
struct Question {
    id: AnswerId,
    kind: AnswerKind,          // String, Bool, Choice(Vec<String>), Int, Secret(SecretSpec)
    prompt: String,
    default: Option<StarlarkExpr>, // may reference other answers
    when: Option<StarlarkExpr>,    // ask only if true
}

/// Concrete values for some subset of questions.
struct AnswerSet(BTreeMap<AnswerId, Value>);   // partial by design — presets are just AnswerSets

/// One recorded unit of template behavior.
struct Patch {
    id: PatchId,               // blake3 of canonical serialization
    depends_on: Vec<PatchId>,  // explicit deps; independent patches must commute
    when: Option<StarlarkExpr>,
    ops: Vec<Op>,
}

enum Op {
    CreateFile { path: TemplatePath, content: Content },
    ModifyFile { path: TemplatePath, hunks: Vec<Hunk> },   // context-anchored, not line-numbered
    DeleteFile { path: TemplatePath },
    RenamePath { from: TemplatePath, to: TemplatePath },
    SetMode    { path: TemplatePath, mode: u32 },
}

/// Paths and content may embed answer references after `commit` abstraction.
enum Segment { Literal(String), Answer(AnswerId), Expr(StarlarkExpr) }
struct TemplatePath(Vec<Segment>);
struct Content(Vec<Segment>);      // line-granular segments are fine for MVP

/// Tasks live in the same graph as file ops.
struct Task {
    id: TaskId,
    inputs: Vec<TaskInput>,        // Glob(String) | Answer(AnswerId) | Task(TaskId)
    action: TaskAction,            // Shell(String) for MVP
    when: Option<StarlarkExpr>,
}
```

Hunks are **context-anchored** (surrounding lines + content hash), never absolute line numbers — this is what makes patches survive reordering and composition.

## Manifest (`weft.toml`)

```toml
[template]
name = "python-service"
weft-version = "0.1"

[[question]]
id = "project_name"
kind = "string"
prompt = "Project name"

[[question]]
id = "use_docker"
kind = "bool"
default = "true"

[[preset]]
name = "internal-service"
file = "presets/internal.toml"    # a partial AnswerSet; later presets override earlier

[[task]]
id = "uv-sync"
inputs = ["glob:pyproject.toml", "glob:uv.lock"]
action = "uv sync"
```

Preset files are plain TOML answer maps. Layering order: template defaults → presets in declared/CLI order → user answers file → interactive answers. Later wins; conflicts at equal precedence are errors, not silent overrides.

## Secrets

- `kind = "secret"` questions carry a `SecretSpec`: `env:VAR_NAME`, `cmd:op read ...`, or `prompt` (never persisted).
- The answers file stores only the **reference** (`env:DATABASE_URL`), never the value.
- Resolution happens at render time in `weft-engine`; `weft-core` treats secrets as opaque `Value::Secret` and refuses to serialize them.
- `weft update` must not re-prompt if a reference resolves.

## CLI surface (MVP)

```
weft new <template> [dest] [--preset NAME]... [--answer KEY=VALUE]... [--answers-file F]
weft update [dest]                 # re-render with pinned template ref, 3-way apply local edits
weft record --base <ref>           # materialize scratch worktree, drop user into it
weft commit                        # diff worktree vs base, propose value abstraction, write Patch
weft presets list|show
weft check                         # validate manifest, patch graph acyclic, patches commute where claimed
```

`weft update` semantics: render old inputs → render new inputs → 3-way merge against the user's tree using `similar`. Conflicts produce `.rej`-style conflict markers plus a summary; never silently clobber. Task graph re-fires only tasks whose declared inputs changed between the two renders.

## Milestones

Execute in order. Each milestone ends with green tests and a short note in `PROGRESS.md` describing decisions/deviations.

### M0 — Skeleton (small)
- Cargo workspace, four crates, CI-ready `cargo fmt --check && cargo clippy -D warnings && cargo test`.
- `weft --version`, `weft check` stub.
- Acceptance: `cargo test` green, binary builds.

### M1 — Data model + serialization (core)
- All types above in `weft-core` with serde round-trips, blake3 `PatchId`, canonical serialization.
- Property test: serialize → deserialize → serialize is byte-identical.
- Acceptance: insta snapshots of canonical forms for fixture patches.

### M2 — Deterministic render
- `render(questions, layered_answer_sets, patches) -> InMemoryTree`.
- Answer layering with precedence rules; Starlark evaluation for `default`/`when` (weft-lang).
- Apply ops in dependency order; context-anchored hunk application with clean failure on mismatch.
- Acceptance: fixture template renders byte-identically across two runs; golden-tree snapshot tests.

### M3 — `weft new`
- Interactive prompts (dialoguer) for unanswered questions; `--preset`, `--answer`, `--answers-file`.
- Write `.weft/state.toml` in dest: template ref, pinned base hash, answer references (secrets as refs only).
- Acceptance: e2e test scaffolds fixture template with a preset non-interactively.

### M4 — `record` / `commit`
- `record`: materialize base state into tempdir worktree, store session metadata.
- `commit`: diff worktree vs base (similar), build `Op` list with context-anchored hunks, run value-abstraction pass (exact literal match against current answers, interactive confirm), append Patch to template.
- Acceptance: e2e round-trip — record a change, commit, `weft new` with different answers produces correctly abstracted output.

### M5 — `weft update` + task graph
- 3-way merge update as specified above.
- Task graph: topo-sort tasks with file/answer/task inputs; fire only on changed inputs; `--dry-run` prints plan.
- Acceptance: e2e — update after template gains a patch applies cleanly over local edits; task with `glob:pyproject.toml` input fires only when that file changed.

### M6 — Hardening
- `weft check` full validation (acyclic deps, commutation smoke test: apply independent patches in both orders, compare trees).
- Error messages with file/patch context (`miette` optional).
- README with a 5-minute tutorial using a fixture template.

## Testing strategy

- Unit tests colocated; snapshot tests via `insta` for rendered trees and canonical serializations.
- Property tests (proptest) for: hunk application inverses, patch commutation on independent patches, answer-layering precedence.
- e2e via `assert_cmd` against `fixtures/templates/`; keep fixtures tiny (≤10 files).
- Every bug found gets a regression fixture.

## Working agreements for Claude Code

- Small commits per logical step; conventional commit messages.
- If a design decision in this plan proves wrong during implementation, stop, write the alternative and tradeoff in `PROGRESS.md`, and pick the simpler option that preserves the guiding principles.
- Never introduce a text-templating dependency (Jinja/Tera/Handlebars) — that is the anti-goal.
- Prefer `weft-core` staying pure (no filesystem, no prompts); all I/O in `weft-engine`/`weft-cli`.
- Keep `unsafe` at zero.
