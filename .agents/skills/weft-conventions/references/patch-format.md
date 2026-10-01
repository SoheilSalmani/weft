# The patch file, for the times a hand edit is right

Patches are normally written by `weft commit`, and slots by `weft share`. Open this when you need to read one, fix a typo in `added` lines, add an `{"expr"}` segment (commit never emits one), write a fixture, or move a patch between templates. After any hand edit, `weft check` with answers: ids are recomputed from content, so nothing else needs updating, but a broken hunk only shows up at render.

## Contents

- Top level
- Ops
- Segments and hunks
- Slots
- What is hashed
- Portable patches
- A recorded patch, verbatim

## Top level

```json
{
  "title": "Docker image",
  "description": "Adds a uv-based Dockerfile that installs from the lockfile.",
  "tags": [],
  "depends_on": ["app"],
  "when": "use_docker",
  "ops": [ … ],
  "hooks": [ … ],
  "generator": { … }
}
```

| Field | Hashed | Meaning |
| --- | --- | --- |
| `title`, `description`, `tags` | no | display metadata; `weft patch set` edits them |
| `depends_on` | yes | names (file stems) of parent patches; missing means a root |
| `when` | yes | Starlark gate; false skips this patch and every dependent |
| `foreach` | yes | integration patch rendered once per instance of the named include; must be a leaf |
| `ops` | yes | applied in order; omit for an action patch that only carries hooks |
| `hooks` | no | see the hooks section of `SKILL.md`; `weft hook add` writes them |
| `generator` | no | the `--exec` command, record-time answers and keep-literal keys; `weft patch resync` re-runs it |

The `title` field is absent from the docs' field table but present in every recorded patch and in `weft describe --json`.

## Ops

```json
{ "op": "create_file", "path": "README.md", "content": ["# hello", ["Project: ", { "answer": "project_name" }]], "mode": 420 }
{ "op": "create_file", "path": ".mcp.json", "omit_when_empty": ["servers"], "content": ["{", "  \"mcpServers\": {", { "slot": "servers", "separator": "," }, "  }", "}"] }
{ "op": "create_binary_file", "path": "app/favicon.ico", "data": "<base64>", "mode": 420 }
{ "op": "modify_file", "path": "README.md", "hunks": [ { "context_before": ["Scaffolded by weft."], "removed": [], "added": ["", "Ships with Docker."], "context_after": [] } ] }
{ "op": "delete_file", "path": "old.txt" }
{ "op": "rename_path", "from": "a.txt", "to": "b.txt" }
{ "op": "set_mode", "path": "scripts/run.sh", "mode": 493 }
{ "op": "fill_slot", "path": ".mcp.json", "slot": "servers", "key": "linear", "lines": ["    \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }"] }
```

- Paths are tree-relative, `/`-separated, never `..` or absolute. A path may itself be a segment array: `["src/main/java/", {"answer": "package_path"}, "/App.java"]`.
- `mode` is optional: `420` is `0o644`, `493` is `0o755`. Recording writes it on every file op.
- `create_file` errors if the path exists; `modify_file`, `delete_file`, `rename_path`, `set_mode` error if it does not. Weft picks `create_binary_file` for any file whose bytes are not UTF-8; you never mark it by hand, and binary content is never abstracted or hunk-merged.
- Content is an array of lines; a fully literal line is a plain string. Output ends with exactly one newline.
- `fill_slot` errors when the file does not exist on top of the patch's dependencies, so a filler always depends on the patch that creates the file.

## Segments and hunks

| JSON | Renders |
| --- | --- |
| `"literal"` | as is |
| `{"answer": "id"}` | the answer's value |
| `{"expr": "…"}` | the Starlark result: a string, `True`/`False`, or a decimal int |

A list never renders. Project it: `{"expr": "', '.join(components)"}`.

A hunk's `context_before + removed + context_after` must match at exactly one position. Zero matches means the file diverged; two means the context is ambiguous. Both are clean errors. An empty pattern appends at end of file. Context lines carry abstraction too, which is why a hunk recorded under `Demo Service` still matches a file rendered under `Orders`.

