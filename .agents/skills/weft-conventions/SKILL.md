---
name: weft-conventions
description: House conventions and the mental model for Weft, the record-based template engine (weft.toml, patches/*.json, sessions, hooks, extends, includes). Covers what one patch may contain, how to name patches, titles, descriptions, questions, hooks and presets, which tools are hardcoded rather than asked, and how to change a patch that already shipped. Use when reading, writing, reviewing or editing anything in a Weft template, running weft commands, or when a request mentions weft, patches, `weft.toml`, or scaffolding a project from a template. No model was trained on Weft, so read this before assuming anything about it. To build a whole template from nothing use creating-weft-templates, to promote a real project into one use extracting-weft-templates, and to carve a template into two use splitting-weft-templates.
---

# Weft conventions

Weft is a personal project. No model has seen it in training and its documentation is not online, so nothing about it may be assumed from memory. Everything here was verified against `weft 0.1.0` on 2026-09-25, and the composition facts again on 2026-09-27, after `extends` and include nodes landed. Where the docs and the binary disagree, the binary wins, and `weft <command> --help` is the check to run before relying on any flag. `references/cli.md` is the verified command sheet.

## The model

- A template is a directory holding `weft.toml`, the one file written by hand, plus `patches/<name>.json`, an optional `presets/`, and `.weftignore`.
- A patch is a function from answers to tree edits: `depends_on` (the **names** of other patches), an optional Starlark `when` gate, `foreach` on an integration patch, and `ops`. Its id is a blake3 hash of exactly those, computed on every load and never stored. Change the behaviour of a patch and every descendant's id changes. That is intended: a scaffolded project pins the ids it was rendered from and carries its own copy of their bodies in `.weft/base.json`, so `weft update` still merges after amend, squash or rename.
- `title`, `description`, `tags`, `hooks` and `generator` are metadata. Never hashed, editable at any time, and how future readers understand the patch.
- Rendering replays the active patches over an empty tree in dependency order, ties broken by id. Two patches with no dependency path between them must produce the same tree in either order. `weft check` tests every independent pair under the answers you pass it, and only those answers.
- File content is lines of segments: a literal string, `{"answer": "id"}`, or `{"expr": "…"}`. A `modify_file` hunk anchors on two lines of context each side and must match exactly once in the rendered file.
- Nothing in a template contains template syntax. You **record**: render a base into a worktree, edit real files with real values, then `weft commit` diffs the worktree, replaces occurrences of answer values with references, replays the result to prove it reproduces the worktree, and writes the patch. Abstraction only matches values of answers that already exist in the session, so a question is declared before the value it should capture is typed.
- Questions live in `weft.toml`. Answers arrive from Starlark defaults, an answers file, `--answer`, `--answers-json`, then prompts; later wins. A preset locks what it answers.
- Hooks belong to patches. `pre` runs before any file is written, `post` after; the effect is `check`, `setup` or `deploy`. Hooks are the only place shell exists.
- `extends = "../base"` in `[template]`, or a `gh:` or `hub:` ref, imports a base template as it is: its questions, includes and patches keep their names and ids. The extender's patches depend on inherited ones by plain name, and an inherited patch is amended in the base, never in the extender. A path ref is live; a git or hub ref is pinned in `weft.lock` and moved by `weft lock --upgrade`.
- `[[include]]` mounts a whole child template at `path`. A single include's patches are nodes `<include>/<patch>` in the parent's graph: a parent patch may depend on them, and may edit files under the mount only once it does. `path = ""` mounts at the root, where two patches creating the same file is a render error. A repeat include is one opaque node that only `foreach` patches reach. Nothing overrides a patch: there is no overlay.

Weft is not Copier or cookiecutter. Bring none of their habits: no `{{ }}` in files, no templated file names, no answers file inside the output.

## The loop

```sh
cd $(weft session new docker --base base --answer "project_name=Demo Service" --answer use_docker=true)
# edit real files with real values, or let a tool write them
weft diff                                   # concrete text, abstraction candidates highlighted
weft commit --name docker --title "Docker image" \
  --describe "Adds a uv-based Dockerfile that installs from the lockfile." \
  --when use_docker --yes
weft check --answer "project_name=Demo Service" --answer use_docker=true
```

These invocations, and the others in this skill, are how an agent runs weft unattended: `--yes`, `--non-interactive` and `--no-tui` stand in for a person. A page written for people shows what a person types; the `writing-documentation` skill has that rule.

Facts that shape the loop, all observed on the binary:

- `weft session new` prints only the worktree path on stdout; everything else goes to stderr, so `cd $(…)` works where there is no terminal. In a terminal the answers wizard opens even when every question is answered on the command line, and inside `cd $(…)` it blocks where the person cannot see it: add `--no-wizard`, or run `weft session new` on its own and `cd` into the path it prints. `weft patch amend` prints a path relative to the template.
- A commit that leaves nothing uncommitted ends the session and removes the worktree weft created. An adopted directory is only unlinked.
- With changes left over, the next patch is a **sibling** by default when scripted: the base stays put and the committed content is peeled back out, so the two must commute. `--stack` builds the next patch on the one just committed. Adopted sessions always stack, and `--sibling` is refused there.
- A patch whose `--when` is false under the session's answers cannot be committed while other changes remain. Record with answers that make the gate true, or give it its own session.
- After a failed commit the staged set is still staged. Run `weft status` and `weft reset` before staging the next piece, or the next commit takes both.
- Record with distinctive answer values (`Demo Service`, not `test`) and check `weft diff --abstracted` before committing. A generator that names the project after its directory writes the literal `worktree` into files; pass the name explicitly (`uv init --name ${package_name}`) or the answer is never used and every project inherits the leak.

## One patch is one increment

A patch is the smallest change that leaves the rendered project in a state someone would want, and that a single `when` could switch off. Tests, in order of usefulness:

1. **The title needs no "and".** "JUnit 6 + AssertJ" is one test stack; "Docker and CI" is two patches.
2. **Gating it off leaves a coherent project.** If it would not, it belongs in the patch it completes, or it depends on that patch.
3. **New files over hunks.** Files at distinct paths commute for free. A hunk must anchor on lines its own dependency wrote, and sit at least three lines from any sibling's hunk in the same file, because the context radius is two.
4. **It depends only on what it anchors on or needs at runtime.** Record with `--base <ancestor>`, not the default `latest`, unless the change genuinely builds on the last patch. `weft commit --depends-on a,b` declares parents at commit time and refuses a claim the content cannot honour. A chain forced by one shared config block is legitimate; a chain that exists because nobody passed `--base` is not.
5. **Structure is a gate, value is a segment.** A feature whose presence changes which files exist is its own patch under `--when`. A value that changes inside one edit is an answer reference. An `{"expr": "'a' if flag else 'b'"}` around whole blocks is two patches.
6. **Recorded, not hand-written**, except `{"expr"}` segments (commit never emits them), test fixtures, and typo fixes in `added` lines. After any hand edit, `weft check`.
7. **It carries its own hooks.** The patch that introduces a tool owns `verify-<tool>`; the patch whose files a setup step reads owns that step, with `inputs` so `weft update` re-fires it.
8. **Nothing derived, secret, or accidental.** `.weftignore` lists build output, virtualenvs, lockfiles and caches before the first recording. A lockfile is regenerated by a `setup` hook, never stored. A secret only ever reaches content through a `kind = "secret"` question, which weft abstracts unconditionally.
9. **Named, titled, described.** Never the `patch-NNN` auto-name.
10. **`weft check` passes after every commit**, with the answers and every preset you ship.

## Naming

| Thing | Rule | Examples |
| --- | --- | --- |
| patch name | kebab-case **noun of the concern**; the root is `base`; variants are `<axis>-<variant>`; generated patches `<tool>-<thing>` | `docker`, `ci`, `spring-boot`, `conn-duckdb`, `shadcn-button` |
| title | noun phrase, sentence case, five words or fewer, no period: what the project gains | `Docker image`, `GitHub Actions CI`, `Spotless + palantir-java-format` |
| description | one or two sentences, third person, ends with a period: what it adds and the decision it embodies | `Adds a uv-based Dockerfile that installs from the lockfile and runs as a non-root user.` |
| tags | none, unless something reads them | |
| question id | snake_case; a feature toggle is `use_<concern>`, matching the patch it gates; exclusive alternatives are one `choice`; derived values are `computed = true` | `use_docker` for `docker`, `use_ci` for `ci`, `adapter`, `package_path` |
| prompt | a question ending in `?` for a bool, a noun label for anything else | `Ship a Dockerfile?`, `Project name` |
| hook id | `verify-<tool>` for a pre check, `<tool>-<verb>` for a setup step, `deploy-<target>` for a deploy | `verify-uv`, `uv-sync`, `git-init`, `deploy-vercel` |
| hook label | imperative sentence, no period | `Install dependencies` |
| session | the name of the patch it will record | `docker` |
| preset | `with-<feature>` or `no-<feature>` | `with-prisma-postgresql` |
| template | its directory name, naming the stack rather than the purpose | `java`, `fastapi` |

Names are nouns because every patch adds something, so the verb is noise, and because the name is read where a noun belongs: `--when use_docker`, `--base docker`, `depends_on: ["docker"]`, `weft patch amend docker`. A patch later amended to do more or less keeps a true name. Titles are noun phrases because a title labels a lasting node in the graph and a row in `weft patch ls`, `weft describe --json` and the studio, read by someone deciding whether they want it; "Add Docker support" describes an event that never happened. The `weft init` comment and `weft commit --help` still show imperative examples; the house style overrides them. `references/naming.md` has rewrites of names, titles and descriptions that went wrong.

## Questions: ask what varies, pin the house stack

A question is for identity (`project_name`, with slugs derived by `computed` questions), a structural option whose presence legitimately differs between projects (`use_<concern>`, or one `choice` between exclusive alternatives), a secret, or a value with no house default (organisation, package). Everything else is a decision the template makes:

- **Never ask which tool.** uv for Python, pnpm for Node, Gradle with the Kotlin DSL for Java. The house stack is recorded in `references/house-stack.md`; check the templates directory before trusting it.
- **Never ask a version.** Pin the tool and language versions in the patch content and bump them with `weft patch amend`. A version question freezes divergence into every project's `.weft/state.toml`; a pinned version moves every project forward on `weft update`.
- **Every optional question gets a default.** Starlark binds every name before `and`/`or` can short-circuit, so a gated-off question without a default breaks any expression that mentions it.
- `default` and `when` are Starlark source: a string default is `"'api'"`, a bool is `"True"`.
- Add `description` and `example` to anything an agent will answer; `weft describe --json` surfaces both.

`references/questions.md` holds the kinds, the preset semantics, secrets, and the eager-binding rule with its fix.

## Hooks

```sh
weft hook add base --id verify-uv --phase pre --effect check \
  --label "Verify uv is installed" --action "command -v uv" --no-tui
weft hook add app --id uv-sync --phase post --effect setup \
  --label "Lock and sync the Python environment" --action "uv lock && uv sync" \
  --input "glob:pyproject.toml" --no-tui
```

- `check` is read-only and safe to auto-run. `setup` must be idempotent. `deploy` is external and irreversible; it is confirmed, never assumed, and gated with `--when` on an explicit answer.
- A post hook without `inputs` runs only on the first `weft new`. Use that for `git init`. Anything else declares `glob:`, `answer:` or `hook:` inputs.
- Ordering across patches is `--after <hook-id>`; references to hooks on inactive patches are ignored, so a finalize hook lists every possible predecessor.
- Recording and committing never run hooks. `weft new --skip-tasks` renders without them.

## Changing a patch that already shipped

Patches are mutable. Projects survive because they carry their own base.

| Want | Do | Ids |
| --- | --- | --- |
| different behaviour | `weft patch amend NAME`, edit the scratch worktree, `weft commit --yes` | patch and descendants change; dependents replay, a broken one is named |
| better words | `weft patch set NAME --title … --describe …` | unchanged |
| two patches that always travel together | `weft patch squash A B --into A --title …` | one new id |
| a generated patch, regenerated | `weft patch resync NAME` or `weft patch set-command NAME "…" --resync` | changes |
| a rename | `mv patches/old.json patches/new.json`, then fix every `"old"` in other patches' `depends_on` | unchanged |
| a different parent | edit `depends_on` by hand (there is no `weft patch rebase`), then check that it still renders | patch and descendants change |
| gone | `rm patches/NAME.json` | descendants become roots or break; check |

Never fork a `docker-v2`. Run `weft check` after every one of these: amend, re-parent and delete can break a dependent's anchors, and the others are cheap to confirm.

## Prove it

- `weft check --answer … [--preset …]` after every commit, once per preset and per answer combination you ship. It renders and tests commutation only under the answers it is given.
- `weft graph --answer …` to see the shape. A star around `base` is the usual healthy shape; a chain is a smell unless each link anchors on the previous one.
- `weft new TEMPLATE /tmp/out --answers-json '{…}' --non-interactive`, then run the project's own setup, because check proves the render, not that the result builds.
- `weft describe --agents-md` after the questions or patches change; it writes the template's `AGENTS.md`.

## Never invent

- A stored `id`, a numeric prefix, or ordering by file name. Order comes from `depends_on` alone.
- Op types beyond `create_file`, `create_binary_file`, `modify_file`, `delete_file`, `rename_path`, `set_mode`; line-numbered hunks; fuzzy matching.
- Question kinds beyond `string`, `bool`, `int`, `choice`, `multichoice`, `secret`; validation keys; secret sources beyond `env:`, `cmd:`, `prompt`.
- Hooks in `weft.toml`, a `[[task]]` table, hook fields such as `cwd` or `env`, phases or effects beyond the three named.
- `weft record` (now `weft session new`), `weft patch split`, `rebase`, `rename`, `gate`, `weft question add`, `weft reset -p`, or `layers` in `weft.toml`. `weft describe --json` still prints `weft record` under `usage.author`; it is stale.
- Abstraction of ints or bools, or a question weft proposed on its own. It never does.
- A live documentation URL. The docs are the `docs/` directory of the weft-cloud repository; `weft --help`, `weft <cmd> --help` and `weft describe --json` are what is reachable from a shell.

`references/patch-format.md` is the file format for the times a hand edit is right, including the portable-patch recipe.

## Before you finish

- Every command you relied on was confirmed by `--help` on the installed binary, or by running it.
- Every patch has a noun name, a noun-phrase title and a sentence description, and none is `patch-NNN`.
- No patch bundles two concerns, stores a lockfile, a cache, a secret literal, or the word `worktree`.
- Every optional feature is a gated patch; no tool or version is a question.
- `weft check` ran with every preset and passed; the graph is the shape you meant.
