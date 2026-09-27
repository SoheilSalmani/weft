# How to write tutorials that exercise weft's template features

This guide is for the agent writing weft's tutorials. Each tutorial is also
an acceptance test: a reader who follows it end to end must see every claimed
result, and a script that replays it must pass. Follow the
`writing-documentation` skill for the prose (Diátaxis tutorial mode: do, don't
explain); this page fixes *what* a tutorial has to prove and *how* it stays
true.

## The rule that makes tutorials trustworthy

**Run every command. Paste real output. Assert it in a script.**

- Every tutorial has a companion script under `docs/tutorials/scripts/`
  (`set -euo pipefail`, a fresh `mktemp -d`, `weft` from `cargo build -p
  weft-cli`). The script runs the exact commands in the exact order and
  checks every observable claim with `grep -q` / `test -f` / `diff`. A
  tutorial whose script fails is wrong, not the script.
- Expected output in the prose comes from a run of that script, trimmed, never
  typed from memory. Patch ids are 64 hex chars — show them as `…` or the
  12-char short form `weft graph` prints, and never assert a specific hash
  (assert equality *between* two hashes instead).
- Make every run deterministic: `--non-interactive`, `--answer k=v` for every
  question, `--yes` on commit, `--skip-tasks` unless hooks are the topic.
  Use `EDITOR=true` so nothing opens an editor.
- One tutorial, one feature ladder. Do not teach includes inside the update
  tutorial; link to the includes tutorial instead.
- If a tutorial puts the project under git (it should, from rung 2 on:
  `git init && git add -A && git commit -m scaffold` right after `weft
  new`), commit before every `weft update` — an update refuses to write over
  uncommitted changes. Scratch projects outside git are never blocked.

## Layout the reader builds

Every tutorial starts from an empty scratch directory and keeps templates
as *siblings*, because `extends = "../base"` and `[[include]] template =
"../app"` are paths relative to the template root:

```
work/
├── base/          # a template (extended by the others)
├── app/           # a template meant to be mounted
├── monorepo/      # extends ../base, includes ../app
└── proj/          # a scaffolded project (weft new … proj)
```

Name templates after their role (`base`, `app`, `monorepo`), projects `proj`,
`proj-2`. Keep templates tiny: a manifest, one or two questions, two to four
patches, files of a few lines. The repository fixtures under
`fixtures/templates/` (`hello`, `workspace`, `base`, `nextjs-app`, `monorepo`,
`nextjs-repo`) are the reference shapes — copy their structure, not their
names, so the tutorial's template is the reader's own.

## Command surface a tutorial may use

Verified against the current CLI; if a tutorial needs a flag not listed
here, check `weft <cmd> --help` and add it to this table.

| Step | Command | What to show the reader |
| --- | --- | --- |
| Blank template | `weft init DIR --name NAME` | the generated `weft.toml`; append `[[question]]` blocks by hand |
| Record a change | `weft session new NAME --template T --answer k=v` | prints the worktree path; `cd` into it and edit with real tools |
| Inspect | `weft status`, `weft diff [--staged]` | staged/unstaged, the abstraction candidates |
| Stage | `weft add PATH…`, `weft reset` | only staged content is committed |
| Commit | `weft commit --name N --yes [--title …] [--stack\|--sibling] [--depends-on A,B] [--after P]` | stderr: ``committed patch `N` …``; the written `patches/N.json` |
| Validate | `weft check T --answer k=v` | `render ok (N files)`, `commutation ok for N independent pair(s)`, `passed all checks` |
| Look at the graph | `weft graph T [--answer …] [--json] [--diff PATCH]` | nodes, edges, `[active]`/`[inactive]`, a node's contribution |
| Describe | `weft describe T --json`, `--agents-md -` | the contract agents read |
| Scaffold | `weft new T DEST --answer k=v [--preset P] [--instance inc=key] --skip-tasks --non-interactive` | the tree, `.weft/state.toml`, `.weft/base.json` |
| Sync | `weft update DEST [--template T] [--dry-run [--diff]] --non-interactive --skip-tasks` | the `answers:` block (when answers move), stderr `N file(s) written`; conflicts exit nonzero |
| Change answers | `weft update DEST --answer ID=V [--unset ID] …` (`--reconfigure` opens the wizard; TTY only) | `answers:` lines, derived ones tagged `(derived)` / `(bind)` |
| Inspect answers | `weft answers DEST [--json]` | each answer's value and origin: `given`, `derived`, `secret` |
| Edit a patch | `weft patch amend NAME --template T --answer …`, then `weft commit --yes` | ``amended patch `NAME` …; its content id changed`` |
| Combine | `weft patch squash A B --into C` | ids downstream move; projects still update |
| Fleet | `weft instance add INC KEY --dest DEST`, `instance remove`, `instance list` | files appear/disappear, integration lines follow |
| Sessions | `weft session refresh`, `weft session end [--discard]`, `weft session adopt DIR` | |

