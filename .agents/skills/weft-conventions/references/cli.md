# The weft command sheet

Copied from `weft <command> --help` on `weft 0.1.0`, 2026-09-25, with the slot, `weft commit` and `weft patch resync` changes of a weft built from source on 2026-09-29 (`weft patch resync --yes` does not exist on 0.1.0), the prompting and flag changes from removing the full-screen wizard and forms checked against the build on 2026-09-30, and `weft share`, `weft commit --share` and amending a slot owner from a build of `e11dcba` with that feature, the same day. The docs under `docs/content/docs/weft/reference/cli.mdx` lag the binary in places (they omit `--title`, spell the tag flag `--tags a,b`, and still mention `weft record`), so when this sheet and the binary disagree, run `--help` and believe the binary. `weft --version` first: a newer binary may have moved flags, a build from source still reports `0.1.0`, and `weft 0.1.0` itself still has the `--no-tui` and `--no-wizard` flags this sheet no longer lists.

## Contents

- Template
- Sessions
- Staging and committing
- Patches after the fact
- Hooks
- Presets
- Validation and description
- Consuming a template
- Composition and hub
- Errors you will meet

## Template

```text
weft init [DIR] [--name NAME]          # weft.toml + empty patches/ + a default .weftignore (node_modules/)
```

The generated `weft.toml` carries `description = "TODO: what this template scaffolds."`. Replace it; `weft describe` and the studio show it first.

## Sessions

```text
weft session new NAME [--template DIR] [--base latest|PATCH] [--preset P]... [--answer K=V]...
                      [--answers-file F] [--path DIR] [--foreach INCLUDE=KEY] [--exec [CMD]]
                      [--force] [--non-interactive] [--shell]
weft session adopt PATH -n NAME [--template DIR] [--base REF] [--preset P]... [--answer K=V]...
                      [--answers-file F] [--scope GLOB]... [--force] [--non-interactive]
weft session list | ls
weft session path [NAME]
weft session shell [NAME] [--template DIR] [-- CMD [ARGS]...]
weft session move NAME DEST
weft session scope [--add GLOB]... [--rm GLOB]
weft session refresh [--answer K=V]... [--preset P]... [--answers-file F] [--answers-json J]
weft session end [NAME] [--discard]
```

- `--base latest` (default) stacks on everything active. `--base PATCH` renders that patch and its ancestors only, so the new patch is independent of everything later.
- `--exec CMD` runs the command in the rendered worktree; its output is the patch and the command is stored so `weft patch resync` can re-run it. `${answer}` and `${expr}` interpolate declared answers. Pass `--exec` with no value to write the command in `$EDITOR`.
- `adopt` on a directory `weft new` made needs no arguments beyond `-n`; it reads `.weft/state.toml`. On any other directory pass `--template`, the answers, and `--scope`, or every file reads as new.
- `end` refuses a worktree with uncommitted changes unless `--discard`. A worktree weft created is deleted; an adopted one is only unlinked.
- `shell` without `CMD` opens `$SHELL` (else `/bin/sh`) in the worktree with `WEFT_SESSION` set to the session's name; it needs a terminal, and `exit` returns. With `-- CMD` it runs that command there instead, terminal or not, and exits with its status (127 when the command is not found). `session new --shell` opens the same shell once the session exists, and without a terminal refuses before creating anything.
- Inside a worktree no `--template` or `--session` is needed: weft walks up to `.weft/worktree.toml`. Anywhere else `status`, `add`, `reset`, `diff` and `commit` need `-s NAME`, even when the template has one session; `session scope|refresh|path|shell|end` still fall back to the only one.

## Staging and committing

```text
weft add [PATTERNS]... [-A|--all] [-p|--patch]
weft reset [PATTERNS]...                                   # no --patch variant exists
weft status
weft diff [--abstracted] [--json] [--staged|--cached]
weft commit --name NAME [--title TEXT] [--describe TEXT] [--tag T]... [--when EXPR]
            [--yes] [--keep-literal ANSWER@PATH:LINE[:NTH]]... [--stack|--sibling]
            [--depends-on A,B | --after NAME] [--share NAME]
```

