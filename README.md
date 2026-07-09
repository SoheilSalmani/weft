# weft

Record-based project scaffolding. Weft replaces hand-authored Jinja templates
(Copier-style) with **recorded patches**: template authors edit real, rendered
files, and weft captures the change as a patch of *inputs* — an answer-schema
delta plus context-anchored operations — never output diffs.

Core ideas:

- **Patches store inputs, not outputs.** Rendering is deterministic:
  `render(base, answers, patches) → tree`. Same inputs, byte-identical output.
- **Record against a clean base.** `weft record` materializes a pinned base
  state into a scratch worktree; your dirty working directory never leaks in.
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

## Build

```sh
cargo build --release          # produces target/release/weft
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
weft record --template /tmp/tpl --answer "project_name=My Demo"
# prints: /tmp/tpl/.weft-record/worktree
```

Edit the worktree with your normal tools — it's just files:

```sh
echo "serve-my-demo:\n\techo My Demo" > /tmp/tpl/.weft-record/worktree/Makefile
weft commit --template /tmp/tpl --name makefile --yes
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
# check: render ok (5 files)
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