Manifests: `[template] extends = "../base"`; `[[include]] name = "web"
template = "../app" path = "apps/web"` (+ `repeat = true` with `{key}` in
`path`, `[include.bind]`). Patch files: `depends_on` may name `web/<patch>`
(single includes) and inherited names; `"foreach": "<repeat include>"` for
integration patches.

## The ladder of tutorials

Write them in this order; each assumes the previous one's vocabulary and links
to it. Every tutorial ends with the reader running `weft check` and reading
`weft graph`.

### 1. Getting started: one template, one project, one round trip

Prove: patches store inputs; the same inputs render byte-identically.

1. `weft init`, add a `project_name` question. Start a session, create
   `README.md` containing the concrete answer, commit as `readme`.
2. Show `patches/readme.json`: the literal became `{"answer":
   "project_name"}` (assert the concrete value is *absent* from the file).
3. `weft new` twice into `proj` and `proj-2` with different answers; assert
   each README carries its own value.
4. Determinism: `weft new` the same answers into two directories and
   `diff -r` them (ignore nothing — they must be identical).

Trap: an answer that also appears as a common word abstracts everywhere it
matches; pick distinctive answer values (`Trendlift`, not `app`).

### 2. Evolving a template and syncing a project

Prove: `weft update` is a 3-way merge that keeps local edits, applies template
changes, and is idempotent; conflicts are marked, never clobbered.

1. Scaffold `proj` from tutorial 1's template. Edit `proj/README.md`
   (append a line the template never wrote).
2. In the template: a new session, add `Makefile`, commit `make`. `weft
   check`.
3. `weft update proj --dry-run` — show the plan line `dry run: update
   Makefile` (every write, new or changed, is `update`; removals are
   `delete`). Then `weft update proj`: assert `Makefile` exists and the local
   README line survived.
4. Run `weft update proj` again: assert stderr contains `0 file(s) written`.
5. **With a conflict**: in the template, amend `readme` (or a new patch
   depending on it) that changes the README line the project also changed
   locally. `weft update proj` must exit nonzero, stderr mention
   `conflict`, and the file contain `<<<<<<< local` … `>>>>>>> template`.
   Resolve by hand, run update again: `0 file(s) written`.
6. **Without a conflict on the same file**: change a *different* line in the
   template; update merges cleanly. Show both so the reader learns what a
   conflict actually is.
7. `weft patch amend` the template, update again: the project follows the
   rewrite (`.weft/base.json` is why — one sentence, then link to the
   explanation page, do not explain in the tutorial).
8. **Change an answer.** `weft answers proj` first (given vs derived), then
   `weft update proj --answer "project_name=New Name"`: assert the
   `answers:` block lists the given change *and* the derived one
   (`package_name … (derived)`), the files follow, and a local edit on
   another line survives. Then scaffold `proj-2` with an explicit
   `package_name`, rename again, and assert it stays; `--unset
   package_name` makes it follow. Finish with a conflicting answer change
   (local edit on the title line), show that the next plain `weft update`
   refuses until the markers are resolved, resolve, and re-run.