- Paths and globs are relative to where you stand, or to the worktree root when `-s` names the session from outside it. `weft add .` stages the subtree, `-A` the whole worktree. A pattern that matches no file fails and changes nothing.
- `-p` prompts `y n a d s q ?` per hunk; `s` splits a hunk that has internal context. A new file is one hunk and cannot be split, so trim a new file in the worktree before staging it.
- `commit` takes the staged set, or the whole worktree when nothing is staged. `--name` is the file stem; without it a terminal asks for one, offering `patch-NNN`, and an unattended commit (no TTY, or `--yes`) takes `patch-NNN`.
- `--tag` is repeatable on `commit`; `--tags a,b` in the docs is wrong.
- `--yes` accepts every abstraction candidate. `--keep-literal` keeps one occurrence literal and implies `--yes` for the rest; the keys come from `weft diff --json` (`occurrences[].id/path/line/nth`).
- `--depends-on` names must be in the session's base, and the patch must still apply with only their closure present. `--after NAME` is sugar for one parent. A slot filler passes `--depends-on <owner>`.
- Commit context never comes from lines an `expr` rendered or from slot content; a change with only such lines around it is refused unless it appends at the end of the file.
- Lines added inside a slot are recorded as a `fill_slot` keyed by the patch name, and `weft diff` prints ``note: line 3 fill slot `servers` under key `lightdash` `` under the file, using the session's name as the key. The block must sit where its key sorts, with the separator ending every contribution but the last; commit refuses it otherwise and prints the layout it expects.
- A commit whose patch creates a file an independent patch already creates says so. Without a terminal, or with `--yes`, it commits as before and prints ``note: patch `linear` (when use_linear) also creates .mcp.json. The two clash in a project that has both on; if they belong together, run `weft share .mcp.json --name NAME` `` (with several other creators: `They clash in a project that has them all on`), plus ``note: weft cannot share it from here: <reason>`` when this commit cannot share it, for example ``a generated or foreach patch cannot fill a shared file``. In a terminal it asks ``patch `linear` (when use_linear) also creates .mcp.json. Can both be on in the same project?``, and `Yes: share the file` runs `weft share` from the commit.
- `--share NAME` shares every such file in the same commit: ``created patch `mcp`: …``, ``rewrote patch `linear`: …``, ``committed patch `jira`: adds its lines to .mcp.json, depends on `mcp` ``, and the session ends. It takes the default title `Shared .mcp.json` without asking; change it with `weft patch set`. It needs the commit to take the whole worktree (``2 file(s) are still uncommitted in this session, and sharing ends it``) and no other open session, and fails with ``--share: no other patch creates a file this patch creates`` when nothing clashes.
- In a `weft patch amend` session, `--title`, `--describe` and `--tag` apply as `weft patch set` would, in the same save as the ops. `--when`, `--depends-on`/`--after` and a `--name` other than the patch's are refused before anything is written; edit those in the JSON.
- Amending a patch that declares a slot shows every filler's lines in the slot, whatever their gates, and `weft diff` marks them: ``note: lines 3-4: the lines `jira`, `linear` add to slot `entries`, which stay theirs``. A slot nobody fills shows as the line `⟪slot entries: other patches add their lines here⟫`, noted ``line 4: the place of slot `entries`, where other patches add their lines; keep the line``. Edit the lines around them; commit writes the slot back where those lines are and keeps `omit_when_empty`. Changing, splitting or deleting them makes `weft diff` print `note: cannot record: …` and commit refuse (see the errors table).
- `commit` ends the session, and a worktree weft created is deleted with it, so nothing later in the same shell command can run in it.

## Patches after the fact

```text
weft patch ls [--template DIR]
weft patch set NAME [--title TEXT] [--describe TEXT] [--tag T]... [--clear-tags]
weft patch amend NAME [--answer K=V]... [--preset P]... [--answers-file F] [--force] [--non-interactive]
weft patch squash NAME NAME... --into NAME [--title TEXT]
weft patch resync [NAMES]... [--all] [--answer K=V]... [--keep-literal SPEC]... [--dry-run] [--yes] [--json]
weft patch set-command NAME [COMMAND] [--resync]
weft patch detach NAME
weft share PATH... [--name NAME] [--title T] [--describe D] [--example PATH=FILE]... [--yes] [--dry-run] [--template DIR]
```

