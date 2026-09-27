# weft

Record-based project scaffolding. Weft replaces hand-authored Jinja templates
(Copier-style) with **recorded patches**: template authors edit real, rendered
files, and weft captures the change as a patch of *inputs* — an answer-schema
delta plus context-anchored operations — never output diffs.

Core ideas:

- **Patches store inputs, not outputs.** Rendering is deterministic:
  `render(base, answers, patches) → tree`. Same inputs, byte-identical output.
- **Record against a clean base.** `weft session new NAME` materializes a pinned
  base state into a worktree; your dirty working directory never leaks in.
  Sessions work like git worktrees: several at once, each with its own staging
  index, each a directory you `cd` into.
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

> Weft is pre-1.0. Building from source is the verified path today; Homebrew,
> prebuilt binaries, and `cargo install` arrive with the first tagged release.

**From source** (works today — needs a Rust toolchain, Linux or macOS):

```sh
git clone https://github.com/SoheilSalmani/weft
cd weft
cargo build --release          # produces target/release/weft
install -m 0755 target/release/weft ~/.local/bin/weft   # put it on your PATH
```

**Homebrew** (macOS and Linux, with the first release):

```sh
brew install SoheilSalmani/tap/weft
```

**Prebuilt binary** (with the first release):

```sh
curl -fsSL https://raw.githubusercontent.com/SoheilSalmani/weft/main/install.sh | sh
```

**cargo** (with the first release): `cargo install weft-cli`.

Full instructions and the documentation: <https://github.com/SoheilSalmani/weft>.
The docs site (guides, tutorials, cookbook, CLI reference) is built from the
`weft-cloud` repo and published separately.

### Developing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace         # full test suite (unit + e2e)
```

## Five-minute tutorial

The repo ships a tiny fixture template. Scaffold a project from it:

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
base state:

```sh
cp -r fixtures/templates/hello /tmp/tpl
cd $(weft session new makefile --template /tmp/tpl --answer "project_name=My Demo")
```

`weft session new` prints the worktree path, so `cd $(…)` puts you inside it.
Edit with your normal tools — it's just files — then commit from there; weft
finds the template by walking up, and paths are relative to where you stand:

```sh
echo "serve-my-demo:\n\techo My Demo" > Makefile
weft add Makefile
weft commit --name makefile --yes
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
weft update /tmp/demo                    # apply; conflicts get <<<<<<< markers
```

Tasks re-fire only when their declared inputs (file globs, answers, upstream
tasks) actually changed between the two renders. Running `update` twice in a
row is a no-op.

A file left with conflict markers blocks the next `weft update` until you
resolve it. Inside a git work tree, `weft update` also refuses to write over
files with uncommitted changes, so `git diff` shows exactly what the merge
did and `git checkout` undoes it; `--allow-dirty` overrides.

### Change a project's answers

The same update changes answers and re-renders:

```sh
weft answers /tmp/demo                                  # what's set, and where it came from
weft update /tmp/demo --answer "project_name=New Name"  # re-render + 3-way merge
weft update /tmp/demo --answer svc.port=8080            # an include's answer
weft update /tmp/demo --unset package_name              # back to its default
weft update /tmp/demo --reconfigure                     # edit them all in the wizard
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
├── weft.toml            # [template], [[question]], [[preset]], [[include]]
├── weft.lock            # pinned versions/commits of remote includes
├── presets/*.toml       # partial answer maps; later layers win
└── patches/*.json       # the recorded patch DAG; deps by patch name
```

See `fixtures/templates/hello/` for a complete example and `PLAN.md` /
`PROGRESS.md` for the design and its implementation notes.

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

## Workspace layout

| Crate         | Role                                                        |
| ------------- | ----------------------------------------------------------- |
| `weft-core`   | Data model, patch algebra, rendering, 3-way merge. Pure — no I/O. |
| `weft-lang`   | Starlark evaluation (conditions, defaults, derived values). |
| `weft-engine` | new/record/commit/update/check orchestration; all I/O.      |
| `weft-cli`    | The `weft` binary (clap).                                   |
| `tests/e2e`   | Full-binary tests against `fixtures/templates/`.            |