Traps: `weft update` reads `.weft/state.toml` for the template path; when the
template moved, pass `--template`. Every `--answer` the template needs must be
in state or on the command line — a new required question prompts, so add it
with a `default`. An answer change and template changes land in the same run;
to show only the answer's effect, run `weft update` (and commit) first.

### 3. A base template and a template that extends it

Prove: `extends` imports the base as-is — same question names, same patch
names, same ids — and a change in the base reaches every extender and every
project without editing the extender.

1. Build `base` (a `use_linear` bool question; patches `skills` creating
   `AGENTS.md`, `linear` gated by `use_linear` modifying it). `weft check`.
2. Build `monorepo` with `[template] extends = "../base"` and one own patch
   that depends on `skills` (e.g. `workspace` adding a line to `AGENTS.md`).
   Assert `weft patch ls --template monorepo` flags `skills`/`linear` as
   `[inherited]`, and that `weft graph --json` gives `skills` the same `id`
   in both templates (the identical-ids claim — assert it, it is the whole
   point).
3. Scaffold `proj` from `monorepo` with `--answer use_linear=true`: the
   base's gated patch renders; scaffold `proj-plain` without it: it does not.
4. **Modify the base**: a session *in `base`*, change `AGENTS.md`, commit.
   Do nothing in `monorepo`. `weft check monorepo` passes; `weft graph
   monorepo` shows the new base patch. `weft update proj`: the change lands.
5. Show the refusal that keeps the model honest: `weft patch amend skills
   --template monorepo` fails with ``inherited from `…/base` — amend it
   there``; a `patches/skills.json` in `monorepo` fails to load with
   `already defined by the extended template`.
6. Sync with and without conflict, as in tutorial 2, but with the edit made
   in the *base*: once against an untouched project file (clean), once
   against a line the project changed (markers).

Trap: `extends` is live for path refs — there is no "sync the child
template" step. Say so explicitly, and point to `weft lock --upgrade` as the
sync step for `hub:`/git refs.

### 4. Mounting a template inside another: include nodes

Prove: a single include's patches are nodes `web/<patch>` the parent may
depend on; a parent patch may edit the child's files once it depends on the
owning node; gates propagate across the boundary.

1. Build `app` (question `app_name`; patch `app` creating `package.json`,
   patch `tailwind` gated by `use_tailwind` modifying it). Build `monorepo`
   including it at `apps/web` with a `bind`.
2. `weft graph monorepo`: nodes `web/app`, `web/tailwind` with
   `include: "web"`, `mount: "apps/web"`.
3. **The session base is composed**: `weft session new` in `monorepo`;
   assert `apps/web/package.json` exists in the worktree.
4. Edit `apps/web/package.json` and a root file; `weft commit --name turbo
   --yes`. Assert stderr `edits under an include's mount: depending on
   web/app` and `patches/turbo.json` has `"web/app"` in `depends_on` (and
   not `web/tailwind`). `weft check` passes. Scaffold and show the edit in
   place.
5. **Gate inheritance**: hand-write a patch depending on `web/tailwind` whose
   hunk anchors on the Tailwind line; scaffold with `--answer
   web.use_tailwind=false`: no error, the parent hunk is simply absent.
6. **The static rule**: add a patch creating `apps/web/.env` with no
   dependency; `weft check` fails with ``is under include `web`'s mount but
   the patch does not depend on any of its nodes``; add `"depends_on":
   ["web/app"]`, it passes.
7. Root mounts: an include with `path = ""` merges into the root; show one
   file from each template side by side, and the render error when both
   create the same path (this is why a mountable template owns no README).

Traps: namespaced answers are `web.use_tailwind=false` (dot), node names are
`web/tailwind` (slash). Edits under a mount can only reference the *parent's*
answers.