- `set` never changes an id. An empty string clears a field.
- `amend` opens a session named after the patch (one per patch at a time) and prints its worktree path; `weft commit --yes` inside it rewrites the patch in place. The id changes and dependents replay. A dependent whose anchors no longer match is not named, the amend is written anyway, and `weft patch amend` on that dependent then fails to render (verified 2026-09-29). So before an amend that touches lines a dependent anchors on, move the dependent's JSON out and keep its title, description, `depends_on` and `when`; after the amend, re-record it in `weft session new NAME` with `weft commit --name NAME --depends-on … --when … --title … --describe …`. A slot filler keeps its key.
- `squash` members must be convex in the graph, share a gate, and be neither generator nor foreach patches.
- `resync` refuses while any session is open. `amend` refuses a generator patch until `detach`. `resync` still skips a generated patch that declares a slot, with an issue: ``it declares slot(s) `entries` in `.mcp.json`, which regenerating from the command would drop; to keep them, `weft patch detach mcp` and change it with `weft patch amend mcp` ``. `weft share` never makes one, because it refuses a generated creator.
- `share` takes a file that two or more independent patches each create. The lines their versions share before and after their own become a new patch NAME that creates the file around a slot `entries`, with `omit_when_empty` and no gate, depending on what every creator depended on. Each creator's `create_file` becomes a `fill_slot` keyed by its name, and it gains `depends_on` NAME. Each patch alone renders the same bytes as before; with all on the file lists every patch's lines in name order. The separator is `,` for `.json`, `.jsonc` and `.json5`, none otherwise.
- Scripted, or with `--yes`, `share` takes weft's proposal and `--name` is required (``--name is required: the name of the new patch that owns .mcp.json``). In a terminal it opens the combined file in `$VISUAL`/`$EDITOR` and works out the split and separator from what you save, then asks `Name of the patch that owns .mcp.json [mcp]` and `Title [Shared .mcp.json]` for flags you left out. `--example PATH=FILE` gives the split as a combined file: TOML tables with a blank line between them make the separator a newline. `--dry-run` prints `.mcp.json, with every patch on:` and the file on stdout, and ``dry run: would create patch `mcp` and rewrite `jira`, `linear` `` on stderr.
- `share` prints ``created patch `mcp`: creates .mcp.json, left out while no patch adds to it``, one ``rewrote patch `jira`: adds its lines to .mcp.json, depends on `mcp` `` per creator, and `run `weft check` with answers that turn them on`. The owner's default description is `Holds the lines of .mcp.json that the patches adding to it share. A file is left out while no patch adds to it.`
- Once a file has an owner, the next contributor is recorded as a filler (`weft session new github --base mcp`, then `weft commit --name github --depends-on mcp`). `share` refuses a file a patch already fills, or changes.
- `resync --answer` is stored in the generator metadata even when the output is up to date (``note: `base`: stored the --answer override(s) in its generator metadata (ops unchanged)``). `--keep-literal` re-derives the ops even when the output is unchanged, so it repairs an occurrence an earlier resync abstracted (`gen: rewritten (1 op(s))`); when the ops come out the same it only stores the specs. Questions added since recording take their defaults, as on `weft update`. When the regenerated ops reference an answer the previous version did not, the patch is skipped with the occurrence keys (`docs_url@README.md:1:1`) until `--yes` accepts them or `--keep-literal` keeps them literal.
- Rename: `mv patches/old.json patches/new.json`, then edit every `"old"` in `depends_on` arrays. Delete: `rm`. Both followed by `weft check`.

## Hooks

```text
weft hook add PATCH --id ID --phase pre|post --effect check|setup|deploy --label TEXT --action CMD
              [--description TEXT] [--when EXPR] [--after HOOK_ID]... [--before HOOK_ID]... [--input glob:P|answer:ID|hook:ID]...
weft hook rm PATCH ID
weft hook ls                                               # every hook, in execution order, with when it runs
```

`--input` is post-only. Bad `--after` or `--input` references are rejected and the patch file is rolled back. Leaving out `--action` opens `$EDITOR` for the command, so a script always passes it. `hook ls` ends each line with when the hook runs: `(runs: create + every update)` for a pre hook, `(runs: create only)` for a post hook without inputs, `(runs: create + update on glob:pyproject.toml)` otherwise; `hook add` prints the same, and `describe --json` carries it as `runs`.

## Presets

```text
weft presets list
weft presets show NAME
weft presets save NAME [TEMPLATE] [--answer K=V]... [--fix K=CHOICE]... [--block K=CHOICE]...
weft presets rm NAME
```

`save` writes `presets/NAME.toml` and appends a `[[preset]]` entry to `weft.toml`. A preset locks what it answers; secrets cannot be preset.

## Validation and description

```text
weft check [TEMPLATE] [--answer K=V]... [--preset P]... [--answers-file F] [--json] [--frozen]
weft graph [TEMPLATE] [--answer K=V]... [--preset P]... [--json] [--diff PATCH]
weft describe [TEMPLATE] [--json | --agents-md [PATH]]
weft schema [--out DIR]                                    # weft-patch.schema.json, weft-manifest.schema.json
```

