# Moving patches into a child template

The exact procedure for the include split, run on 2026-09-25 against `weft 0.1.0` on a parent with `base` (root files) and `api` (files under `apps/api/` plus a README hunk). Read the patch JSON rules in `weft-conventions` (`references/patch-format.md`) first if any of the fields below are unfamiliar.

## Contents

- Choose the set
- Create the child
- Rewrite and move the ops
- Wire the parent
- Check both, diff the renders
- Repeated includes and foreach glue
- What was observed

## Choose the set

```sh
weft graph --answer "project_name=Acme Shop"
python3 -c "import json,glob; [print(p, json.load(open(p)).get('depends_on', [])) for p in sorted(glob.glob('patches/*.json'))]"
```

The set that moves is every patch whose ops live under the mount prefix, plus any patch they depend on that also lives there. A patch with ops on both sides is split by hand into the part that moves and the glue that stays. A patch outside the set that depends on one inside it must be examined: if it anchors on moved lines it moves too, otherwise its `depends_on` is rewritten to the nearest parent patch that remains.

## Create the child

```sh
weft init ~/Desktop/Projects/templates/fastapi --name fastapi
```

In the child's `weft.toml`, declare only the questions the moved ops and hooks mention, in the child's own vocabulary. A parent `project_name` that the child uses for a service title becomes `service_name`; a computed slug is recomputed from it. The parent will bind them.

## Rewrite and move the ops

For every moved op, strip the mount prefix from `path`, from `from` and `to` on renames, from every `glob:` in hook `inputs`, and from a leading `cd <mount> &&` in hook `action`. Rename answer references to the child's ids. This is a mechanical transform; do it with a script, not by hand, and keep the glue ops for the parent:

```python
import json
src = json.load(open('patches/api.json'))
mount = 'apps/api/'
child = {"title": src["title"], "description": src["description"], "ops": [], "hooks": []}
glue = {"title": "API registration", "description": "Registers the mounted API service in the root README.",
        "depends_on": ["base"], "ops": []}
for op in src["ops"]:
    path = op.get("path") or op.get("from")
    (child if isinstance(path, str) and path.startswith(mount) else glue)["ops"].append(
        {**op, **({"path": op["path"][len(mount):]} if isinstance(op.get("path"), str) and op["path"].startswith(mount) else {})})
for h in src.get("hooks", []):
    child["hooks"].append({**h, "action": h["action"].replace(f"cd {mount.rstrip('/')} && ", ""),
                           "inputs": [i.replace(mount, "") for i in h.get("inputs", [])]})
text = json.dumps(child).replace('"project_name"', '"service_name"')
json.dump(json.loads(text), open('../fastapi/patches/base.json', 'w'), indent=2)
json.dump(glue, open('patches/api.json', 'w'), indent=2)
```

A segment path (`["apps/api/", {"answer": "x"}, ".py"]`) needs the prefix removed from its first literal; the script above only handles string paths, so extend it or fix those by hand. The first moved patch drops its `depends_on` and becomes the child's root; later moved patches keep names that now resolve inside the child.

## Wire the parent

Delete the moved files from `patches/`, keep the glue patch with `depends_on: ["base"]`, and add the include:

```toml
[[include]]
name = "api"
template = "../fastapi"
path = "apps/api"

[include.bind]
service_name = "project_name + ' API'"
```

`bind` runs first with the parent's answers in scope; explicit `--answer api.service_name=…` overrides it; whatever is left resolves through the child's own defaults and prompts. For a hub-published child, `template = "hub:owner/fastapi"` with `version = "^1.0"`, then `weft lock`.

## Check both, diff the renders

```sh
weft check ../fastapi --answer "service_name=Acme Shop API"
weft check . --answer "project_name=Acme Shop"
weft new . /tmp/after --answer "project_name=Acme Shop" --skip-tasks --non-interactive
diff -r /tmp/before /tmp/after --exclude=.weft          # /tmp/before rendered from the parent before the split
```

Render `/tmp/before` before touching anything. The diff should be empty or show only the changes you chose (in the run this was written from, the one difference was a slug suffix the rewrite had doubled, fixed in the child's default). Then `weft describe --agents-md` in both templates.

## Repeated includes and foreach glue

For a fleet (`path = "connectors/{key}"`, `repeat = true`), the glue that touches root files once per instance is a `foreach` patch, recorded in the parent with a sample instance mounted:

```sh
weft session new registry --template . --foreach connector=stripe --answer "project_name=Acme"
# edit the root file against the rendered connectors/stripe/, then:
weft commit --name registry --title "Connector registration" --describe "Registers every connector instance in the workspace manifest." --yes
```

Commit abstracts the sample key into `{"answer": "key"}`. A `foreach` patch must be a leaf; nothing may depend on it. Instances belong to projects (`weft new … --instance connector=github`, `weft instance add`), never to the template.

## What was observed

- `weft check` on the parent reports the parent's own files (`render ok (2 files)` for a parent whose full render has four); the child's files are proven by checking the child. `weft new` renders both.
- The parent's `weft describe --json` listed `includes` with the child's full question schema and binds, but its `hooks` array was empty although the child carries a post hook; describe the child to see its hooks.
- Documented rather than run here: a moved mount behaves as delete plus create on `weft update`; `weft session new` on a composed parent records parent-only files, and an edit under a mount is rejected at commit.
