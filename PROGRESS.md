# Progress log

Running notes per milestone, as required by PLAN.md. Records decisions and
deviations from the plan.

## Post-MVP — Interactive completion forms (TUI)

- **One form engine, one theme** (`weft-cli/src/tui/`): accent focus bar,
  right-aligned labels, popup pickers (no enter-cycling), cursor-based
  text editing, inline validation under the field, adaptive footer keymap,
  submit row. The answers wizard was restyled onto the same theme.
- **Uniform activation rule**: a command with required options missing, in
  a real terminal, without `--no-tui`, opens a prefilled form (provided
  flags = defaults). Non-TTY behavior is byte-for-byte unchanged (e2e
  proves it). Submitting echoes the equivalent flag invocation.
- Wired: `new` (template picker → wizard), `commit` (prefilled patch-NNN),
  `hook add`, `patch set`, `instance add`.
- The PTY smoke test (script(1), 0×0 pty) caught a real `clamp` panic in
  popup sizing on tiny terminals — fixed with a 0×0 render unit test; a
  full form submission was driven end-to-end through the pty.

## Post-MVP — Hand-edit-free authoring

- **`weft init` / `weft hook add|rm|ls` / `weft patch set|ls`**: template
  skeletons and metadata editing from the CLI. Hook edits validate with the
  same checks as `weft check` and roll the file back on failure; e2e proves
  ids never move under metadata edits.
- **`weft record --foreach include=key`**: foreach patches are now recorded
  — one *sample* instance mounts into the base, the author registers it in
  parent files, and commit abstracts the sample into `key`/`instance_<id>`
  references (`key` canonical when values tie). Plain record works on
  composed templates (parent-only); commit rejects edits under mounts.
- **Base excludes foreach patches** (record + commit): nothing may depend
  on them, and their per-instance output must not leak into recorded hunk
  contexts.
- **Deps from *active* leaves**: a patch gated off under the session's
  answers no longer becomes a dependency of the recorded patch (found by
  the dbt tutorial's two-branch profile — conn-remote must not depend on
  the gated-off conn-duckdb).
- **Rename detection at commit**: exact-content move → `rename_path`
  (+ `set_mode` when modes differ).

## Post-MVP — Patch titles + preview instance declarations

- **`PatchMeta.title`**: display name ("Add Prisma support") — metadata,
  never hashed, alongside description/tags. Flows through the patch file,
  `graph` (`GraphNode.title`), `describe` (`PatchDescription.title`),
  `weft commit --title`, and the cloud commit input. UIs show
  `title ?? prettified(kebab-name)`; ids/hashes move to advanced details.
- **`IncludeGraph.bind`**: bind sources exposed in `graph --json` so UIs can
  mark child questions as seeded "from parent".
- **`preview_parts` takes explicit instance declarations** (`declared`
  set), so a repeat-include instance with zero overridden answers (fully
  bound) still previews; the cloud `TemplateInput` gained `instances:
  ["include=key"]`.

## Post-MVP — Composition M5: nested includes + preview parts

- **Recursive composition**: `ComposedPart` gained `children`;
  `render_composed` recurses, so a child that itself declares includes mounts
  its own children (platform → workspace → hello, binds chained through each
  level — e2e proves `# Mega` → `# Mega Acme` → `# Mega Acme Service`).
  Nested levels resolve from binds + the grandchild's own defaults/secrets
  (no namespaced pass-through yet); nested `repeat` includes have zero
  instances (nested instance state is deferred); on update, nested parts
  re-derive on both sides (grandchild answers aren't pinned). Nested
  children's hooks don't run yet (known gap).
- **`compose::preview_parts` + `SecretMode`**: preview-oriented instance
  resolution — placeholder secrets at every level, never prompting, repeat
  instances only where namespaced answers imply them. This is what servers
  and UIs call; `resolve_instance_answers` gained a `presolved` parameter
  (also lets update pass stored child secrets through without re-prompting).

## Post-MVP — Composition M4: the contract

- `weft describe --json` gains `includes` — each with its mount, `repeat`,
  bind sources, and the **child's full question schema** (so agents know
  exactly which namespaced answers each instance accepts) — and patches gain
  `foreach`. AGENTS.md gets an includes table.