Commit takes context only from lines that render the same under other answers: never a line an `expr` rendered, never slot content. Context stops at the first such line, so a hunk may carry one line or none; a group of changes is split at one. A change with nothing to anchor on that is not at the end of the file is refused, naming the segments around it. A hunk written by hand, or by an older weft, that anchors on `expr` output fails `weft check` under other answers:

```text
error: patch `old` does not apply under these answers: hunk 0 does not match `mise.toml`: it expects `linear = "1"` before the change, where line 4 is empty, rendered from an `expr` segment of patch `base` (`'linear = "1"' if use_linear else ''`); context taken from an expression's output only holds under the answers it was recorded with, so re-record the hunk with context that avoids that line
```

## Slots

Siblings that insert after the same line do not commute, and siblings that create the same file clash. Where several independent patches each create one file, `weft share` gives the file an owner with a slot, and each of them fills it.

- **The slot line.** A slot is a line of its own, `{"slot": "servers", "separator": ","}`, in `create_file` `content` or in a hunk's `added` lines. `separator` is optional. The name is ASCII letters, digits, `-`, `_` and `.`, unique within the file. A slot inside a segment array does not parse, and a slot anywhere else, including inside a fill, is an error: slots do not nest.
- **`omit_when_empty: ["servers"]`** on `create_file` leaves the file out of the render while every listed slot is empty. A patch that changes that file by anything but a fill, while nothing it depends on fills a slot, fails: ``patch `header` does not apply under these answers: it changes `.mcp.json`, which patch `mcp` leaves out while its slots are empty (`omit_when_empty`), and nothing it depends on fills them; …``.
- **Filling.** `{"op": "fill_slot", "path", "slot", "key", "lines"}`. `key` is one non-empty line, a string or a segment array; `lines` holds at least one line. Commit keys a fill by the patch name; a foreach patch's key is `["<name>/", {"answer": "key"}]`, and amend and resync keep the key a patch already used.
- **Rendering.** Contributions sort by key; the separator is appended to the last line of every contribution but the final one; an empty slot renders no lines. No hunk sees slot content, so fill-only patches commute in any order and `weft check` does not render their pairs.
- **Errors.** Two fills with one key in one slot (``patches `linear` and `twin` both fill slot `servers` of `.mcp.json` under key `linear` ``), an unknown slot, a fill on a missing file, an empty fill, a bad or duplicate slot name.

### Where slots come from

Nobody writes a slot by hand. `weft share PATH --name NAME` (or `weft commit --share NAME`, or answering yes when commit asks) takes a file that two or more independent patches each create and writes a new patch NAME that creates it with the lines they all share around a slot `entries`, `omit_when_empty: ["entries"]` and no gate. Each creator's `create_file` becomes a `fill_slot` keyed by its name, with `depends_on` NAME added. Recorded on a scratch template with `linear` and `jira` each creating `.mcp.json`:

```json
{ "op": "create_file", "path": ".mcp.json", "omit_when_empty": ["entries"], "content": ["{", "  \"mcpServers\": {", { "slot": "entries", "separator": "," }, "  }", "}"], "mode": 420 }
{ "op": "fill_slot", "path": ".mcp.json", "slot": "entries", "key": "linear", "lines": ["    \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }"] }
```

`share` handles only files the patches create. A file one patch creates and others change by hunks (a Gradle `dependencies {}` block in `build.gradle.kts`, a line several siblings add to `README.md`) is refused: ``patch `ci` changes `README.md`; weft shares a file that patches only create``. Those patches stay a chain, or keep their hunks at least three lines apart.

To replace `expr` lines that let several bools add to one file, record one patch per bool that creates the whole file with its own entry, delete the `expr` version, and share the file. On a scratch copy of the house `base` template, replacing the `expr`-line `mcp` with `linear-mcp` (when `use_linear`) and `jira-mcp` (when `use_jira`), each creating `.mcp.json`, `.codex/config.toml` and `.omp/mcp.json`, then `weft share .mcp.json .codex/config.toml .omp/mcp.json --name mcp`, kept Linear only, Jira only and neither byte-identical, and `weft check` passed under all four combinations.

