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

> Weft is pre-1.0. Building from source is the verified path today; prebuilt
> binaries, `cargo install`, and Homebrew arrive with the first tagged release.

**From source** (works today — needs a Rust toolchain, Linux or macOS):

```sh
git clone https://github.com/SoheilSalmani/weft
cd weft
cargo build --release          # produces target/release/weft
install -m 0755 target/release/weft ~/.local/bin/weft   # put it on your PATH
```

**Prebuilt binary** (with the first release):

```sh
curl -fsSL https://raw.githubusercontent.com/SoheilSalmani/weft/main/install.sh | sh
```

**cargo** (with the first release): `cargo install weft-cli`.

Full instructions and the docs: <https://github.com/SoheilSalmani/weft>.

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
weft update /tmp/demo --dry-run     # show the plan
weft update /tmp/demo               # apply; conflicts get <<<<<<< markers
```

Tasks re-fire only when their declared inputs (file globs, answers, upstream
tasks) actually changed between the two renders. Running `update` twice in a
row is a no-op.

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
├── weft.toml            # [template], [[question]], [[preset]], [[task]]
├── presets/*.toml       # partial answer maps; later layers win
└── patches/*.json       # the recorded patch DAG; deps by patch name
```

See `fixtures/templates/hello/` for a complete example and `PLAN.md` /
`PROGRESS.md` for the design and its implementation notes.

## Workspace layout

| Crate         | Role                                                        |
| ------------- | ----------------------------------------------------------- |
| `weft-core`   | Data model, patch algebra, rendering, 3-way merge. Pure — no I/O. |
| `weft-lang`   | Starlark evaluation (conditions, defaults, derived values). |
| `weft-engine` | new/record/commit/update/check orchestration; all I/O.      |
| `weft-cli`    | The `weft` binary (clap).                                   |
| `tests/e2e`   | Full-binary tests against `fixtures/templates/`.            |