- `weft graph --json` gains `includes` (nested structural child GraphDocs —
  the future cloud subflow feed) and per-node `foreach`.
- Manifest schema mirrors `[[include]]`; patch schema mirrors `foreach`;
  `schemas/` regenerated.
- weft-cloud server still compiles untouched (all additions were additive);
  surfacing includes in the UI is the next pass. Docs: new
  `weft/guides/composition.mdx` + manifest/patch-format/cli/state reference
  updates.

## Post-MVP — Composition M3: foreach integration patches

- A parent patch may declare `"foreach": "<include>"` — it renders **once per
  instance** of that include, after the children are mounted. Scope: parent
  answers + `key` + the instance's child answers flattened as
  **`instance_<id>`** (underscore, not the planned dot: dots aren't valid
  Starlark identifiers, and one convention must work in both `{"answer": …}`
  segments and expressions). Canonical use: a `modify_file` hunk registering
  each instance in a parent file (pnpm-workspace style).
- **Hashing**: `foreach` is behavioral, so it joins `PatchBody` — with
  `skip_serializing_if None`, so every pre-existing patch id is unchanged
  (tested).
- **Semantics**: plain (core) render skips foreach patches — they only apply
  in composed rendering, deterministically (patches by id, instances by
  key). A part with an **empty patch set** (an instance pinned by
  `weft instance add` before its realizing update) is skipped: it doesn't
  exist on that side yet, so it contributes no integration lines either —
  this is what makes add/remove produce clean registry diffs.
- **check**: foreach must name a real include; foreach patches must be graph
  leaves (nothing depends on them); segments may reference `key` /
  `instance_<child-question>`; after the full render, each foreach patch is
  trial-applied with a dummy instance so its hunks/segments are validated
  without real instances. Graph marks foreach nodes active-as-deps (their
  gate is per-instance).
- Workspace fixture gains a `registry` foreach patch; the fleet e2e now also
  proves integration: lines appear per instance at scaffold, on
  `instance add`, and disappear on `instance remove`.

## Post-MVP — Composition M2: repeat instances + `weft instance` (fleets)

- `repeat = true` includes are instantiated **0..N times per project**.
  Instance keys are slugs; answers are namespaced `<include>.<key>.<id>`
  (providing one implicitly declares the instance); `--instance
  connector=github` declares one explicitly with no answers.
- **`weft instance add <include> <key> [--answer id=v…]`** pins a new
  instance with an *empty base* and only the explicit answers, then runs the
  ordinary update pass — the old side renders nothing for it, so its files
  land as pure additions and the full resolution (binds, defaults, secrets)
  is re-pinned. **`remove`** drops the instance from the update's *new* side
  (`UpdateOptions::drop_instances`): template-deleted semantics — untouched
  files deleted, user-modified kept + reported, instance unpinned. **`list`**
  prints `include=key @ mount`.
- **Fleet update stays one command**: `weft update` re-renders every pinned
  instance; the e2e lifecycle proves scaffold-with-2 → add → single update
  propagating a child-template change into all instances (+ the non-repeat
  include), local edits preserved, then remove.
- Deliberate scope cuts: no reserved `instances` table in answers files
  (namespaced flat keys + `--instance` cover it); the MCP `scaffold` tool
  does not yet declare instances.

## Post-MVP — Composition M1: `[[include]]` (single instances)

A template can now **include** other templates, mounted at a path prefix —
the foundation of multi-instance fleets and Turborepo-style workspaces.

- **Manifest**: `[[include]] { name, template (path, relative to the root),
  path (mount prefix, `{key}` substitution), repeat (M2), bind }`. Children
  are ordinary self-contained templates; loading recurses with cycle
  detection (canonicalized roots) and a depth cap of 8.
- **Answers**: child answers are namespaced from the parent as
  `<include>.<id>` (`--answer svc.port=8000`, same in answers files —
  quote dotted TOML keys — and `--answers-json`). `bind` expressions seed
  child answers from parent scope (plus `key`); explicit namespaced answers
  override binds; the rest resolve via the child's own defaults/secrets/
  prompts (`answers::gather` reused per instance).