- `check` without answers validates the manifest, expressions, graph and slot declarations only, unless every question has a default, in which case it renders under the defaults. With answers it renders every patch on top of its own dependencies, then the whole graph, then every independent pair for commutation, under those answers. A patch that fails on its own dependencies is reported once (``patch `mine` does not apply under these answers: …``) and left out of the rest. Pairs whose only shared files both merely fill are counted, not rendered: `commutation ok for 2 independent pair(s); 1 more only fill the same slots, which commutes by construction`. `--json` returns `{ok, issues, notes}`.
- `describe --json` returns `template`, `questions`, `presets`, `patches` (with `title`, `description`, `when`, `depends_on`, op summaries), `hooks` in execution order, `includes`, `usage`. Its `usage.author` lines are the recording loop: `cd "$(weft session new <patch> --template DIR … --non-interactive)"`, edit, then `weft commit` from the worktree. On 0.1.0 they still say `weft record`; ignore them there.
- `describe --agents-md` writes `<template>/AGENTS.md`; `-` writes to stdout.

## Consuming a template

```text
weft new TEMPLATE [DEST] [--preset P]... [--answer K=V]... [--answers-file F] [--answers-json J|@file|-]
         [--instance INCLUDE=KEY]... [--skip-tasks] [--non-interactive] [--frozen] [--offline]
weft update [DEST] [--dry-run [--diff]] [--template DIR | --to REV] [--answer K=V]... [--preset P]... [--answers-file F]
            [--answers-json J] [--unset ID]... [--allow-dirty] [--skip-tasks] [--non-interactive] [--frozen] [--offline]
weft answers [DEST] [--json]
weft instance add INCLUDE KEY [--answer ID=V]... | list | remove INCLUDE KEY
```

`DEST` must be empty or absent; two templates cannot be scaffolded into one directory. `--skip-tasks` renders without running hooks. In a terminal, `new` asks every question that no flag, file or preset answered, offering its default; `--non-interactive` takes the defaults and fails only on a question without one.

`update` merges each file three ways: the template's last render, the project's copy and the new render. A file the project deleted stays deleted when the new render changes it (`chapters/tour.mdx: kept it deleted: you deleted it, and the new render changes it`). A file the project rewrote, such as starter content replaced by the user's own, takes every template change to it as a conflict, and the post hooks that update would run are held back until a later update finds the markers gone. So starter content a user is meant to replace goes in files of its own, which they delete, never in the file they write in (verified 2026-09-30 on `slides`).

