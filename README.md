# weft

Record-based project scaffolding. Weft replaces hand-authored Jinja templates
(Copier-style) with **recorded patches**: template authors edit real, rendered
files, and weft captures the change as a patch of *inputs* — an answer-schema
delta plus context-anchored operations — never output diffs.

Core ideas:

- **Patches store inputs, not outputs.** Rendering is deterministic:
  `render(base, answers, patches) → tree`. Same inputs, byte-identical output.
- **Record against a clean base.** `weft session new` materializes a pinned
  base state into a worktree; your dirty working directory never leaks in.
  Sessions work like git worktrees: several at once, each with its own staging
  index and its own directory, which `weft session shell` opens a shell in.
- **Abstraction happens at commit.** You edit concrete files ("My Demo");
  `weft commit` proposes which literals correspond to which answers and stores
  `{"answer": "project_name"}` segments instead.
- **Patch composition, Pijul-flavored.** Patches form a dependency DAG;
  independent patches must commute (`weft check` proves it by applying them in
  both orders).
- **Starlark, not shell, in the manifest.** Conditions, defaults, and derived
  values are Starlark expressions; shell exists only inside task actions.
- **Secrets are references.** `env:VAR`, `cmd:...`, or `prompt` — resolved at
  render time, never persisted (the core `Value::Secret` refuses to
  serialize).

## Install

> Weft is pre-1.0. Homebrew and prebuilt binaries cover macOS and Linux
> (x86_64 and arm64); `cargo install` arrives once the crates are published.

**Homebrew** (macOS and Linux):

```sh
brew install SoheilSalmani/tap/weft
```

**Prebuilt binary**:

```sh
curl -fsSL https://raw.githubusercontent.com/SoheilSalmani/weft/master/install.sh | sh
```

**From source** (needs a Rust toolchain, Linux or macOS):

```sh
git clone https://github.com/SoheilSalmani/weft
cd weft
cargo build --release          # produces target/release/weft
install -m 0755 target/release/weft ~/.local/bin/weft   # put it on your PATH
```

**cargo** (once published to crates.io): `cargo install weft-cli`.