- **Composed render** (`compose.rs`): parent patches render at the root;
  each instance's child render is path-prefixed under its mount and merged
  (collision = error). Core stays single-template pure.
- **Update = fleet update for free**: the old tree is reconstructed from
  pinned parent + per-instance child bases (`[[instance]]` in
  `.weft/state.toml`: include, key, stored mount, base ids, answers, secret
  refs) and stored answers; the new tree from current state; the existing
  3-way merge applies unchanged. Stored instance answers win over
  re-evaluated binds (same pinning policy as parent answers). The mount is
  stored so a template-side mount move behaves as delete+create.
- **Hooks**: child pre-hooks run at the dest root (mount not yet written),
  child post-hooks inside their mount, parent post last. On update, child
  post-hooks re-fire against a child-relative change set (paths under the
  mount, stripped; changed child answers). `after` does not cross template
  boundaries.
- **record** on a composed template bails — patches belong to the child;
  record against it directly.
- **check**: mount validation + cross-include collision, `repeat` requires
  `{key}`, bind parse + trial-eval over dummy parent answers + bind targets
  must exist in the child, and full recursion into child checks (issues
  prefixed with the include name).
- New `workspace` fixture (includes `hello` at `services/hello` with a
  bind); e2e covers composed scaffold, namespaced overrides, child-template
  evolution propagating through one `weft update` (local edits preserved,
  idempotent), and check.

## Post-MVP — Composition prelude: action patches + question sections

First milestone of the template-composition track (includes/instances/fleet
updates — see the plan). Two small model blessings:

- **Action patches**: a patch may omit `ops` entirely (`#[serde(default)]` on
  `PatchFile.ops` + schema) and exist purely to carry hooks + a `when` gate —
  the home for code-free actions (deploy, provision). Decided *against*
  attaching hooks to questions: questions are inputs, hooks are effects; a
  multi-answer hook has no owning question; and patches already provide DAG
  ordering/gating/graph visibility. `hello` fixture gained a zero-op `deploy`
  patch; e2e covers graph/describe presence + hook firing + schema acceptance.
- **`Question.section`**: optional display-grouping metadata (sidebar
  sections, wizard groups), carried through describe; no effect on
  resolution.

## Post-MVP — Patch-scoped hooks (pre/post-render actions)

Replaces template-level `[[task]]` with **hooks** owned by patches. A hook is
a labeled, AI-classified side-effect: `{ id, phase: pre|post, effect:
check|setup|deploy, label, description?, action, when?, after[], inputs[] }`.

- **Home & identity**: hooks live in `PatchMeta` (`weft-core`), which
  `PatchBody`/`canonical_json` already omit — so adding/editing a hook never
  changes patch ids (same guarantee as `description`/`tags`;
  `metadata_never_changes_the_id` extended to cover hooks). `task.rs` →
  `hook.rs`: `TaskAction`→`Command`, `TaskInput`→`HookInput` (`hook:` prefix),
  `TaskId`→`HookId`, `Task`→`Hook`. Manifest `[[task]]` removed entirely
  (`Manifest` drops `tasks`); every action is now patch-scoped.
- **effect** is the AI-ready/risk axis: `check` (read-only, safe to auto-run;
  a failing *pre* check aborts before any file is written), `setup`
  (idempotent local), `deploy` (external/irreversible — confirm first).
- **Ordering** (`weft-engine/hooks.rs`, was `tasks.rs`): collect hooks from
  **active** patches only, split by phase; within a phase topo-sort over an
  `after` DAG with a stable base order = patch render order + in-patch
  declaration order (Kahn, ready-set drained by base index). Finalize hooks
  (`format`, `git commit`) sequence via `after`.
- **Execution**: pre-hooks run in `new.rs`/`update.rs` *before* `write_tree`
  (abort → nothing written); post-hooks after `state.save`, rendered against
  resolved answers via `render::render_segments`, bailing on first failure. On
  update, a post-hook re-fires only when an `inputs` entry changed (`glob:`/
  `answer:`/`hook:`); pre-hooks always run. Secret safety: the run log prints
  the label (not the rendered command) for any non-literal command, so an
  interpolated secret can't leak to stdout.