### 5. Parent, child, project: the full sync chain

Prove the sequence the composition feature exists for: parent template →
child template based on it → project → change on either side → everything
resyncs.

1. `base` (extended) and `app` (mounted) as before; `monorepo` extends
   `base` and includes `app`; `proj` scaffolded from `monorepo`.
2. Change `base`: update `proj` (clean). Change `app` with `weft patch
   amend` (every id downstream moves): `weft check monorepo` still passes
   because dependencies are by name; `weft update proj` adopts the amend and
   a second update writes `0 file(s)`. This is the case that used to drop
   the child silently — assert both the content and the no-op.
3. Change `monorepo`'s own glue patch that anchors on a child line, then
   change that line in `app`: `weft check monorepo` reports the broken
   anchor (`does not match`), the author re-records the glue, check passes,
   `weft update proj` follows. Show the failure *and* the fix.
4. A conflict variant: the project edited the child's file locally, the
   child template changed the same line — markers under `apps/web/…`.

### 6. Fleets: repeat includes and integration patches

Prove: instances are project-time; `foreach` patches render once per
instance and may reach inside an instance.

1. `[[include]] name = "connector" … path = "connectors/{key}" repeat =
   true`. Scaffold with `--instance connector=github --instance
   connector=stripe`; `weft instance add connector jira --dest proj`;
   `instance list`; `instance remove`.
2. Record an integration patch: `weft session new --foreach
   connector=stripe`, edit the root README *and* a file under
   `connectors/stripe/`; commit. Assert the patch has `"foreach":
   "connector"`, `{"answer": "key"}` in both a content line and a path, and
   the literal `stripe` nowhere. Scaffold: the line and the file exist for
   every instance.
3. Show the refusal in a plain session: an edit under `connectors/…`
   fails with ``inside the instances of repeat include `connector` ``.
4. Hooks, if in scope: a child post-hook runs inside its mount; a parent hook
   with `after: ["web/install"]` runs after it — assert an `order.txt`.

## Invariants every tutorial asserts somewhere

- `weft check` passes after every commit the tutorial makes.
- Two scaffolds with equal inputs are `diff -r`-identical.
- The second `weft update` in a row writes `0 file(s)`.
- A patch file never contains a concrete answer value that was abstracted.
- Editing `description`/`title`/`tags`/`hooks` of a patch does not change any
  id in `weft graph --json`; editing `ops` or `depends_on` does.

## Failures to demonstrate on purpose

Readers learn the model from what weft refuses. Each ladder rung above names
its refusal; keep the exact stderr fragment in the script:

| Situation | Fragment to assert |
| --- | --- |
| Conflict on update | `conflict`, `<<<<<<< local`, `>>>>>>> template` |
| Update with markers still in a file | `conflict markers that are still there` |
| Update over uncommitted git changes | `uncommitted changes in file(s) this update would write` (`--allow-dirty` overrides) |
| `--unset` on a question with no default | `has no default to fall back to` |
| Answer for a repeat instance the project lacks | ``add it with `weft instance add`` |
| `--depends-on` that cannot be honoured | `does not apply with only` |
| Inherited patch edited in the extender | ``inherited from`` |
| Mount edit without dependency (check) | `is under include` … `does not depend on any of its nodes` |
| Instance edit outside `--foreach` | `inside the instances of repeat include` |
| Depending on a repeat include by name | `reach its instances with` … `foreach` |
| Non-commuting siblings | `do not commute` |

## Before publishing

- The script passed from a clean clone (`cargo build -p weft-cli && bash
  docs/tutorials/scripts/<n>.sh`).
- Every output block in the prose is a copy of the script's run.
- The tutorial never explains *why* beyond one sentence; the why links to an
  explanation page (`docs/explanation/…`) or the README's composition section.
- A reader who stops after any step is left with a working, checkable state
  (`weft check` green), never a half-written patch file.