Full instructions and the documentation: <https://github.com/SoheilSalmani/weft>.
The docs site (guides, tutorials, cookbook, CLI reference) is built from the
`weft-cloud` repo and published separately. How to write its tutorials, and
the scripts that replay them, is in
[`docs/tutorials/README.md`](https://github.com/SoheilSalmani/weft-cloud/blob/master/docs/tutorials/README.md)
there.

### Developing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace         # full test suite (unit + e2e)
```

## Five-minute tutorial

The repo ships a tiny fixture template. Scaffold a project from it; weft asks
for each answer the command leaves out, offering its default, so press Enter
to accept them:

```sh
alias weft=target/release/weft

weft new fixtures/templates/hello /tmp/demo --answer "project_name=My Demo"
cat /tmp/demo/README.md
# # My Demo
#
# Scaffolded by weft.
#
# Ships with Docker.
```

`package_name` was derived by a Starlark default
(`project_name.lower().replace(' ', '-')` → `my-demo`), `use_docker` defaulted
to `True`, and the `mark-synced` task ran because its `glob:pyproject.toml`
input was created. Presets are layered partial answer-sets:

```sh
weft new fixtures/templates/hello /tmp/demo2 \
    --preset no-docker --answer project_name=Slim
ls /tmp/demo2          # no Dockerfile
```

### Evolve the template by recording

Copy the fixture somewhere writable, then record a change against a rendered
base state. `--shell` opens a new shell in the session's worktree once it is
rendered. First weft asks for the answers you left out, as `weft new` did;
press Enter to accept the defaults:

```sh
cp -r fixtures/templates/hello /tmp/tpl
weft session new makefile --template /tmp/tpl --answer "project_name=My Demo" --shell
```

The name (`makefile`) is optional; leave it out and the session is `default`.
The worktree is just files, so edit it with your normal tools. In your editor,
create `Makefile` in the worktree:

```make title="Makefile"
serve-my-demo:
	echo My Demo
```

Commit from the worktree; weft finds the template by walking up, and paths are
relative to where you stand. The commit takes the whole worktree, so it ends the
session and removes the worktree, and `exit` returns you to where you started:

```sh
weft add Makefile
weft commit --name makefile --yes
exit
```

Commit diffed the worktree against the base, noticed `my-demo` and `My Demo`
are the current values of `package_name` and `project_name`, and stored the
patch abstracted. Scaffold with different answers to see it adapt:

```sh
weft new /tmp/tpl /tmp/other --answer "project_name=Other App"
cat /tmp/other/Makefile
# serve-other-app:
#         echo Other App
```

### Update scaffolded projects

Projects remember their template and pinned base in `.weft/state.toml`.
When the template gains patches, `weft update` re-renders both states and
3-way merges the difference over your local edits:

```sh
weft update /tmp/demo --dry-run          # show the plan
weft update /tmp/demo --dry-run --diff   # …and the unified diff it would apply
weft update /tmp/demo                    # apply
```

In a terminal, `weft update` is interactive. It lists the project's answers
and asks whether to change any (Enter keeps them), asks every question the
project never answered, such as one the template added since, offering its
default, and puts each conflict to you before writing the file. Without a
terminal, or with `--non-interactive`, it keeps the answers, takes new
questions' defaults (failing on one without), and leaves conflicts marked in
the files.

A conflict is a region that you and the template both changed, differently.
In a terminal you see what the template had there, what you have, and what
the template has now, and keep yours, the template's, both, an edit of the
block in `$EDITOR`, or the markers to resolve later. In the file the markers
carry all three:

```
<<<<<<< local
version = "0.2.0"
||||||| base
version = "0.1.0"
=======
version = "1.0.0"
>>>>>>> template
```

A file left with conflict markers blocks the next `weft update` until you
resolve it. Inside a git work tree, `weft update` also refuses to write over
files with uncommitted changes, so `git diff` shows exactly what the merge
did and `git checkout` undoes it; `--allow-dirty` overrides.

Pre hooks run on every update. A post hook runs when its project is created,
or its include instance (`weft instance add` creates one too), and on a later
update only when one of its inputs changed: a file its `glob:` matches, its
`answer:`, or the hook its `hook:` input names ran in the same update. A post
hook without inputs therefore runs only at creation; `weft hook ls` shows
when each hook runs. Post hooks an update holds back because it left
conflicts run on the next `weft update` that finds the markers gone. So does
a post hook that fails, with the ones after it, whether it failed on `weft
new` or on an update: fix the cause and run `weft update`. Running `update`
twice in a row is a no-op.

### Change a project's answers

Run `weft update` in a terminal and say yes when it asks to change the
answers. A project with include instances is then asked whose answers to go
through: its own, and any instance's. Each question comes back with its
current value offered: Enter keeps it, and what you type replaces it. Where
one of your answers differs from the template's default (or an include's
bind), the prompt names that default, and giving it hands the answer back:
it follows the template again, as `--unset` does. Flags change answers
without prompts:

```sh
weft answers /tmp/demo                                  # what's set, and where it came from
weft update /tmp/demo --answer "project_name=New Name"  # re-render + 3-way merge
weft update /tmp/demo --answer svc.port=8080            # an include's answer
weft update /tmp/demo --unset package_name              # back to its default
```

`.weft/state.toml` keeps two kinds of answers. `[answers]` holds the ones you
gave; they stay until you change them. `[derived]` holds what the template
computed from them (defaults, computed values, include binds); those follow on
every update. So renaming `project_name` also moves a `package_name` that
defaulted from it, but not one you set yourself. Before any file is written,
the update lists every answer that changes, derived ones marked:

```
answers:
  project_name  "Old Name" → "New Name"
  package_name  "old-name" → "new-name"  (derived)
```

Projects scaffolded before weft recorded this store every answer as given.
Those values stay put, and the update points out the ones that no longer
match their default (`kept as given: … --unset package_name to follow it`).

Template changes arrive in the same run: when the template moved too, run
`weft update`, commit, then change answers to review the two separately.

### Templates from git

A template can live in a git repository — on its own or as one directory of
a repository of templates. The ref grammar is `<repo>[//<subdir>][@<rev>]`:

```sh
weft new gh:acme/react-template app                       # GitHub shorthand
weft new https://github.com/acme/templates.git//base app  # subdirectory
weft new git@github.com:acme/templates.git//base@v1.2 app # …at a tag
```

`@rev` is a tag, branch, or commit; without it the remote's default branch is
tracked. The project records the ref *and* the commit it rendered from:

```toml
[state]
template = "gh:acme/templates//base@main"
commit = "9f3c1e…"
```

`weft update` fetches, resolves the tracked rev again, and merges whatever
changed (an answer change re-renders even when the commit is unchanged); a
tag stays put until you move it. `weft update --to v2.0` (or a
branch, or a commit) retargets and tracks that from then on; `--offline`
updates from the local mirror only. Repositories are mirrored under
`~/.weft/git/` with one immutable export per commit, so a pinned commit
scaffolds with no network — the same guarantee as `hub:` refs. Everything
goes through your `git`, so SSH keys and credential helpers just work. A
template may `[[include]]` a git source the same way
(`template = "gh:acme/templates//go-service@v1"`); `weft lock` pins the
commit in `weft.lock`.

**What to commit in a scaffolded project:** `.weft/state.toml` and
`.weft/base.json` — they hold the source ref, the pinned base, and your
answers (secrets only as references), and they are what lets any clone of
the project run `weft update`. `.weft/worktree.toml` (written by `weft
session adopt`) is a machine-local pointer; weft drops a `.weft/.gitignore`
that excludes it.

### Validate a template

```sh
weft check /tmp/tpl --answer project_name=x
# check: render ok (4 files)
# check: commutation ok for N independent pair(s)
# ok: template `hello` passed all checks
```

## Template anatomy

```
my-template/
├── weft.toml            # [template], [[question]], [[preset]], [[include]], [refine.*]
├── weft.lock            # pinned versions/commits of remote includes
├── presets/*.toml       # partial answer maps; later layers win
└── patches/*.json       # the recorded patch DAG; deps by patch name
```

See `fixtures/templates/hello/` for a complete example and `PLAN.md` /
`PROGRESS.md` for the design and its implementation notes.

## Slots: several patches adding to one file

Two independent patches inserting after the same line do not commute, and
two that each create one file clash. When several patches each create a
file (one MCP server list per tracker, say), it gets an owner with a
**slot**, and each of them fills it with `fill_slot`. Nobody writes the
slot: record each patch creating the file the way it wants it, then share it.

```sh
weft share .mcp.json --name mcp            # or: weft commit … --share mcp
```

Weft compares the patches' versions of the file: the lines they share
become the owner `mcp`, with a slot where their own lines were, and each
patch now fills that slot under its name. In a terminal weft first opens the
combined file (every patch on) in `$EDITOR` to confirm where the shared lines
end and what separates the patches' lines (`,` in JSON); `--example
PATH=FILE` gives it that file instead. Each patch alone still renders exactly
what it did. `weft commit` notices a second patch creating one file and asks
whether to share it (or prints the `weft share` command), and `weft check`
suggests it for two creators that clash.

```json
{ "op": "create_file", "path": ".mcp.json", "omit_when_empty": ["entries"],
  "content": ["{", "  \"mcpServers\": {", { "slot": "entries", "separator": "," }, "  }", "}"] }

{ "op": "fill_slot", "path": ".mcp.json", "slot": "entries", "key": "lightdash",
  "lines": [["    \"lightdash\": { \"url\": \"", { "answer": "lightdash_url" }, "/api/v1/mcp\" }"]] }
```

- A slot renders its contributions sorted by `key`, the separator ending
  every one but the last; an empty slot renders nothing, and
  `omit_when_empty` leaves the file out while all its listed slots are empty.
- No hunk sees slot content, so patches that only fill slots commute in any
  order; `weft check` counts such pairs as commuting by construction. Two
  contributions with one key in one slot are an error naming both patches.
- `weft commit` records lines added inside a slot's span as a `fill_slot`
  keyed by the patch name. Place them where the key sorts (`weft diff` says
  which slot they fill); hunk context never comes from slot content, or from
  lines an `expr` rendered.
- `weft patch amend` on an owner shows every patch's lines in its slots (a
  marker line where none fills one); edit the lines around them, and commit
  puts the slot back where they are. Slots do not nest, and `weft patch
  resync` skips a generated patch that declares one.
- `weft share` takes only a file the patches each create. Lines several
  patches add to a file one patch created (a Gradle `dependencies {}` block)
  still need hunks that anchor apart, or a dependency chain.

## Composition: extends and include nodes

A template's graph is *composed*: every patch that renders when the template
does is a node in it, wherever that patch lives.

- **`[template] extends = "../base"`** (or `{ template = "hub:org/base",
  version = "^1" }`) imports a base template as-is: its questions, includes,
  and patches keep their names and ids and become root nodes of this
  template. Your own patches build on them like any other ancestor
  (`"depends_on": ["base-readme"]`); you cannot redeclare an inherited name,
  and `weft patch amend`/`squash` send you to the base template to edit it.
  `weft patch ls` flags inherited rows.

  The names are shared, not namespaced: a base and every template that
  extends it have one set of question ids, patch names, and hook ids. A base
  that gains a name an extender already uses therefore breaks that extender:
  a question id or patch name stops it loading, a hook id fails its `weft
  check`. A path base does it at once, a git or hub base at the extender's
  next `weft lock --upgrade`. Each error names every clash and how to settle
  it, so run `weft check` on every extender before you publish a base change.
- **Include nodes.** A single (non-repeat) `[[include]]` named `web`
  contributes its whole graph as `web/<patch>` nodes (nested:
  `web/svc/base`), rendered under its mount with the instance's answers. A
  root patch may depend on them: `"depends_on": ["web/next-config"]`. A
  gated-off child patch switches off every parent patch depending on it,
  across frames.
- **Root mounts.** An include with `path = ""` (or omitted) mounts at the
  root: its files merge with the parent's. Two roots creating the same path
  is a render error, so a template meant to be mounted must not own repo
  scaffolding (README, `.gitignore`, git init) — leave that to the outermost
  template.
- **The mount rule is inverted.** A parent patch may edit files under an
  include's mount *iff* it depends on that include's nodes — the dependency
  is what makes the child's files part of its base. `weft commit` infers the
  dependency from the paths you touched; a patch editing under a mount it
  does not depend on is rejected.
- **Repeat includes stay opaque.** A `repeat = true` include is one node
  standing for every instance; nothing may depend on it by name. Reach
  inside instances with an integration patch: `"foreach": "connector"`
  renders once per instance with `key` and `instance_<question>` in scope,
  so a path segment `{"answer": "key"}` (e.g. `["connectors/", {"answer":
  "key"}, "/README.md"]`) addresses that instance's files.

`weft graph --json` and `weft describe --json` list the composed graph:
include nodes carry `include`/`mount`, repeat nodes `opaque: true`,
inherited patches `inherited: true`, and the document its `extends`.

### Refining inherited questions

An extender cannot redeclare an inherited question, but it can narrow one
with a `[refine.<id>]` table
([ADR-0002](docs/adr/0002-extenders-narrow-inherited-questions.md)):

```toml
[template]
extends = "../base"

[refine.stack_skills]        # a multichoice inherited from base
choices = ["dbt", "sql"]     # keep only these (or: blocked = ["airflow"])
fixed = ["dbt"]              # always selected; the prompt names it, never offers it
default = "['sql']"          # pre-selected, still editable

[refine.use_jira]
lock = "False"               # never asked; --answer use_jira=true errors

[refine.project_name]
description = "Its snake_case slug names the dbt project."
```

Refinements only narrow. They cannot add a choice, change a kind, touch a
secret, or undo what a template further up the `extends` chain narrowed, so
everything an extender renders is something its base could render. A
refined `default` or `lock` keeps the inherited question's place in the
answer order, so it may only mention questions declared before it.

Blocked choices are never offered at the prompt and are gone from `weft
describe`; picking one on any input layer (flag, JSON, answers file, preset)
is an error that names the refining template. Fixed choices join whatever is
selected, and a default that lists a blocked choice simply drops it. Answers
you give are stored as given, fixed choices included; the template never
rewrites them. If a template narrows further after projects exist, `weft
update` stops on a stored answer that no longer fits and says how to change
it (`--answer`) or hand it back to the template (`--unset`).

## Workspace layout

| Crate         | Role                                                        |
| ------------- | ----------------------------------------------------------- |
| `weft-core`   | Data model, patch algebra, rendering, 3-way merge. Pure — no I/O. |
| `weft-lang`   | Starlark evaluation (conditions, defaults, derived values). |
| `weft-engine` | new/record/commit/update/check orchestration; all I/O.      |
| `weft-cli`    | The `weft` binary (clap).                                   |
| `tests/e2e`   | Full-binary tests against `fixtures/templates/`.            |