- **Surfacing**: `weft check` → `hooks::validate_all` (unique ids, resolvable
  `after`/`inputs`, no cycle, pre-hooks reject `inputs`, parseable exprs);
  `describe` → a top-level `hooks` list in execution order (pre then post,
  patch render order, declaration order) + an AGENTS.md "Hooks" table with the
  effect legend; `graph` `GraphNode.hooks` (phase/effect/label/action) for the
  cloud UI's pre/post panels. Patch-file schema gains `hooks`; manifest schema
  drops `task`. Fixture `hello`'s task migrated to a `base` post-hook.

## Post-MVP — List answers & multi-select (for the web-template port)

Driven by converting a real Copier monorepo template
(github.com/SoheilSalmani/web-template) that leans hard on `multiselect`
questions (shadcn components, AI-elements, fonts). Weft had no list type.

- **`Value::List(Vec<Value>)`** — first-class list value. Serializes as a
  JSON/TOML array; a list containing a secret still refuses serialization
  (the inner secret errors). `render_text` gives a `, `-joined *display*
  form, but writing a list into path/content is a hard error
  (`RenderError::ListInContent`) — lists must be projected via an expression
  (`', '.join(x)`) first, keeping content substitution unambiguous.
  Truthiness: non-empty list is true.
- **`AnswerKind::MultiChoice { choices }`** — `kind = "multichoice"` in
  `weft.toml`; the answer is a `Value::List` whose every element is one of
  `choices` (validated in `check_type`). CLI `--answer` takes a
  comma-separated subset; `--answers-json`/presets take a JSON array.
- **Starlark list bridge (`weft-lang`)** — core `Value::List` marshals to a
  Starlark list and back (`AllocList` / `ListRef`), so `when`/`default`/
  content expressions get native `in`, `join`, and comprehensions
  (`[f for f in selected_fonts if ...]`). Secrets (and any list transitively
  containing one) are still never injected into the interpreter. A
  list-valued `default` (e.g. `selected_fonts = [font_ui, font_text, ...]`)
  builds the list from earlier scalar answers.
- Downstream surfaces updated for the new variants: engine answers
  (coerce/json/dummy), describe JSON projection, terminal `MultiSelect`
  prompt, ratatui wizard (comma-separated entry), LSP preview answers, and
  the `weft schema` mirror (`multichoice`). Gate green.
- **Computed questions** (`computed = true`): a derived value, never
  prompted, always taken from its (required) `default` — for values built
  from other answers (`selected_fonts = [font_ui, font_heading, ...]`,
  `add_dotenv = use_prisma or use_cloudinary`). Replaces Copier's
  `when: false` idiom. The wizard resolves them (so later gates see the
  value) but hides them; `describe` marks them `computed` and never
  `required`; `check` requires a default and forbids computed+secret.
- **Interpolated task commands**: `TaskAction` is now a segment sequence
  (reusing the content `Segment` model), so a command can splice in answers
  and expressions — e.g.
  `action = ["pnpm dlx shadcn@latest add ", { expr = "' '.join(shadcn_ui_components)" }]`.
  Shorthand: a bare string is a single literal (back-compatible). Rendered
  against the resolved answers right before `sh -c`
  (`render::render_segments`, shared with path rendering). `check` parses
  every command `expr`; `describe` shows a `${…}` source preview. Working
  directories are expressed with a plain `cd sub && …` in the command
  rather than a new field.
- **Gated-off questions fall back to their `default`** (Starlark
  eager-binding fix). starlark-rust binds *all* referenced free names before
  evaluating — even the untaken side of `and`/`or` — so
  `use_shadcn_ui and ('ai-elements' in shadcn_registries)` raises a
  `NameError` when `shadcn_registries` was gated off, rather than
  short-circuiting. Fix: a `when:false` question with a `default` still
  resolves to that default (its name stays defined for later
  `when`/`default`/content exprs); with no default it stays absent, and a
  gated-off default that itself can't compute is tolerated (left absent).
  Mirrored in engine `gather` and the wizard so live gating matches render.
  This lets a Copier template's short-circuit-reliant conditions port
  cleanly by giving referenced-but-optional questions an empty/`False`/`[]`
  default.