In a terminal a bare `update` lists the project's answers, include instances' as `svc.port` or `connector.stripe.port`, and asks `Change any of these answers? [y/N]`. On yes, a project with instances asks `Whose answers?` (the project's own is checked; space toggles each instance). Each picked frame's open questions come back offering their current value: the stored answer, or a derived one re-derived from what was just typed; an instance's prompts start with its name (`connector.github · Project name`). What you type is stored as given, an accepted offer keeps its origin, and a prompt that names a `(template default: "my-demo")` hands the answer back to that default or bind when given it, as `--unset` would (the report then marks it `(derived)` or `(bind)`). Either way it asks each root question the project never answered under an open gate (new to the template, or behind a gate that opened), offering the default. Answer flags skip the review question, `--dry-run` asks neither and previews with defaults, and `--non-interactive` or no terminal keeps the answers and takes new questions' defaults (failing on one without). Conflict markers are diff3-style: `<<<<<<< local`, the project's lines, `||||||| base`, the last render's, `=======`, the new render's, `>>>>>>> template`. In a terminal each conflict is shown as `it was`, `you have` and `the template now has`, then `Keep:` offers yours, the template's, both (yours first), an edit of the block in `$VISUAL`/`$EDITOR` (saved with markers, it stays a conflict), or the conflict markers (the default). A file left marked goes into `[state] conflicts` and blocks the next update; that update's post hooks go into `[state] pending_hooks` (``held back post-hook(s) `mark-synced` because of conflicts; the next `weft update` runs them once the markers are gone``) and run on the next update that finds the markers gone, even when nothing else changed. A post hook that fails, on `weft new` or an update, leaves itself and the hooks after it there too (``post-hook `sync` failed; the next `weft update` runs `sync`, `report` ``); the hooks before it stay done. An agent runs `weft update --non-interactive`, or the MCP `update_project`, which never runs hooks (verified 2026-10-09 on a build from source, in a pseudo-terminal).

## Composition and hub

```text
weft lock [TEMPLATE] [--upgrade] [--registry URL]
weft hub publish OWNER/NAME --version X.Y.Z [--template DIR] [--registry URL] [--token T]
weft hub search TEXT | info OWNER/NAME
```

## Errors you will meet

| Message | Cause | Do |
| --- | --- | --- |
| `this patch is gated off under the session answers, so the session cannot continue on top of it` | `--when` false under the session's answers, with changes left | record with answers that make the gate true, or give the patch its own session |
| `this patch does not apply with only `a` in the base` | `--depends-on` too narrow | declare the patch it anchors on too |
| `replaying the recorded patch does not reproduce the worktree` | ambiguous hunk context | add a distinguishing line, or commit the file whole |
| `a dependent patch no longer applies after the amend` | the amend changed lines a dependent's hunks anchor on; the amend is already written | restore the patch file from git, set the dependent aside, amend again, then re-record the dependent under its old name |
| `--sibling` on an adopted session (refused with an error) | adopted worktrees always stack | use `--depends-on base` on later commits to keep them independent |
| `destination … is not empty` | `weft new` into a used directory | pick an empty directory; adopt the existing one instead |
| `base state hash changed since the session started` / `pinned base patch … no longer exists` | the template moved under an open session | copy the worktree files out, `weft session end NAME --discard`, start again |
| `not inside a session worktree of …; its sessions: …` | a worktree command run outside a worktree without `-s` | run it inside the worktree (`weft session shell NAME` opens a shell there), or pass `-s NAME` |
| `` `src/auth/` matched no file in session `promote`; nothing changed`` | a typo, a directory without `/**`, a path outside an adopted session's scope, or a template-root path given with `-s` | fix the path; `dir/**` for a directory, `weft session scope --add GLOB` for the scope |
| ``the change at line 2 sits between lines that read differently under other answers or with other patches active (`expr` segment …)`` | the only lines around the change were rendered by an `expr` or are slot content | make the change next to a literal line |
| ``with this patch's lines under key `lightdash`, slot `servers` renders as: …`` | the new block in a slot is out of key order, or a separator is missing or extra | move the block where its key sorts; every contribution but the last ends with the separator |
| ``the worktree changed slot `servers` in a way no single contribution explains`` | two blocks in one slot, or an edit to another patch's contribution | one block per patch; change another patch's lines by amending that patch |
| ``patches `linear` and `twin` both fill slot `servers` of `.mcp.json` under key `linear` `` | two fills with one key in one slot | rename one key in its JSON |
| ``slot `entries` of `.mcp.json` held the lines `jira`, `linear` add, and they are no longer in the file as they were:`` then the lines, then `keep them together and unchanged (to change a patch's lines, amend that patch)` | in `weft patch amend` of a slot owner, the fillers' lines (or the marker line of an empty slot) were changed, split or deleted | restore them as shown; amend the filler to change its lines |
| ``create_file target `.mcp.json` already exists; if both belong in one project, run `weft share .mcp.json --name NAME` so each adds its lines to one shared file`` | `weft check`: two independent patches create one file | `weft share` it, or gate them so they are never on together |
| ``note: patch `linear` (when use_linear) also creates .mcp.json. The two clash in a project that has both on; …`` | `weft commit`, scripted, recorded a second creator of a file | as above; `--share NAME` on the commit does it at once |
| ``in `.mcp.json` the patches' lines go in the order of the patches' names (`jira`, `linear`), in every project; put them in that order`` | `weft share`: the saved file or `--example` puts the patches' lines out of name order | reorder them |
| ``` `.mcp.json` as saved is not the shared lines with each patch's own lines between them: line 4 reads `…` where weft proposed `…`. …``` | `weft share`: a patch's own line changed in the saved file | move where the shared lines end or change the separator only; amend the patch first to change its lines |
| ``patch `tweak` changes `.mcp.json`; weft shares a file that patches only create`` | `weft share`: another patch has a hunk on the file (also the case for a file `base` creates and siblings edit) | no shared file for this; keep the dependency, or hunks three lines apart |
| ``patch `jira` adds lines to a slot of `.mcp.json`; weft shares a file that patches only create`` | `weft share` on a file that already has an owner | re-record the new creator as a filler with `--base mcp --depends-on mcp` |
| ``patch `gh` is generated by a command that writes the whole file; detach it first with `weft patch detach gh` `` | `weft share` with a generated creator | detach it, or keep it apart |
| ``session(s) `open1` are open in `.`; commit or end them before sharing a file`` | `weft share` with open sessions | commit or `weft session end` them |
| `` `.mcp.json` does not exist on top of its dependencies; depend on the patch that creates it`` | a filler without `depends_on` on the owner | add the owner to `depends_on` |
