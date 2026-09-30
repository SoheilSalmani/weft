# Progress log

Running notes per milestone, as required by PLAN.md. Records decisions and
deviations from the plan.

## Post-MVP — Hook `before`

- **Why**: a template that `extends` a base inherits its hooks, and the
  house `base` ends with a `git-commit` post hook that must run after every
  setup hook. A stack extending it (dbt) adds setup hooks (`uv-sync`,
  `dbt-deps`) that must run before that inherited hook, but only `after`
  existed: the base cannot name its extenders' hooks (it knows nothing of
  them, and `weft check` flags unknown `after` refs), and base order puts
  the base's hooks first.
- **What**: `Hook.before: [ids]`, the mirror of `after`: `before: ["x"]` on
  `h` adds the edge h → x, resolved relative to `h`'s frame like `after`
  and ignored at run time when `x` is outside the phase set. Hooks are
  metadata outside the patch hash, so ids do not change. `weft check`
  reports an unknown ref as ``hook `h`: unknown `before` hook `x` `` and its
  cycle detection includes `before` edges; cycle errors now read `hook
  ordering cycle among: …`. `weft hook add --before ID` (repeatable);
  describe and graph JSON show `before` next to `after`, and describe's
  hook order honours it. Schemas regenerated.
- **Tests**: unit (`before_edges_reorder`,
  `root_before_runs_ahead_of_an_earlier_child_hook`,
  `cycle_mixing_after_and_before_is_an_error`,
  `unknown_before_ref_is_a_check_issue`,
  `check_flags_a_cycle_mixing_after_and_before`); e2e
  `extender_hook_runs_before_an_inherited_hook` scaffolds an extender whose
  hooks say `before: ["commit"]` and asserts they ran ahead of the base's
  `commit`.

## Fix — name every clash between an extender and its base, and how to settle it

- **Decided: names stay shared across `extends`.** Namespacing question ids
  by template was considered and declined: one template can fill several
  include slots (the slot name, not the template name, is what tells them
  apart), template names are neither unique nor stable, and a shared id is
  what lets a base's question be the extender's own, move between base and
  stack without renaming stored answers, and be narrowed by `[refine]`.
  Includes keep their slot namespaces (`web.x`, `connector.<key>.x`).
- **The cost is clashes**, including a base gaining a name an extender
  already uses. The errors now list every clash at once (a stack moving onto
  `extends` meets all its duplicates together) and say how to settle each:
  a question: delete it from the extender to use the inherited one,
  narrowing it with `[refine.<id>]` if it must differ, or rename it; a
  patch: delete a copy of the inherited patch, or rename it and the
  `depends_on` entries naming it (the id is unchanged); a hook id (a `weft
  check` issue that named nothing): the patches declaring it, the
  extender's own first so the LSP anchors it in an editable file.
- README notes the shared namespace for base authors. Verified by
  scratch runs of each clash and each fix, including adopting a base's
  computed question through `[refine]` and a renamed patch keeping its id.

## Post-MVP — TUI removed until after v1 (BREAKING: `--no-tui`, `--no-wizard`, `--reconfigure`)

- **Why**: PLAN.md's MVP non-goals include "no GUI/TUI beyond plain
  interactive prompts". The ratatui layer (answers wizard, completion forms,
  preset-authoring wizard: ~3,300 lines, a third of `weft-cli`) kept a second
  answer resolver in `WizardState` that every question-model feature had to
  mirror, and its own terminal rules (the `$(…)` fix below). Decided with the
  user: remove it now, reconsider after v1. The TTY check and shell quoting
  that `weft session shell` borrowed from `tui/` moved into `shell.rs`.
- **The trap it hid**: `answers::gather` prompts only a question with no
  answer and no default, so without the wizard an interactive `weft new`
  would take every `use_<concern>` default silently. New
  `answers::gather_reviewed` (`new`, `session new`, `patch amend`) asks every
  open, promptable question no input answered, offering its default. A
  multichoice a preset constrains is asked with the blocked choices hidden and
  the fixed ones pinned, starting from the preset's selection
  (`PresetConstraints`, which replaces `Layered.locked`). `Gathered.entered`
  holds what was typed that differs from the offer; `weft new` stores it as
  given, so an accepted default stays derived and follows the template, as an
  untouched wizard row did. `NonInteractive::ask` now takes an offered default
  (as `confirm` already did), so unattended runs resolve exactly as before;
  `gather` is unchanged for every other flow.
- **Commit name**: without `--name`, commit asks for one, offering
  `patch-NNN` (`commit::ask_name`); unattended (no TTY, or `--yes`) takes
  `patch-NNN`, as before.
- **Removed**, no aliases (pre-1.0): `--no-tui` (`new`, `commit`, `hook add`,
  `patch set`, `instance add`), `--no-wizard` (`new`, `session new`, `patch
  amend`), `weft update --reconfigure` (use `--answer`/`--unset`), `weft
  presets save --non-interactive` (save never prompts now), and the `weft new`
  template picker (TEMPLATE is required); with them `AnswerChanges.typed` and
  `AmendOptions.answers_json`. clap now requires `hook add` PATCH, `--id`,
  `--phase`, `--effect` and `--label`, `patch set` NAME, and `instance add`
  INCLUDE KEY; `hook add` without `--action` still opens `$EDITOR`. The offer
  to save answers as a preset after `weft new` fires once a person typed an
  answer. `weft add -p` reads its keypresses through `crossterm` directly.
  `weft describe`'s `usage.author` passes `--non-interactive` where it passed
  `--no-wizard`.
- Tests: engine unit tests for review (defaults offered, only changed answers
  entered, an unattended review resolves like `gather`, a constrained
  multichoice); the two e2e tests that pinned the forms' fallback messages are
  gone and the rest drop the removed flags. README, nvim docs, the schema's
  `section` text and the weft skills follow.

## Post-MVP — `weft session shell` and `weft session new --shell`

- **The gap**: entering a session meant `cd`-ing into the path `weft session
  new` printed; nothing opened a shell there, or started an editor or an
  agent with the worktree as its directory.
- **`weft session shell [NAME] [--template DIR] [-- CMD…]`** (`shell.rs`):
  a `weft session` subcommand, so the session resolves as for `session path`,
  the template's only one included (ADR-0004). Bare, it opens `$SHELL` (else
  `/bin/sh`) at the worktree root and needs a terminal; without one it fails
  at once and names `-- CMD`, since a shell on a pipe would run whatever it
  reads. With `-- CMD` it runs that argv there, without shell parsing or a
  terminal, and exits with its status; a command that cannot start exits
  127/126 like `env`. On Unix weft `exec`s, so Ctrl-C, job control and the
  exit status belong to the child and no weft parent is left to die on
  SIGINT; elsewhere it waits and propagates. The shell gets
  `WEFT_SESSION=<name>` for prompts; a `-- CMD` child does not, so an editor
  or agent started that way is never told to `exit`. weft reads it only to
  word hints, never to pick a session, keeping the rule that patch-writing
  commands never choose one silently. Every child gets a `PWD` that matches
  its directory. A session whose worktree is missing is an error naming
  `session end --discard`, not a misleading "cannot run `$SHELL`".
- **`weft session new --shell`** refuses without a terminal before anything
  is rendered, prints the path as before, then opens the same shell. No
  `-- CMD` on `new`: it would read like `--exec`, whose output becomes the
  patch.
- **Hints**: the session-start and amend hints name the exact `weft session
  shell` command, with `--template` when the current directory would not
  find the template, relative to that directory when the template sits below
  it (`start::announce`/`amend::announce` moved to the CLI). The error a
  worktree command gives outside a worktree (ADR-0004) names the same command
  instead of `cd "$(weft session path NAME)"`. A commit or `session end` that
  deletes the worktree its own session shell stands in says that `exit`
  returns to where the shell was opened.