## Post-MVP — Editor track (schemas, LSP, wizard, shims)

- `weft schema` emits JSON Schemas for patches and weft.toml from
  hand-mirrored schemars structs (core/engine stay schemars-free; mirrors
  document the *file* formats incl. segment shorthands). Generated copies
  committed under `schemas/`; e2e validates every fixture and rejects
  malformed ops/kinds/stray keys.
- `weft lsp` (tower-lsp, stdio): check diagnostics mapped to the files each
  issue names with token-anchored ranges, answer-id/dep completion, hover
  with question/patch metadata, go-to-definition. Position features are
  line-heuristic by design (noted for later precision). e2e speaks framed
  LSP over stdio.
- Full-screen ratatui wizard as the interactive path of new/record: pure
  `WizardState` (declaration-order gating identical to render, provenance
  labels, readiness) unit-tested separately from the terminal loop;
  `--no-wizard` falls back to dialoguer. Wizard answers travel as the
  highest-precedence answers-json layer; engine gained
  `answers::layered_with_json` shared by new/record/wizard.
- Thin editor shims in `editors/`: VSCode extension (LSP client, bundled
  patch schema, `Weft: New Project` QuickPick flow from describe --json)
  and weft.nvim (`weft lsp` autostart + `:WeftNew` floating-terminal
  wizard).

## Post-MVP — AI-ready (contract, metadata, AGENTS.md, MCP)

- Patches carry `description`/`tags` metadata **outside the canonical hash**
  (regression-tested: documenting a patch never changes its id); questions
  gained `description`/`example`, templates a top-level `description`.
- `weft describe [--json|--agents-md]`: the agent contract — evaluated
  default previews via dummy-answer trial eval (declaration-order scope),
  `required` flags, preset contents, patch DAG with metadata, ready-to-run
  usage commands. AGENTS.md generator renders the same doc as a committed
  guide.
- `weft new --answers-json` (inline/@file/stdin) and `weft check --json`.
- `weft mcp` (rmcp 2.x, stdio): list/describe/scaffold/check plus the full
  record→commit authoring loop as typed tools. Guardrails: scaffold never
  runs template tasks; secrets never accepted as answers; worktree paths
  validated. e2e drives the real JSON-RPC protocol.
- Engine seams added for non-terminal callers: `RecordOptions.answers_json`,
  `CommitOptions.decisions` (per-answer abstraction decisions).

## Post-MVP — `weft graph`

- New `weft-engine::graph` module: `GraphDoc` (nodes/edges/questions/presets,
  serializable) and `node_diff` (render a patch's ancestor closure with and
  without the patch, diff the trees). Powers the `weft graph` CLI command and
  is the foundation for graph-based UIs (weft-cloud).
- Active-state computation mirrors `render`'s skip logic exactly; without a
  resolvable answer set the graph is structural (`active: null`).
- `answers::placeholder_secrets` provides `<secret:id>` stand-ins so
  graph/preview paths never resolve real secrets.
- `Template::ancestor_closure` extracted from `record::pin_base` for reuse.
- e2e helper fix: always run `cargo build -p weft-cli` (incremental) before
  the tests — `cargo test` builds the bin's test harness but doesn't reliably
  uplift the executable, so an existing binary could be stale.

## M6 — Hardening

- `weft check` now validates: duplicate question ids, Starlark parse of every
  `default`/`when` (questions, patches, tasks), choice lists non-empty,
  preset files parse + reference declared non-secret questions, task graph
  (duplicate ids, unknown deps, cycles, glob syntax, `answer:` refs), and
  `Segment::Answer` references inside patch ops against declared questions.
- **Commutation smoke test:** for every pair of patches with no dependency
  path between them, render `prelude + [p, q]` and `prelude + [q, p]`
  (prelude = topo order of the union of their ancestor closures) via a new
  `render_ordered` core entry point that skips the self-sorting `render`
  does — necessary because `render` itself is order-independent by
  construction and would hide non-commutation. Trees must hash identically;
  a failing order (e.g. both patches create the same file) also counts as
  non-commuting.
- Render + commutation checks need concrete answers; they resolve from
  defaults plus `--answer/--answers-file/--preset` on `check`, and are
  skipped with an explanatory note when unanswered questions remain.