Later fillers are recorded, not hand-written: `weft session new lightdash --base mcp`, type the entry where its key sorts, then `weft commit --name lightdash --depends-on mcp`. Where the owner has `omit_when_empty` and nothing else fills the slot, the worktree has no such file: write it whole, the owner's lines around your entry. `weft diff` shows it as created with the fill note, and commit still records one `fill_slot`; amending the only filler of such a file stays a `fill_slot` too. `weft share` refuses a file a patch already fills (``patch `jira` adds lines to a slot of `.mcp.json`; weft shares a file that patches only create``), so a late creator is re-recorded this way.

`weft patch amend` on the owner shows every filler's lines in the slot, or `⟪slot entries: other patches add their lines here⟫` for an empty slot; edit the lines around them and commit writes the slot back in place. `weft patch resync` still skips a generated patch that declares a slot; `share` never produces one, because it refuses a generated creator until `weft patch detach`.

## What is hashed

Canonical form is compact JSON in the order `depends_on` (sorted), `when`, `foreach` (omitted when absent), `ops`, and the dependency names are replaced by the dependencies' ids. So two patches with the same behaviour and parents have the same id in any template, and metadata can be rewritten freely.

## Portable patches

Verified 2026-09-25: a patch with no `depends_on` and only `create_file` ops at paths no other patch touches can be copied unchanged between templates. It becomes a second root, `weft check` proves it commutes with everything, and it keeps the same id in every template it lives in. The house shared its skills and editor config this way until 2026-09-30, when every stack template moved to `extends = "../base"` (`house-stack.md`); copying is now for templates that share no base. Never copy a patch into a template that already inherits it: names are one namespace across `extends`, and the template stops loading with an error naming the clash.

```sh
cp ~/templates/one/patches/editorconfig.json ~/templates/other/patches/editorconfig.json
weft check ~/templates/other --answer "project_name=Demo Service"
```

Constraints: the patch may reference only answers every host template declares (best: none). To update, re-copy; projects pick the change up on `weft update`.

A portable patch that also needs lines in a shared file such as `.gitignore` creates that file with only its own lines, and each host shares it with the host's `base`: copy the patch in, then `weft share .gitignore --name gitignore`. Verified 2026-09-30 on two scratch hosts whose `base` wrote different `.gitignore` lines: in each, the owner `gitignore` held only the slot (nothing was shared, so no separator), `base` and the portable patch became fillers of it, `weft check` passed, and the rewritten portable patch came out byte-identical in both hosts. It is no longer a root, so a host that has not shared the file needs the original, file-creating version. On weft 0.1.0 there are no slots; give the lines to each host's `base` instead.

## A recorded patch, verbatim

Recorded with `weft session new base --exec 'uv init --bare --name ${package_name} --python 3.13'` and `weft commit --name base --title "uv project" --describe "…" --yes`:

```json
{
  "title": "uv project",
  "description": "Bare uv project pinned to Python 3.13, recorded from uv init so patch resync can regenerate it.",
  "ops": [
    {
      "op": "create_file",
      "path": "pyproject.toml",
      "content": [
        "[project]",
        ["name = \"", { "answer": "package_name" }, "\""],
        "version = \"0.1.0\"",
        "requires-python = \">=3.13\"",
        "dependencies = []"
      ],
      "mode": 420
    }
  ],
  "generator": {
    "command": ["uv init --bare --name ", { "answer": "package_name" }, " --python 3.13"],
    "answers": { "package_name": "demo-service", "project_name": "Demo Service", "use_docker": false }
  }
}
```

Note what abstraction did: `demo-service` became `{"answer": "package_name"}` because `package_name` was a declared (computed) question whose value appeared verbatim. Had the generator been run without `--name`, the file would say `name = "worktree"` and nothing would be abstracted.