- **Docs**: the README tutorial records through `weft session new --shell`
  and leaves with `exit`; weft-cloud's pages for people do the same, and its
  tutorial replay runs `--shell` and `weft session shell` in a pty with a
  stand-in shell.
- **Decided against**: an "open a shell?" prompt after `session new` (a
  preference, not missing input, so a flag or an alias holds it; and a new
  trailing prompt is one more place for an agent in a pty to block); an
  editor command or an editor setting (`$VISUAL`/`$EDITOR` mean "edit this
  buffer and block", wrong for opening a folder with `code --wait` or
  `nano`; `-- code .` covers any editor, and weft has no user config file);
  shell integration for a real `cd` (per-shell init scripts and rc edits;
  revisit if nested shells annoy).
- Tests: e2e `session.rs` covers a command's directory and `WEFT_SESSION`,
  its exit status and 127, a bare `shell` without a terminal refusing without
  running its piped stdin, and `new --shell` without a terminal leaving no
  session. Smoke in a pty (the interactive paths need one): the wizard,
  submitted with `s`, hands a restored terminal (`icanon echo`) to the shell
  `--shell` opens, whose exit status weft returns; nesting prints its note;
  both exit hints and the `--template` form of the start and amend hints
  appear only where they should.

## Fix — the answers wizard opened inside `$(…)`

- **Bug**: `cd $(weft session new …)`, the documented way into a session,
  hung for a person at a terminal. The wizard was gated on stdin alone and
  draws on stdout, which `$(…)` captures, so it waited behind a blank screen
  (in a pty: still blocked after 4 s, while `--no-wizard` returned the path
  at once). The README's own tutorial line did this.
- **Fix**: `maybe_wizard` and `update --reconfigure` now use
  `tui::interactive`, the stdin-and-stdout check the forms already used.
  Inside `$(…)` missing answers are asked line by line on stderr, as with
  `--no-wizard`, and the capture holds only the path.
- No regression test: the bug needs a pty and the e2e harness has none.
  Smoke in a pty: `$(weft session new …)` returns the path with every answer
  given and with one asked on stderr.

## Post-MVP — Worktree commands need a worktree (BREAKING)

