# The patch file, for the times a hand edit is right

Patches are normally written by `weft commit`. Open this when you need to read one, fix a typo in `added` lines, add an `{"expr"}` segment (commit never emits one), write a fixture, or move a patch between templates. After any hand edit, `weft check` with answers: ids are recomputed from content, so nothing else needs updating, but a broken hunk only shows up at render.

## Contents

- Top level
- Ops
- Segments and hunks
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
{ "op": "create_binary_file", "path": "app/favicon.ico", "data": "<base64>", "mode": 420 }
{ "op": "modify_file", "path": "README.md", "hunks": [ { "context_before": ["Scaffolded by weft."], "removed": [], "added": ["", "Ships with Docker."], "context_after": [] } ] }
{ "op": "delete_file", "path": "old.txt" }
{ "op": "rename_path", "from": "a.txt", "to": "b.txt" }
{ "op": "set_mode", "path": "scripts/run.sh", "mode": 493 }
```

- Paths are tree-relative, `/`-separated, never `..` or absolute. A path may itself be a segment array: `["src/main/java/", {"answer": "package_path"}, "/App.java"]`.
- `mode` is optional: `420` is `0o644`, `493` is `0o755`. Recording writes it on every file op.
- `create_file` errors if the path exists; `modify_file`, `delete_file`, `rename_path`, `set_mode` error if it does not. Weft picks `create_binary_file` for any file whose bytes are not UTF-8; you never mark it by hand, and binary content is never abstracted or hunk-merged.
- Content is an array of lines; a fully literal line is a plain string. Output ends with exactly one newline.

## Segments and hunks

| JSON | Renders |
| --- | --- |
| `"literal"` | as is |
| `{"answer": "id"}` | the answer's value |
| `{"expr": "…"}` | the Starlark result: a string, `True`/`False`, or a decimal int |

A list never renders. Project it: `{"expr": "', '.join(components)"}`.

A hunk's `context_before + removed + context_after` must match at exactly one position. Zero matches means the file diverged; two means the context is ambiguous. Both are clean errors. An empty pattern appends at end of file. Context lines carry abstraction too, which is why a hunk recorded under `Demo Service` still matches a file rendered under `Orders`.

## What is hashed

Canonical form is compact JSON in the order `depends_on` (sorted), `when`, `foreach` (omitted when absent), `ops`, and the dependency names are replaced by the dependencies' ids. So two patches with the same behaviour and parents have the same id in any template, and metadata can be rewritten freely.

## Portable patches

Verified 2026-09-25: a patch with no `depends_on` and only `create_file` ops at paths no other patch touches can be copied unchanged between templates. It becomes a second root, `weft check` proves it commutes with everything, and it keeps the same id in every template it lives in. This is the house mechanism for root-level scaffolding that several templates share (agent skills under `.agents/skills/`, editor config, CI workflows). It predates `extends`, which shares a base inside the engine; the house templates had not moved to it on 2026-09-27.

```sh
cp ~/templates/skills/patches/skills.json ~/templates/java/patches/skills.json
weft check ~/templates/java --answer project_name=x
```

Constraints: the patch may reference only answers every host template declares (best: none). To update, re-copy; projects pick the change up on `weft update`. A portable patch that also needs a hunk in a shared file such as `.gitignore` is no longer portable; give the hunk to the host template's `base` or accept a per-template copy.

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