- `miette` skipped (plan marked it optional): errors already carry
  patch names/ids, paths, and hunk indices via thiserror/anyhow context.
- README written with the 5-minute tutorial driving the fixture template
  through new → record → commit → new-with-different-answers → update →
  check.

## M5 — `weft update` + task graph

- `weft_core::merge` implements a diff3-flavored line merge: each side's
  edits against the base are computed with `similar`, overlapping (or
  touching) edit regions are absorbed into one region, and regions where both
  sides produce different text become `<<<<<<< local` / `>>>>>>> template`
  conflicts. Identical edits on both sides merge cleanly.
- `weft update`: old render = pinned patches + stored answers; new render =
  all patches + answers extended for any new questions (defaults/prompts;
  stored secret *references* re-resolve without re-prompting, per the plan).
  Per file: template-unchanged → keep ours; user-untouched → take theirs;
  both diverged → `merge3` with conflict markers and a summary, nonzero exit.
- Deletion policy on divergence: template-deleted + user-modified keeps the
  user's file; user-deleted + template-modified keeps it deleted. Both are
  reported as notes, never silent.
- State is re-pinned to the new template even when conflicts remain (the
  markers are in the tree; re-running update won't re-apply the same hunks).
- **Task refire:** a `ChangeSet` of paths differing between the two renders
  plus answers whose values changed (secret values are opaque — only
  presence changes count). Tasks fire only if a `glob:` input matches a
  changed path, an `answer:` input changed, or an upstream `task:` input
  fired; tasks are skipped entirely when conflicts remain. `--dry-run`
  prints the file plan and task plan and touches nothing.
- Idempotence covered by e2e: second `update` writes 0 files.
- `weft new` now stores the template path canonicalized so `update` works
  from any cwd.

## M4 — `record` / `commit`

- `weft record` renders the pinned base into
  `<template>/.weft-record/worktree/` and stores `session.toml` (pinned patch
  ids, base tree hash, answers with secrets as refs only). A second `record`
  without `--force` fails. **Deviation:** the worktree lives inside the
  template dir rather than a tempdir so the session survives across
  processes; "drop user into it" is a printed path (stdout) instead of
  spawning a shell — e2e-testable and shell-agnostic.
- `--base <ref>` accepts `latest` (default) or a patch name, pinning that
  patch's ancestor closure.
- `weft commit` re-renders the base (verifying the stored tree hash),
  diffs the worktree with `similar` grouped ops (context radius 2; change
  runs closer than the radius merge into one hunk so hunks never step on
  each other's context), and runs the value-abstraction pass.
- **Abstraction decisions:** string/choice answer values of length ≥ 2 are
  candidates (ints/bools skipped in MVP — too collision-prone); longest
  value wins on overlap; abstraction applies to *context and removed lines
  too*, because a context line derived from an abstracted segment must
  itself be abstracted or it won't match under different answers. Secret
  values are abstracted unconditionally (a literal secret in a patch file
  would be persistence). Interactive runs confirm per answer; `--yes` /
  non-interactive accepts exact matches.
- **Commit self-validation:** before writing, the candidate patch is
  replayed against the base and must reproduce the worktree byte-for-byte
  (catches ambiguous hunk contexts); the session's base leaves become the
  new patch's `depends_on`.
- Renames are recorded as delete+create in MVP (no rename detection);
  `RenamePath` stays in the model for hand-written/future patches.

## M3 — `weft new`

- Engine grew `template` (loading `weft.toml` + `patches/*.json`), `answers`
  (layering + coercion + gathering), `interact` (an `Interaction` trait with
  dialoguer-backed and non-interactive impls), `secrets`, `state`, `fsio`,
  `tasks`, and the `new` orchestration; CLI wires `new`, `presets list|show`,
  and a `check` that at least fully loads/validates the template.
- **Patch file format decision:** `patches/<name>.json` reference deps by
  *name* (file stem), and content ids are recomputed on load — this keeps
  fixture patches hand-writable (you can't hand-compute a blake3 id). Core
  patch ids remain content-addressed; the name→id resolution happens at load.
- **Precedence:** presets (CLI order) → answers file → `--answer` flags →
  secrets/defaults/prompts during gather. Duplicate `--answer` keys with
  different values are an error (equal precedence conflict); a `--answer`
  naming a nonexistent question is an error.
- Questions with defaults are auto-filled rather than prompted; only
  default-less unanswered questions prompt. Non-interactive runs (no TTY or
  `--non-interactive`) fail with the exact `--answer` flag to pass.
- Secrets can't be provided via presets/answers files (the reference comes
  from the question's declared source); state stores only the spec string
  under `[secrets]`.
- On first scaffold all `when`-passing tasks fire in declaration-stable topo
  order (`task:` inputs create ordering edges); changed-input filtering
  arrives with `weft update` in M5.
- `weft new` refuses a non-empty destination.

## M2 — Deterministic render

- `weft_core::render` implements `resolve_answers` (when-gating in question
  declaration order, provided-value type checks, Starlark defaults) and
  `render(patches, answers, eval) -> Tree`.
- **Core stays Starlark-free:** core defines an `ExprEval` trait; `weft-lang`
  implements it (`StarlarkEval`). Core tests use a stub evaluator; the
  golden-tree tests live in `weft-lang` where the real evaluator is available.
- **Determinism:** patches apply in topological order with ties broken by
  patch id, so the input ordering of the patch list never matters (covered by
  a test that reverses the input and compares tree hashes).
- **`when`-skipped patches skip their dependents transitively** rather than
  erroring; a dependent's ops usually target files the skipped patch created.
- Hunk application requires the rendered pattern (`context_before + removed +
  context_after`) to match at exactly one position: zero matches and multiple
  matches are distinct, clean errors. A fully-empty pattern means "append at
  end of file".
- Rendered paths are validated: relative, `/`-separated, no `..`/`.`/empty
  components — answer values cannot escape the destination tree.
- `starlark` 0.14 changed module construction to a scoped
  `Module::with_temp_heap` API; evaluation happens inside that scope.
  Secrets are never injected into the Starlark environment.

## M1 — Data model + serialization

- All plan types implemented in `weft-core` with serde round-trips.
- **Canonical form:** compact JSON with fixed struct key order; a patch's id
  is blake3 of the canonical body (`depends_on` sorted dep *ids*, `when`,
  `ops`) — the id itself is never inside the hashed bytes, so ids form a
  Merkle DAG.
- **Hand-writable patch files:** `Segment`/`Line`/`TemplatePath` serialize
  fully-literal values as plain JSON strings and abstracted values as
  `{"answer": id}` / `{"expr": src}` objects. Fixture patch files can be
  written by hand; ids are recomputed on load (files don't embed their id).
- **Content model decision:** text-only (UTF-8) for MVP; rendered files are
  normalized to end with exactly one trailing newline (empty files stay
  empty). Binary files and exact-trailing-newline preservation are post-MVP.
- Secrets: `Value::Secret` fails serialization with an explicit error and is
  redacted in `Debug`; only `SecretSpec` references are serializable.
- `deny_unknown_fields` can't be combined with the flattened `kind` on
  `Question` (serde limitation) — stray-key validation deferred to
  `weft check`.
- Property tests (proptest): serialize → deserialize → serialize is
  byte-identical for ops and canonical patch bodies. Insta snapshots pin the
  canonical forms and a fixture patch id.

## M0 — Skeleton

- Cargo workspace with `weft-core`, `weft-engine`, `weft-lang`, `weft-cli`,
  plus `tests/e2e` as a workspace member package (integration tests via
  `assert_cmd`; run with `cargo test --workspace` so the `weft` binary is
  built first).
- **Deviation:** `starlark = "0.13"` (the version current when the plan was
  written) no longer compiles — its `allocative` dependency drifted against
  newer `hashbrown`. Pinned `starlark = "0.14"` instead; same API surface for
  our use.
- The reference design docs mentioned in PLAN.md (`weft-mvp-design.md`,
  `weft-record-mvp.md`) are not present in the repo; PLAN.md is the sole
  source of truth.
- CI gate is `cargo fmt --check && cargo clippy --workspace --all-targets
  -- -D warnings && cargo test --workspace`. All green.