- **The reported pain, again** (the `weft add init` report under "Sessions
  are git-style worktrees" never fully went away): at the template root with
  one session, `weft status` worked and `weft add README.md` staged the
  *worktree's* README while the shell completed the *template's*. A
  tab-completed `.weft-sessions/default/worktree/x.txt` matched nothing,
  printed `staged 0 path(s)`, exited 0, and the next `weft commit` took the
  whole worktree (an empty stage means "commit everything"). The same
  `status` failed once a second session existed.
- **Rule** (`weft-cli/src/ctx.rs`): `status`, `add`, `reset`, `diff` and
  `commit` act on `--session NAME` or the worktree you stand in;
  `Scope::resolve` no longer falls back to `Session::only`, not even with one
  session. With `--session` from outside the worktree, paths are relative to
  the worktree root (git's `--work-tree`). The error lists every session and
  its worktree and ends with `cd "$(weft session path NAME)"` /
  `--session NAME` (the real name when there is one session). The template
  root is weft's bare repository; git refuses `git status` in one too.
- **Unchanged**: `weft session scope|refresh|path|end` still take the only
  session (`Scope::resolve_or_only`): they manage sessions from the template,
  take no paths, and `session refresh` right after editing `weft.toml` at the
  root is the natural flow. Staging was already per session
  (`.weft-sessions/<name>/stage/`, the layout of git's
  `.git/worktrees/<name>/index`). MCP passes session names explicitly.
- **A pattern that matches nothing is an error** (`stage::unmatched`,
  `ctx::refuse_unmatched`): `weft add PATTERN…` (and `-p PATTERN`) and
  `weft reset PATTERN…` fail before touching the stage and name each dead
  pattern; one that matches only a base path still stages a deletion. The
  hint names the likely cause: from outside the worktree paths are
  root-relative; an adopted session's scope needs `weft session scope --add`.
  `-A`, bare `-p`, bare `reset` and `.` are exempt.
- `weft status` outside a worktree no longer prints "no session" and exits 0.
  `weft describe`'s `usage.author` still said the removed `weft record` and
  `weft commit --template DIR`; it now enters the worktree with
  `cd "$(weft session new <patch> --template DIR … --no-wizard)"` and commits
  from there.
- e2e: 73 invocations in 14 files name their session; `worktree.rs` gains
  `the_template_root_never_implies_a_session` (old behaviour: `status` at the
  root with one session succeeded) and
  `a_pattern_that_matches_nothing_changes_nothing`; a stage unit test pins
  that a deletion counts as a match. ADR-0004 (Accepted) records the rule.
  Gate green: `cargo fmt --all --check && cargo clippy --workspace
  --all-targets -- -D warnings && cargo test --workspace`.
- Known limit: a session pins its base patch ids, so amending a patch another
  open session builds on strands that session (`status` and `session refresh`
  both say "pinned base patch … no longer exists"). New patches from parallel
  sessions are fine.

## Post-MVP — Slots: several patches adding to one file (`fill_slot`)

- **The gap**: only one patch could create a file, and siblings inserting
  after the same anchor do not commute. The house templates worked around it
  with `expr` lines on several bools (`mcp`), a gated owner split into
  complementary patches (`lightdash-mcp` / `-standalone`), and dependency
  chains that exist only to order lines (`[tools]` in `mise.toml`, Gradle
  blocks in `java`).
- **Model** (format-agnostic, line level): a slot is declared as a line of the
  lines a patch adds — `create_file` content or a hunk's `added` — written as
  the object itself, `{"slot": "servers", "separator": ","}`
  (`Segment::Slot(SlotDecl)`, only ever a whole `Line`; a slot inside a
  segment array does not parse). `fill_slot {path, slot, key, lines}` adds a
  contribution. `create_file` gains `omit_when_empty: [slot…]`. Both new
  fields are skipped when empty, so existing ids are unchanged (the canonical
  snapshot still passes).
- **Render** (`weft_core::draft`): ops now apply to a `Draft`, finished into
  a `Tree`. While open, a slot is one NUL-bearing marker line no hunk can
  match, and fills are kept aside by key; `finish` renders contributions in
  key order, the separator ending all but the last, an empty slot as zero
  lines, and drops a file whose `omit_when_empty` slots are all empty. So
  hunks never see slot content and fill-only patches commute by
  construction, whatever the order. Errors: duplicate key in one slot (names
  both patches), unknown slot, fill on a missing file, empty fill, bad or
  duplicate slot name, a slot line anywhere else (`MisplacedSlot`, so slots
  do not nest), a lost marker.
- **Decided** (the open questions): slots may open in a hunk's `added` lines
  (a patch editing a file can offer a place, e.g. `[env]` in `mise.toml`);
  they do not nest; a file `omit_when_empty` drops while another patch
  changed it by anything but a fill is a render error naming both patches
  (dropping the edit silently, or keeping a file with empty slots, were the
  alternatives); keys are strings or segment arrays (a foreach patch needs
  the instance `key` in its key).
- **Recording** (`diff::record_text_change`): commit gets the base's trace.
  A slot owns every line added from just after the line before it to just
  before the line after it; its existing contributions may only move (their
  separator follows their position). The new block becomes a `fill_slot`
  under the patch's name (`<name>/{key}` in a foreach session; an amend or a
  resync keeps the keys the patch already used), the trailing separator
  stripped, and it must render back exactly: out of key order, split in two,
  or editing another patch's contribution is refused with the expected
  layout. Hunks are taken with each slot collapsed to one line that never
  anchors, so no context comes from slot content. `weft diff` notes the
  lines that will fill which slot, keyed by the session's name. A file the
  base left out (`omit_when_empty`, slots empty) keeps its would-be text in
  the trace (`Trace::omitted`), so a worktree that has it is diffed against
  that: the first filler of such a file, and an amend of its only filler,
  record a `fill_slot` instead of a `create_file` that clashes with the owner.
- **Check**: static slot validation; the duplicate-key render error is
  reported once per pair of patches, whichever applied first, with both
  names; every report built from a render error names patches instead of
  ids; pairs whose only shared files are ones both merely fill (under
  different keys) are skipped and counted as commuting by construction.
  `weft patch amend` refuses a patch that declares slots, and `weft patch
  resync` skips one: re-deriving ops from a worktree or a command's output
  would drop them.
- Surfaces: graph/describe op kind `fill_slot` (`path#slot`), schemas
  regenerated (both copies; slot lines only allowed where they are legal).
- Tests: draft unit tests (key order both ways, separators, omit, errors,
  hunk-opened slot, hunks never seeing slot content), canonical forms, a
  diff test with a hunk and a fill in one file, e2e `slots.rs` (siblings +
  check, omit, duplicate key reported once, an omitted file's edit named in
  check, commit records a fill and replays it, the first fill of an omitted
  file, amending its only fill, key order refusal, amend refusal), resync
  refusal (`generate.rs`), schema forms. The weft-cloud server's recording
  (`render_traced` added for flat renders) was migrated and smoked over
  HTTP. Smoke: the `dbt` template's
  `mcp` converted to three slotted files with `linear-mcp`/`jira-mcp` fills;
  a recorded `lightdash-mcp` (`--depends-on mcp,lightdash --when
  use_lightdash`) passes check under all eight tracker/Lightdash
  combinations, replacing the two-patch split.

## Fix — `weft commit` takes hunk context from `expr` output

- **Bug**: context lines were whatever the base rendered next to the change,
  including lines rendered from `{"expr"}` segments. Commit replays under the
  session's answers only, so it accepted context that exists only under those
  answers (`lightdash-mcp` anchored on `mcp`'s Linear line), and `weft check`
  under other answers failed every pair with "do not commute: one
  application order fails".
- **Fix**: rendering can be traced (`Draft::traced`, `Trace`): every line of
  every text file carries its node and source (plain, `expr`, slot). The
  session base is rendered traced, and `hunks_between` takes context only from
  lines that anchor, stopping at the first one that does not (fewer lines,
  down to none) and splitting a group at an interior one. A change left with
  no pattern is refused unless it appends at the end of the file.
- **Check**: every node is rendered on top of its own ancestors first. One
  that fails there fails every order: it is reported once as "does not apply
  under these answers", and it and its descendants stay out of the full
  render and pair reports. For a hunk that matched nowhere a traced render
  gives the near miss: the expected line, the file's line there and the
  patch and segment that rendered it (or the slot the context straddles).
  The repro went from 14 issues to one naming `mcp`'s `expr` segment.
- `RenderError::HunkNoMatch` gained `near`, op errors now carry the graph
  node id (a keyed id for an include's patch), and `RenderError::patch()`
  says which node an error is about.

## Fix — `weft patch resync` answers

- `--answer` overrides are stored in the generator metadata even when the
  output is up to date (they were only saved on a rewrite). A
  `--keep-literal` repair changes how the output is recorded, not what it
  renders, so it always re-derives the ops: an occurrence an earlier resync
  abstracted goes back to literal with the command's output unchanged, and
  when the ops come out the same the patch is left alone and only the specs
  are stored.
- Answers are gathered like `weft update` does: stored answers and overrides
  provided, secrets re-resolved, questions added since recording take their
  defaults. Filling a default silently could abstract it into the regenerated
  text, so answer references the previous version did not have skip the patch
  with their occurrence keys until `--yes` (new flag) or `--keep-literal`.
  A rewrite stores the full answer set it rendered with.

## Fix — `weft commit --describe` in an amend session

- `--title`, `--describe` and `--tag` were silently ignored when finishing
  `weft patch amend`; they now apply as `weft patch set` does, in the same
  save as the ops. `--when`, `--depends-on`/`--after` and a different `--name`
  are refused before anything is written (amend keeps name, gate and
  dependencies). The CLI skips the name form and MCP drops its required name
  for an amend session.

## Post-MVP — Session names are optional (`default`)

- **`weft session new [NAME]`** and **`weft session adopt PATH [-n NAME]`**
  default the name to `default` (`session::DEFAULT_SESSION_NAME`). Chosen
  over a counter (`session-NNN`): a fixed name is something a command can
  target without listing sessions (a planned "open a shell in a session"
  command goes to `default` when given no name), a second bare `new` stops at
  "already exists" instead of piling up forgotten worktrees, and parallel
  sessions get names worth typing after `--session`. Same idea as jj's and
  terraform's `default` workspace.
- The pre-multi-session `.weft-record/` now migrates to `default` instead of
  `main`: it was the one unnamed session. Templates already migrated keep
  their `main`.
- Unchanged by decision: MCP still defaults to `agent`, so an agent's session
  never collides with the author's `default`; `weft patch amend` still names
  its session after the patch; `--force` on a bare `new` replaces `default`,
  exactly as with an explicit name. Resolving the session for `add`/`commit`/…
  (`Session::only`: the one you are in, else the only one) gets no `default`
  fallback when several exist, so a patch-writing command never picks one
  silently.
- Hints that spelled `weft session new NAME` / `N --base` drop the name.

## Fix — `weft update` prunes the directories its deletions empty

- **Bug**: an update that deleted a template file left its directory
  behind, empty. Found by the refine tutorial: handing a skill back to the
  template deleted `.agents/skills/typescript/SKILL.md` but `ls
  .agents/skills` still listed `typescript`, a skill directory with no
  skill in it for anything that scans the directory (a symlink farm for
  another agent, say). `weft instance remove` left instance directories the
  same way.
- **Fix**: `fsio::remove_file` deletes the file, then each directory above
  it that the deletion emptied, stopping at the first one that still holds
  anything and never removing the project directory, as `git checkout`
  does. Update's `Delete` action uses it.
- Unit test in `fsio`; e2e `refine.rs` asserts the emptied skill directory
  is gone after the update.

## Post-MVP — Extenders refine inherited questions (`[refine]`, ADR-0002)

- **The gap**: `extends` imported the base's questions as-is and
  redeclaring one was a load error, so a stack template could not
  pre-select its own skills, require one, or hide another. Template
  families copied the base's patches and questions instead.
- **`[refine.<id>]`** in an extender, per inherited question: `prompt` /
  `description` / `example` (reword), `default` (soft), `lock` (the only
  value; never asked), `choices` (allow-list) or `blocked` (deny-list) for
  choice/multichoice, `fixed` for multichoice. Only narrowing: no new
  choices, no kind change, no secrets, nothing a template higher up the
  chain narrowed can be undone (`blocked`/`fixed` accumulate, the nearest
  `default` wins, a lock is final). Structural mistakes are load errors
  (`refine::apply`), all starting ``refine `<id>`:``.
- **Representation**: applied once at load in the `extends` branch of
  `Template::load_inner`, so every reader of `manifest.questions` sees the
  effective question — choices already narrowed, default replaced (a lock
  is the default plus `narrowing.locked`) — and a new
  `Question.narrowing { locked, fixed, blocked, by }` (never read from
  `[[question]]`). No new plumbing for describe/graph/MCP/LSP/update.
- **Enforcement**: core `render::narrow(q, value, ValueOrigin::Input |
  Default)` — an input picking a blocked choice is `RenderError::Blocked`,
  a default drops blocked multichoice options (a blocked `choice` default
  errors), fixed choices are appended after the selection (its order kept,
  so no answer is ever reordered). `resolve_answers` narrows every value and
  holds locked questions to their lock (`RenderError::Locked`; a
  multichoice compares as a set). `answers::gather` narrows on the way in
  so later gates see the render's values. Presets were the wrong substrate
  (one-shot per command, not stored, never applied to defaults) and are
  unchanged.
- **Decided (ADR-0002)**: answers the user gives stay given, fixed choices
  included — the template never rewrites `[answers]`, so relaxing `fixed`
  never removes anything from a project that answered. A stored answer the
  template no longer allows stops `weft update` with a hint (`--answer`, or
  `--unset` back to the template's value), like a removed choice already
  did. `provenance` only changed `prompted` to `is_promptable()` (a
  multichoice whose every choice is fixed is decided, not asked).
- **Surfaces**: `describe --json` adds `locked`/`fixed`/`blocked`/
  `refined_by`, previews the narrowed default, and never marks a locked
  question required; AGENTS.md shows `locked` and `(fixed: …)`. The wizard
  hides template-decided questions (still in scope for gates), pins fixed
  choices, names the template when a typed choice is blocked, and refuses
  to continue on a provided answer the narrowing rejects; the choice popup
  cursor now indexes visible choices. The sequential prompt offers only
  free choices ("always included: …"). `weft check` reports refined
  defaults/locks that mention later questions or pick blocked choices, an
  inherited `choice` default a refinement blocks, and binds onto locked
  questions; `PresetSpec::validate` rejects presets answering locked
  questions, fixing blocked choices, or blocking fixed ones. LSP: `[refine.`
  completes inherited ids, keys complete inside the table, hover shows the
  narrowing and where the question is inherited from, goto follows the
  `extends` chain, and refine errors anchor on their table. Schemas
  regenerated (both copies).
- Fixtures `skills-base` + `skills-dbt`; tests: core narrowing (7), load
  rules and chains (4), check (2), preset validation (1), wizard (2), e2e
  `refine.rs` (8) and LSP (3).
- Not done (by decision): refining questions of includes (binds still only
  seed); ordering an extender's hook before an inherited one (`before`), a
  prerequisite for moving template families that end in a final commit
  hook onto `extends`.

## Post-MVP — Change a project's answers (`weft update --answer`)

- **Provenance**: `.weft/state.toml` now splits answers into `[answers]`
  (given: supplied on any input layer, or prompted for) and `[derived]`
  (defaults, computed values, include binds, the stand-in value of a
  gated-off question) — root and per instance (`answers::provenance`). The
  old render reproduces from given ∪ derived; the new render feeds back only
  given, so derived values re-derive. Decided: derived values follow the
  template on *every* update (a template changing a default reaches new
  projects; the 3-way merge protects edits). A given answer for a gated-off
  question is kept for when its gate opens. Pre-feature state has no
  `[derived]` → every answer counts as given (unchanged behavior);
  `--unset ID` releases one, and the update lists `kept as given` values
  that matched their default/bind before and no longer do.
- **`weft update`** gained `--answer/--preset/--answers-file/--answers-json`
  (same layering as `new`, namespaced include answers routed through
  `compose::route_answer`, now shared with `find_question`/`split_provided`),
  `--unset ID` (needs a default or bind to fall back to), and
  `--reconfigure` (wizard prefilled with given answers; `x` hands a value
  back to its default — `wizard::run_reconfigure`). Answers for an instance
  the project lacks point at `weft instance add`. The git-source "up to
  date" early exit is skipped when answers change. Before writing, the
  update prints every changed answer (declaration order, tagged
  `(derived)`/`(bind)`); `UpdateReport` carries it structured.
- **Conflict DX**: `[state] conflicts` records files left with markers; the
  next update refuses while they still contain them. Inside a git work tree
  the update refuses to write over files with uncommitted changes
  (`vcs::dirty_paths`, `git status --porcelain -z`; `--allow-dirty` —
  also on `instance add/remove`); outside git nothing is blocked.
  `--dry-run --diff` prints unified diffs (stdout). Notes now name their
  file (`Dockerfile: kept your version: …`).
- **Fixes on the way**: instance child secrets are passed through on the new
  side (a `prompt` secret was re-asked on every update); `instance add`
  rolls its pin back when the update stops before pinning anything;
  `adopt` reproduces from given ∪ derived.
- **`weft answers [DEST] [--json]`** lists each answer with its origin
  (`given`, `given (question off)`, `derived`, `secret`). **MCP**
  `update_project` (answers, unset, dry_run, allow_dirty; hooks never run).
- e2e `update_answers.rs` (8), plus git (same commit + answers), MCP
  (`update_project`), compose (bound child answer is derived); unit tests
  for provenance, porcelain parsing, and the wizard's reconfigure mode.
- Not done (by decision): holding the template still during an answer
  change — run `weft update`, commit, then change answers.

## Fix — remote `extends`/includes resolve in every command

- **Bug**: `weft session new` in a terminal failed on a template with
  `extends = "gh:…"` (``include `extends` references … (a remote
  template)``). The wizard, preset capture, the TUI forms (incl. `weft
  commit`'s), `patch ls/set/set-command/detach/squash`, `hook add/rm/ls`,
  `presets save/rm`, amend's rebase check, and the MCP tools loaded with the
  engine's path-only `Template::load`; only the main command paths used the
  remote resolver.
- **Fix**: every CLI/MCP load goes through `source::load_template` /
  `source::with_resolver` (standard `RemoteResolver`, lock written back);
  engine functions that load (`author::*`, `preset::{save,remove}`,
  `squash::squash`, `amend::finish`) take the resolver. The LSP resolves
  **offline** (lock + cache only, no lock writes) so editing never blocks on
  a fetch. The template picker reads only each manifest.
  `Template::load` remains for path-only uses (tests).
- Messages name the base as `` `extends` `` rather than ``include
  `extends` `` (`IncludeDecl::label`; the synthetic decl is `(extends)`).
- e2e `git.rs::authoring_commands_resolve_a_git_extends`; the TTY wizard
  path smoke-tested under `script`.

## Post-MVP — Git template sources (`<repo>[//<subdir>][@<rev>]`)

- **Third source kind next to path and `hub:`**: `gh:owner/repo` (→
  `https://github.com/owner/repo.git`), any git URL (`https://`, `ssh://`,
  `file://`, scp `git@host:path`), with `//subdir` for a repository of
  templates and `@rev` (tag/branch/commit; absent = remote default branch).
  Parsing/classification = `weft-engine/src/source.rs` (pure; the engine
  never touches the network). `@` is found in the *path* part only, so
  `git@host:` and `user:tok@host` never read as a revision. Rejected
  alternatives: npm `#rev` and Cargo `?tag=&dir=` (shell quoting; `@`
  already exists for `hub:…@version`).
- **Cargo-shaped tracking**: `state.template` says what to track
  (`gh:acme/templates//base@main`, no machine paths), new
  `state.commit` says what rendered. `weft update` fetches, re-resolves the
  rev, short-circuits "up to date" when the commit is unchanged, else runs
  the engine update with `UpdateOptions.stored` so the ref is written back.
  `--to REV` retargets (git rev or hub version — replaces the old
  "wait for `--to-latest`" note); `--offline` uses the mirror only.
  `--template DIR` keeps meaning "switch to this local path".
- **BUG FIXED on the way**: `weft update` on a `hub:` project wrote the cache
  directory into `state.template` (update.rs canonicalized
  `template_override`). `StoredSource` now travels through
  `NewOptions.stored` / `UpdateOptions.stored` / `ProjectTemplate`
  (instance add/remove); `State::new` takes a `StoredSource`. e2e
  `hub.rs::update_keeps_the_hub_ref_and_moves_with_to`.
- **Cache** = `weft-cli/src/git.rs`, Cargo's layout: `~/.weft/git/db/<slug>-
  <hash>/` bare `--mirror` clones (fetch `--prune`), `~/.weft/git/checkouts/
  <slug>-<hash>/<sha>/` immutable **whole-tree** exports (`git archive` →
  the same safe tar unpack as hub). Whole tree, not the subdir, so
  `templates/base` can `[[include]] template = "../shared"`. A full sha with
  an existing export = zero network, zero mirror (pinned + cached, same
  guarantee as hub). Shells out to `git` (user's SSH keys / credential
  helpers apply; `GIT_TERMINAL_PROMPT=0` when stdin isn't a tty). Mirror
  HEAD is set at clone time only (a remote renaming its default branch
  needs `--to`). Wrong `//subdir` lists every `weft.toml` in the export
  (depth ≤ 4). **Concurrency without a lock file**: every process builds
  into its own `<target>.staging-<pid>` and `install` is first-writer-wins
  (`rename` fails onto a populated dir → adopt the winner, drop your copy);
  a populated db/export is never `remove_dir_all`'d, since another weft may
  be reading it. NOTE: `hub::ensure_cached` still uses the older
  remove-then-rename pattern — same treatment would apply there.
- **Includes**: `[[include]] template = "gh:acme/tpls//go-service@v1"`; no
  new field — `version` stays hub-only (check + resolver reject it on git
  with a `@rev` hint). `weft.lock` entries gained `commit`; `version`/
  `sha256` became `Option` (hub-only; existing lock files load unchanged).
  TODO weft-hub: its `RegistryResolver` constructs/reads `Locked` — adapt
  to `Locked::hub(..)` / `Option` fields when bumping the weft crates. Lock
  key = `repo[//subdir]`, `req` = rev or `HEAD`; a changed rev invalidates
  the entry like a changed semver req does. `HubResolver` →
  `source::RemoteResolver` (hub + git + path).
- **Cached templates are read-only**: a parent root under `~/.weft/{hub,git}`
  never gets a lock written; an unpinned include there errors with "author
  must run `weft lock` and commit weft.lock". (Previously a hub cache copy
  could be mutated by lock writes.)
- **Adopt/record**: a project scaffolded from a remote source can't be
  adopted without `--template DIR` (a cache export is not a writable
  clone) — explicit error instead of "weft.toml not found".
- **What to commit in a project**: `.weft/state.toml` + `.weft/base.json`
  (portable now that remote refs carry no paths); `.weft/worktree.toml` is
  machine-local — `WorktreeLink::save` writes `.weft/.gitignore` excluding it.
- Deleted `update::is_up_to_date` (no callers). `hub` download message no
  longer prints `@version` twice. `schemas/*.json` regenerated (the patch
  schema had drifted: generator + create_binary_file).
- e2e `tests/e2e/tests/git.rs` (9, all against local `file://` repos built
  from the hello fixture): subdir@tag pins commit + whole-tree export,
  branch follow + up-to-date no-op, tag stays until `--to`, sibling include
  inside the export, wrong subdir listing, pinned-sha offline + `--offline`
  branch, git include lock (commit pinned, tag moving upstream doesn't move
  the child, `version` rejected), fetched template must ship its lock,
  adopt hint. Gate green: fmt + clippy -D warnings + `cargo test
  --workspace`.
- Follow-ups: semver over tags (`@^1.0`), `weft cache gc`, SSH default for
  `gh:` (`WEFT_GH_PROTOCOL`), `weft check`/`graph` accepting sources, MCP
  scaffold from git refs, nested instance bases in `base.json`.
## Post-MVP — Composed graph: `extends` and include nodes

Templates compose as one patch graph. Two relationships, one engine concept
(a **frame**: mount prefix + answer namespace each node renders in):

- **`[template] extends`** is an import, not a mount: the base template's
  questions, includes, and patches load into the extender's graph as-is
  (same names, same ids — projects scaffolded from the base see no rewrite).
  Name/question-id collisions are load errors; inherited patches are not
  files of the extender (`patch_file` refuses; amend/squash/resync send you
  to the base). Path or `hub:` refs (`{ template, version }`) through the
  same `IncludeResolver`.
- **Single includes are not opaque**: every child patch is a node
  `<include>/<name>` (nested `web/svc/base`) keyed by the include
  (`PatchId::keyed`), so the same child mounted twice is two nodes and the
  hash of a parent patch depending on `web/next-config` embeds the keyed id.
  Gate inheritance holds across frames (`render_framed` skips dependents of
  a gated-off child node). Root mounts (`path = ""`) are legal; a collision
  is a `create_file`-exists render error.
- **Repeat includes stay opaque**: one `Instances` node per include (id =
  hash of child node ids + mount + binds), reachable only through `foreach`;
  a `depends_on` naming it is a load error. Foreach patches may now reach
  inside an instance (`["connectors/", {"answer": "key"}, "/x"]`).
- **Render** (`compose::render_composed`): one topological pass over
  `graph_nodes` (root + every part's patches, keyed through the include
  chain) via `weft_core::render::{framed_order, render_framed}`, then the
  foreach phase per level, deepest first. Parent-first is the no-edges
  special case. A foreach patch with a skipped dependency is now off (was a
  latent bug).
- **Sessions render the composed base**: `Session.instances` pins the
  single includes' answers; `SessionMeta.base` holds composed node ids
  (`--base web/next-config` works; `latest` = every pinnable node).
- **`guard_mounts` inverted** (`commit::include_deps`): edits under a single
  include's mount are legal and become dependencies on the child nodes that
  own those files (leaves among owners; unowned new files → the frame's
  roots; root-mounted includes claim nothing they didn't create), proven by
  the existing closure replay. Edits under a repeat include's instances
  outside a `--foreach` session are still refused (`guard_instances`).
- **`check`**: static rule "op path under a mount ⇒ dependency on that
  include" (closes the smuggled `create_file` gap), full composed render +
  commutation over the combined graph (via `preview_parts`), `extends`
  recursion.
- **`.weft/base.json` pins instance bodies** (`BaseSnapshot.instances`,
  recursive), so `update` reconstructs an amended child's old render instead
  of silently dropping it — the prerequisite the design named.
- **Hooks follow the node's frame**: one `hooks::plan` over the composed
  graph (namespaced ids `web/pnpm-install`, `after` crosses the boundary,
  post-hooks run in the mount, per-frame changed answers on update),
  replacing the fixed children-then-parent order.
- `graph`/`describe` list composed nodes (`include`, `mount`, `opaque`,
  `inherited`, `extends`); LSP hover/goto resolve child and inherited
  names; schemas regenerated. New fixtures `base`, `nextjs-app`,
  `monorepo`, `nextjs-repo`; e2e `nodes.rs`.
- Known limits: nested (grandchild) instance answers still re-derive from
  binds; `patch amend`/`squash`/`resync` only target own root patches;
  parent hunks on child files can only reference parent answers.

## Post-MVP — Git-like recording session (staging index, many patches, sibling/stack)

- **The recording session is now a git-like workspace**: a real staging index,
  many patches per session, an explicit end, and a per-commit sibling-vs-stack
  choice. Replaced the earlier interim `commit --only` + `session
  status/discard` design (which stacked-only, no index). `session refresh`
  kept.
- **Staging index** = `weft-engine/src/stage.rs`: a lazy directory mirror at
  `.weft-record/stage/` (ABSENT means nothing staged = staged tree equals base,
  so zero disk cost until you `weft add`, and it is cleared after each commit).
  Helpers: `staged_tree` (stage dir or base), `add`/`reset` (Tree overlays over
  base), `changed_paths`, `revert_worktree_paths` (the sibling peel), `globset`.
  No new serialization — reuses fsio write_tree/read_tree_ignoring.
- **`weft add PATTERN…` / `-A`, `weft reset [PATTERN…]`, `weft status`**
  (top-level CLI verbs, git-mirrored). Status shows base + answers (secrets
  `(secret)`) + staged / unstaged. All reconstruct base/work via
  `commit::session_trees`.
- **`weft commit` reworked**: commits the STAGED tree if non-empty, else the
  whole worktree (BACK-COMPAT: empty index ⇒ commit-all, so all ~20 existing
  record/commit tutorials + e2e pass untouched). After write: if nothing
  remains uncommitted ⇒ end session (Session::discard, the classic auto-end);
  else keep open and apply the link — **Stack** advances base + re-pins
  tree_hash=staged.hash() (next patch depends on this); **Sibling** leaves base,
  peels committed paths back to base in the worktree (revert_worktree_paths) so
  the next patch is an independent commuting sibling. Link chosen by
  `interaction.confirm` (interactive prompt) or `--stack`/`--sibling`;
  NON-INTERACTIVE DEFAULT = Sibling (NonInteractive::confirm returns the `false`
  default). Continuing sessions require gate_open. Staging rejected for
  foreach/generator/amend sessions.
- **`weft session end [--discard]`**: refuses with a dirty worktree unless
  --discard.
- DESIGN (from the user): sibling-default (weft is a DAG, not git's linear
  history — sibling preserves independence/commutation, which git can't offer);
  file-level staging first (`add -p` hunks + shared-context guard = v2);
  `--amend`/`--fixup` = v2 (would fold staged into a patch via the amend/squash
  rebase engine). e2e tests/e2e/tests/session.rs (6, all pass): sibling-default,
  stack-depends, plain-commit-commits-all-and-ends (compat), session-end-guard,
  refresh-new-question, refresh-merge. Gate GREEN (fmt+clippy+cargo test
  --workspace, zero failures). Docs: reference/cli.mdx (weft add/reset/status,
  reworked commit, session refresh/end), guides/authoring.mdx ("Many patches
  from one session"). GOTCHA: staging a file then editing it more before a
  SIBLING commit loses the later edits (sibling peels committed paths to base) —
  stage the final version, or use stack.
- **`weft diff --staged`** (alias `--cached`): `commit::preview` generalized to
  `preview_target(template, staged, interaction)` — when staged, diffs the base
  against `stage::staged_tree` (the patch the next commit writes) instead of the
  whole worktree; `preview` kept as the `staged=false` wrapper (its only caller
  is the CLI diff handler). Empty index prints a friendly note. e2e:
  diff_staged_shows_only_the_staged_changes. Docs: reference/cli.mdx weft diff +
  authoring.mdx.
- **`weft add -p` / `--patch`** (the v2 item above, done): walk the unstaged
  changes hunk by hunk and stage a subset. KEY INSIGHT — **no stage-format
  change**: the stage holds whole-file blobs, so a partially staged file is just
  "the staged version with the selected hunks applied"; `staged_tree`,
  `stage::write` and `commit` are untouched. The picker diffs **staged →
  worktree** (like git: `-p` shows what is not yet staged), not base → worktree.
  Engine (`diff.rs`): `SelectableHunk`/`HunkLine` + `selectable_hunks` (same
  `TextDiff` + `CONTEXT_RADIUS` grouping as `hunks_between`, but concrete text
  plus the exact old/new line ranges), `apply_selection` (rebuild the file from
  the mask; all-true ⇒ new, all-false ⇒ old, trailing-newline habit preserved)
  and `split` (break a grouped hunk at its interior context into one hunk per
  change run — sub-hunks may SHOW shared context but their `old_range`s are
  disjoint, so apply stays well-defined). CLI: `weft-cli/src/addpatch.rs`, keys
  `y n a d s q ?` (no `e`/manual edit, no `j`/`k` navigation — deliberate).
  Deletions, binary content and mode changes are not hunkable: one `[y/n]`
  prompt, staged whole. Decisions come through the `HunkDecider` trait —
  `TerminalDecider` reads single raw keypresses on a TTY and **one key per line
  when stdin is a pipe**, which keeps `-p` scriptable and lets the e2e tests
  drive it (`write_stdin("y\nn\n")`). `-p` with no patterns means everything.
  `weft reset` stays file-level (unstage the whole file to redo). Tests: 8 unit
  in diff.rs, 11 in addpatch.rs, 5 e2e in tests/e2e/tests/add_patch.rs.

## Post-MVP — Sessions are git-style worktrees (BREAKING: `weft record` removed)

- **The reported pain**: you stood in the template and typed `weft add init`,
  but the files were in `.weft-record/worktree/`, so nothing tab-completed and
  the argument read like a name, not a path. FIX = git's model: the worktree is
  a directory you `cd` into, weft finds itself by walking up, and path arguments
  are relative to where you stand.
- **`weft record` is GONE** (hard rename, no alias, decided with the user).
  `weft session new NAME` replaces it; the name is a required positional.
  Engine module `record.rs` → `start.rs` (`StartOptions`).
- **Layout**: `.weft-record/` → `.weft-sessions/<name>/{session.toml, stage/,
  worktree/}`. A template holds any number of sessions, **each with its own
  index** (`stage::stage_dir(root, session)`). Legacy layouts migrate to
  `.weft-sessions/main/` on first touch (`session::migrate_legacy`, idempotent).
  NOTE for template repos: `.gitignore` needs `.weft-sessions/`.
- **Discovery** = new `weft-engine/src/discover.rs`. A worktree carries a
  back-pointer at `<worktree>/.weft/worktree.toml` (git's `.git`-file trick);
  `.weft/` is already unconditionally skipped by the tree walker, so it can
  never leak into a patch, and it sits beside the `state.toml`/`base.json` an
  adopted project already has. `locate()` checks **pointer then manifest at each
  level**, innermost wins — so a worktree whose render contains its own
  `weft.toml` is still a worktree, while a genuinely nested template resolves to
  itself. CLI side: `weft-cli/src/ctx.rs` (`Scope` clap args + `qualify()` for
  CWD-relative pathspecs). Resolution: CWD's session → `--session` → the only
  session → error listing them. **No stored "current session" pointer** — that
  would be a footgun for a command that writes patches.
- **Worktrees anywhere**: `--path` puts one outside the template (recorded
  absolute on `SessionMeta::worktree`; the default location is left unrecorded
  so the session file stays portable). `weft session move`, plus **self-heal** —
  a bare `mv` is repaired by the next command run inside it, since standing
  there is proof of where it is (git needs an explicit `worktree repair`).
- **`weft session adopt PATH -n NAME`** — the inverse of `weft update`. KEY FIT:
  a `weft new` project already stores everything a session needs in
  `.weft/state.toml` (template ref, pinned base ids, answers, secret refs), so
  adopting one takes no arguments and the diff is exactly the author's edits.
  Base defaults to the project's **own pinned base**, not `latest` — otherwise
  every patch added since would read as a deletion. Plain directories need
  `--template` + answers and want `--scope` globs (stored on the session,
  applied to the **base tree as well**, or out-of-scope base files read as
  deletions). `weft session scope --add/--rm` widens it later.
- **GOTCHA FOUND IN TESTING**: the sibling transition reverts committed paths in
  the worktree — on an adopted project that **deleted the author's real source
  file**. Adopted sessions now always stack, and `--sibling` on one is refused
  *up front* (the first version bailed after `write_patch_full`, leaving the
  patch behind). `session end` unlinks an adopted worktree instead of deleting.
- **`weft commit --depends-on a,b` / `--after X`** overrides the derived
  `base_leaves`. Guard rails: names must be in the session's *active* base, and
  the patch must additionally render against **its declared closure alone** — a
  claim of independence the content can't honour fails at commit instead of at
  `weft check`. (The byte-for-byte replay still runs against the full base; the
  closure render is a second, apply-only check.)
- MCP `record_*` → `session_*` with a `session` param (default `agent`).
  Weft Cloud's `web/lib/improve/agent.ts` has its own tool ids over its own HTTP
  routes — left alone deliberately.
- Tests: `discover.rs` 5 unit, `session.rs` 3 unit, new
  `tests/e2e/tests/worktree.rs` 15 e2e (discovery from a subdir, two sessions
  with separate stages, ambiguity error, list marker, external path + move,
  mv-repair, adopt scaffolded/plain/scope, adopted-file safety, end semantics,
  depends-on/after + both rejections). Existing e2e migrated onto the new verbs.
  Gate GREEN (fmt + clippy + `cargo test --workspace`, 35 binaries).

## Post-MVP — Self-contained update base (survives history rewrites)

- **The prerequisite for using amend/squash/resync on live templates.**
  `weft update` rebuilt its 3-way-merge base by looking up the project's
  pinned patch ids in the CURRENT template → bailed on any id rewrite.
- **Design correction:** an old→new id map (the earlier roadmap idea) is
  WRONG for content changes — remapping a pinned id to the amended patch's
  new id makes base==theirs, so the fix silently wouldn't propagate.
  Instead the project stores its base patch BODIES in `.weft/base.json`
  (state.rs StoredPatch/BaseSnapshot; JSON not TOML — ops nest badly in
  TOML). update reconstructs old_render from those, independent of template
  ids. Bodies not the rendered tree → secrets stay as {answer} refs, never
  a resolved value in .weft/.
- Correct for every rewrite: amend/resync PROPAGATE (stored base = old
  content → true 3-way merge around user edits); squash is a NO-OP (old and
  new render identically). Back-compat: no base.json → id-match + bail
  fallback, self-heals on next successful update. Parent template only;
  foreach instances keep id-match (follow-up).
- new.rs writes the snapshot at scaffold; update re-pins after (dry-run
  untouched). e2e tests/e2e/tests/update_rewrite.rs (4): amend propagates
  across id rewrite, merges around a user edit (proves base=old content),
  squash no-op, back-compat with/without snapshot. Docs: state.mdx
  (.weft/base.json), updating.mdx (surviving rewrites), roadmap correction,
  + shadcn-ui tutorial demonstrating variant composition.

## Post-MVP — Patch editing (set-command, detach, amend, squash)

- **The insight:** in a content-addressed Merkle DAG, editing/amending/
  squashing are all one operation — a subgraph rebase — since changing any
  patch's content/deps rewrites descendant ids. The shared "engine" is
  resync's existing detection (post-render names the broken dependent).
- **`weft patch set-command NAME [CMD] [--resync]`** (author.rs): replace a
  generator patch's stored command (parse_command interp + $EDITOR); refuses
  non-generators; metadata-only (id unchanged).
- **`weft patch detach NAME`**: drop generator metadata → plain editable
  patch (resync no longer applies). The escape hatch for owning generated
  content by hand.
- **`weft patch amend NAME`** (new amend.rs): reopens the patch's
  contribution in a worktree (base = ancestors via ancestor_closure minus
  self; seed = base + apply_ops(target) to force it on regardless of gate);
  `weft commit` branches on `Session.amend` (new field) → amend::finish
  re-derives ops, preserves name/deps/gate/foreach/meta, save_patch_file in
  place. Refuses generators (redirect resync/set-command/detach) and foreach.
  Non-leaf: allowed — after write, a full render under the amend answers
  surfaces a broken dependent (bail with re-record guidance; the amend is
  kept — the rebase-conflict path, matching resync). Most edits don't move a
  dependent's anchor so they just work.
- **`weft patch squash NAMES… --into NAME`** (new squash.rs): ops
  concatenated in topo order (already abstracted → NO answers/re-abstraction
  needed — the elegant approach), external deps unioned, external dependents
  repointed, hooks unioned. Guards: convex set (no outsider between members),
  same gate, no generator/foreach. Snapshot→write→reload-validate→rollback
  transaction.
- e2e tests/e2e/tests/patch_edit.rs (9). Deferred (roadmap): pinned-project
  id-migration (weft update can't follow rewritten ids yet — the true long
  pole for using these on published templates), weft patch split, guided
  interactive rebase loop.

## Post-MVP — Binary file support

- **Problem:** `record --exec 'pnpm dlx shadcn init'` emits `favicon.ico`
  — legitimate content, not junk, so `.weftignore` is wrong. Text-only
  MVP (deferred in PLAN.md) came due.
- **Model:** `weft_core::FileData { Text(String), Binary(Vec<u8>) }`,
  variant chosen by `from_bytes` (valid UTF-8 ⇒ Text) so rendered and
  re-read trees always agree. `FileEntry.content` is now `FileData`.
  New **`Op::CreateBinaryFile { path, data(base64), mode }`** — internally
  tagged, so existing ops' canonical JSON is byte-identical (canonical.rs
  snapshots + proptests pass UNCHANGED — the additive-serde guardrail).
  Tree::hash frames content by bytes exactly as before, so text tree
  hashes (project pins) don't move.
- **Modify a binary = `delete_file` + `create_binary_file`** in one patch
  (ops apply in order); text hunks on a binary are a loud RenderError.
  diff.rs emits this pair (any-binary-side branch) and excludes binary
  from abstraction/occurrences; `create_op` helper picks CreateFile vs
  CreateBinaryFile by `content.text()`.
- **update:** binary both-diverged can't line-merge → keep ours + note
  ("template changed a binary file you also modified; kept your version").
- Previews/diffs (`PreviewFile`, graph `FileDiff`, server `ProposalFile`/
  `FileContent`/`RenderedFile`) gain `binary: bool` (empty before/after,
  "Binary files differ"); `weft diff` prints "(binary file)"; server
  write endpoint refuses to clobber a binary; web FilePreview/DiffView/
  RecordEditor show opaque fallbacks.
- e2e tests/e2e/tests/binary.rs (6): byte-exact round trip + check,
  resync, delete+create modify, update take-theirs / keep-ours+note,
  diff opaque. NOTE learned: resync rewrites patch ids, so resync +
  update on a pre-scaffolded project doesn't compose (pinned id gone) —
  update tests ADD a patch instead. base64 = new weft-core/engine dep.

## Post-MVP — `.weftignore`

- **Problem:** a generator like `--exec 'pnpm dlx shadcn init'` drops
  `node_modules/` in the worktree; read-back would not just bloat the
  patch — it hard-errors on the first non-UTF-8 file. Same latent bug in
  `weft update`, which read the ENTIRE project dir (a real Node project's
  node_modules would break update).
- **`.weftignore` at the template root**, real gitignore semantics via the
  `ignore` crate (negation included). Applied only when reading a worktree
  BACK (commit/diff/resync + the record-exec hash + the server session
  commit) — never to rendered output. Ignored dirs are pruned unread.
  Built-ins: `.git/`/`.weft/` (walker, as before) + `.DS_Store` (new).
  **Base-rendered paths are exempt** (`read_tree_ignoring(root, rules,
  keep)`) so deleting a rendered file still records even if a pattern
  matches it.
- **`weft update` now reads only template-tracked paths** (old ∪ new
  render) from the project via the new `fsio::read_file` — user junk is
  never opened. Behavior-preserving: `plan_file(None, Some, None)` was
  already a no-op.
- Travels with the template: `weft hub publish` packs it; `weft init`
  seeds one (node_modules/ + comment). `Template.ignore` carries the
  rules. e2e: tests/e2e/tests/weftignore.rs (binary junk through
  exec+commit+resync, .DS_Store default, base-path exemption, update
  with binary node_modules).

## Post-MVP — Command interpolation + $EDITOR fallback

- **`${…}` in authored commands, only when relevant.** `record --exec` and
  `hook add --action` parse `${…}` via `hooks::parse_command`
  (balanced-brace scan): a declared question id → answer segment; a
  Starlark expression that trial-evaluates over `dummy_answers` → expr
  segment; everything else (`${HOME}`, `${1:-x}`, typos, list-valued
  bodies like a bare multichoice id) stays literal so shell parameter
  expansion keeps working. The recognized form is echoed
  ("generator interpolates: …") — typos are visible by absence, by
  design (no way to distinguish a typo from an intended shell var).
- **Interpolated generator patches are answer-parametric**: resync with a
  different `--answer` reproduces the same abstracted patch → "up to
  date". Value variation flows through commands + abstraction; structural
  variation still needs one gated patch per variant (see the new
  best-practices guide).
- **`weft check`** now validates generator metadata fully (command exprs
  parse, answer refs declared, keep-literal well-formed, stored answers
  name real questions) and flags unknown answer refs in hook commands
  (foreach patches keep `key`/`instance_*`).
- **$EDITOR fallback**: bare `--exec` (and `hook add` with all flags but
  `--action`) opens `$VISUAL`/`$EDITOR` via `dialoguer::Editor` on a
  seeded `.sh` buffer; `#` lines stripped; not TTY-gated (mtime-based
  save detection), so e2e drives it with `VISUAL="sh fake-editor.sh"`.
- Docs: new `guides/best-practices.mdx` — patch identity (id = blake3 of
  behavior-as-answer-function; metadata free), the two conditionality
  levels (gate = structural/compositional, expressions = value-level),
  and the generated-patch rule (interpolate for values, split gated
  patches for structure).

## Post-MVP — Generator patches (`record --exec` + `patch resync`)

- **A patch can be a command's output.** `weft record --exec CMD` renders
  the base, runs CMD in the worktree (`sh -c`, shared `hooks::run_command`
  path), and commit stores it as **unhashed** `generator` metadata on the
  patch: the command, the record-time answers (secrets kept as source
  references, session-style), and the keep-literal specs. Generator
  sessions force the scripted confirm-all abstraction path — interactive
  per-occurrence choices couldn't be replayed at resync. Hand edits on top
  of the command output are allowed but warned (lost on resync; detected
  via a post-exec worktree hash held in the session).
- **`weft patch resync [NAME…|--all]`** re-runs stored commands and
  rewrites ops from fresh outputs, in dependency order, **reloading the
  template between rewrites** — deps are stored by name and ids recompute
  on load, so rewriting an ancestor needs zero dependent-file edits and
  chained generated patches see fresh ancestor output. Up-to-date = replay
  hash of the existing patch equals the fresh worktree (files stay
  byte-identical); the commit replay guard rejects ambiguous contexts; a
  post-pass full render names hand-recorded dependents whose anchors moved
  (exit nonzero, re-record instruction — no auto-fix, no rollback: git is
  the safety net, `--dry-run` the preview).
- New engine module `generate.rs`; `Generator` in weft-core
  (`PatchMeta.generator`, `#[serde(default)]` so old files parse and no
  ids move); guard_mounts/secret-resolution refactored for sharing.
- Docs: new `reference/commutation.mdx` (the full commutation contract:
  why gated subsets force it, the pairwise check's exact shape, its
  smoke-test limits, Pijul relation) — confirming the Pijul-style
  commutation requirement is still in force and unchanged by this
  feature (resync re-enters the same replay + check gates).

## Post-MVP — Presets v2 (locks + multichoice constraints)

- **Semantics change:** presets are no longer an overridable base layer —
  they **lock what they answer**. A plain entry (scalar/array) locks the
  question: skipped in the wizard/prompts, and an explicit answer with a
  *different* value errors (identical is accepted). A TOML *table* entry
  constrains a multichoice: `fixed` always selected, `blocked` never
  selectable, final value `(user ∪ fixed) − blocked`. Selected presets
  merge (locks must agree; constraints union; fixed∩blocked errors).
- **Engine:** `preset.rs` (`PresetSpec`/`PresetEntry`, parse/merge/
  validate/apply/to_toml/save/remove — manifest edits via `toml_edit` with
  rollback). All user layers (file, `--answer`, `--answers-json`) flow
  through `spec.apply`; `answers::Layered` exposes the lock set +
  constraints to UIs. `Template::preset()` remains as a locks-only
  projection. `describe` reports `locks` + `constraints` per preset and
  omits locked ids from the per-preset usage commands.
- **CLI:** wizard skips locked rows (values still feed gates) with a
  header note; constrained popups hide blocked choices and pin fixed
  ones. `weft presets save` opens a tri-state authoring wizard (space
  cycles free→fixed→blocked, nothing required, `x` clears) or runs
  scripted via `--answer/--fix/--block`; `presets rm` removes both file
  and declaration; after an interactive `weft new` on a local template,
  a one-shot confirm offers to capture the answers as a preset.
- **Server/Studio:** previews resolve through the merged spec (lock
  conflicts are 400s); graph responses carry `locked` + `constraints`;
  the Compose form shows a lock badge and constraint-aware multichoice
  checkboxes; selecting a preset prunes now-locked client answers; the
  preset dialog authors constraints per choice (free/fixed/blocked) and
  the save API accepts `constraints` alongside plain `answers`.

## Post-MVP — Hub-composed templates (version reqs + lockfile)

- **IncludeResolver trait** threads through `Template::load_inner`; the
  engine stays network-free (`PathResolver` default errors on hub refs).
  `Template::load` unchanged; `load_with` takes a resolver, and the
  consumer entry points (new/update/check/record/commit/instance) gained a
  resolver arg.
- **`[[include]] template = "hub:owner/name" version = "^1"`** + `lock.rs`
  (`weft.lock`, Cargo-shaped: reqs in weft.toml, exact versions+sha256 in
  the lock, committed & published). Template-side because a scaffolded
  project already pins child *patch ids* — the lock makes composed
  *loading* reproducible.
- **CLI `HubResolver`**: per-root lock (lazy load, flush on change),
  resolve req → highest matching → download+verify+cache → pin. `weft lock
  [--upgrade]`, `--frozen` on new/update/check, publish packs the lock.
- **Hub server `RegistryResolver`**: resolves a composed publish's hub
  includes against its OWN stored versions (hermetic, same-registry),
  verifies the lock's sha256, stores + renders the dependency set. Rejects
  unpinned/stale/missing.
- e2e: CLI composes a hub include (lock written, --frozen gate); server
  hosts a composed template (deps page) + rejects stale locks. Real smoke:
  publish child → author+lock+publish composed parent → `weft new
  hub:parent` composes the tree.

## Post-MVP — Weft Hub (registry MVP)

- **Standalone repo `weft-hub`**: single-binary axum registry (filesystem
  storage, no DB). Publish-side validation runs the weft crates in-process
  (safe unpack, `Template::load`, hook validation, `describe`); the
  DescribeDoc becomes the version's stored metadata and powers
  server-rendered pages where **hooks with effect badges are the trust
  surface** (deploy hooks warned above the install snippet). Versions are
  semver, strictly increasing, immutable; yank hides without deleting.
- **CLI**: `hub:owner/name[@version]` refs resolve via `WEFT_HUB_URL`
  through a sha256-verified cache (`~/.weft/hub/…`, atomic staging;
  pinned cached versions are fully offline); `weft new hub:` stores the
  *resolved* ref in state (`NewOptions.stored_ref`); update resolves the
  pin from cache and hints on newer versions. `weft hub
  publish/search/info`.
- e2e drives a hand-rolled stub registry (resolve→pin→cache, offline
  hit, sha-tamper rejection); the real API is tested in weft-hub (7
  tests incl. hand-crafted-header path traversal). Real smoke: published
  the dbt templates, scaffolded from `hub:`, page rendered with badges.

## Post-MVP — weft diff + per-occurrence abstraction

- **`weft diff`**: pre-commit view of the session — concrete text with
  candidate spans highlighted (ANSI / `⟨…⟩` piped), legend footer,
  `--abstracted` for the stored `{answer}` form, `--json` for keys.
- **Per-occurrence decisions**: occurrences in authored content keyed
  `(path, line, nth)` in deterministic scan order (enumeration and
  substitution share one walk, so keys can't drift; nth is consumed by
  every match, so exceptions don't renumber). `weft commit
  --keep-literal ANSWER@PATH:LINE[:NTH]`, an interactive yes/no/select
  prompt, Studio commit-dialog checkboxes, and the server's
  `keep_literal` commit input all land the identical patch. Context
  lines keep the per-answer rule (they mirror the base render); secret
  occurrences can never be kept literal.
- `commit::{session_trees, preview}` extracted — the one source of the
  base/worktree/candidates/occurrences view for CLI and server.

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
