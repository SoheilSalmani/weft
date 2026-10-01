---
name: splitting-weft-templates
description: "Decides whether a Weft template should become two, and does the move by hand because weft has no split command. Covers the three outcomes: a subtree that other templates would mount becomes a child template behind [[include]]; root-level scaffolding several templates share (agent skills, editor config, CI) moves into the base template they all extend; variants of one project stay as gated patches plus presets. Size alone is never a reason. Use when a template feels too big, a second template wants part of it, or a request mentions base templates, extends, includes, reuse across templates, a workspace of parts, or a fleet. For first-time authoring use creating-weft-templates. Load weft-conventions alongside it."
---

# Splitting a Weft template

Weft composes templates in two ways. `extends` imports a base template as it is, keeping its patches' names and ids, and the extender builds on it without changing its patches; the only thing it changes is what it asks, rewording or narrowing inherited questions with `[refine.<id>]`. `[[include]]` mounts a whole child template under a path: the child knows nothing of the parent, and a parent patch may edit files under a single mount only by depending on the child's patch that owns them. Nothing overrides a patch; there is no overlay. So "split" means one of three different things, and the first job is to say which.

## Decide

Ask, in this order:

1. **Is the part a subtree another template would mount, or that one project wants N times?** `apps/api`, `services/{key}`, `packages/ui`, `connectors/{key}`. Yes: a child template plus `[[include]]`, `repeat = true` for the fleet.
2. **Is it root-level scaffolding several templates share?** Agent skills under `.agents/skills/`, `.editorconfig`, a CI workflow, Renovate, a `.gitignore` stanza. Yes: it belongs in the base those templates extend. Every house stack template declares `extends = "../base"`, and base holds the shared scaffolding and every skill, as the house-stack survey in weft-conventions records; what only some stacks want sits behind a gate there, which each stack narrows with `[refine.<id>]`. Templates that share no base can still copy a portable patch between them (`patch-format.md` in weft-conventions).
3. **Is it a variant of the same project?** Postgres or SQLite, Docker or not, Maven or Gradle. Yes: gated patches under one `when` each, presets to name the bundles, and `weft check --preset` per bundle. One template.
4. **Is it just big?** Not a reason. `weft patch squash` for patches that always travel together, `weft patch amend` to trim, titles and descriptions for the rest.

The trigger for the first outcome is external: a second template wants the subtree, or a project wants several of it. Cost side, said before starting: two directories to version, `weft.lock` if the child comes from git or the hub, updates per instance, and glue kept in the parent, as a patch that depends on the child's node when it edits the child's files (a single mount) or a `foreach` patch (a repeated mount).

## The include split, by hand

The graph knows what travels together: `weft graph` and each patch's `depends_on`. The moved set must bring its own dependencies, and the parent must keep every patch that anchors on root files. `references/moving-patches.md` is the exact procedure; the outline:

1. `weft init` the child; declare in its `weft.toml` only the questions the moved ops mention, renamed to the child's vocabulary (`service_name`, not the parent's `project_name`).
2. Move each subtree patch's ops: strip the mount prefix from every `path` (`apps/api/main.py` becomes `main.py`), from `from`/`to` in renames, from hook `inputs` globs and hook `action` `cd` prefixes. Ops that touch root files stay behind as glue in the parent.
3. Give the child its own root: the first moved patch drops its `depends_on`; the rest reference moved names only.
4. In the parent: delete the moved patches, keep the glue patch depending on `base`, add the include.

```toml
[[include]]
name = "api"
template = "../fastapi"
path = "apps/api"

[include.bind]
service_name = "project_name + ' API'"
```

5. `weft check` the child with its own answers, then the parent. Render the parent before and after the split into two directories and `diff -r` them; the only differences should be ones you chose.

Ids change for every moved patch. Projects built from the old parent still update, because they carry their own base; instances of the new include are pinned in `.weft/state.toml` from then on.

## Moving scaffolding into the base

A base's patches keep their names and ids in every template that extends it, so one change in base reaches every template, and `weft update` carries it to every project. Moving a stack's scaffolding there, by hand:

1. Record it in base, or move its patch file into `base/patches/` when it depends on nothing stack-specific. What only some stacks want goes behind a gate on a base question, and each stack narrows that question with `[refine.<id>]` (`choices`, `default`, `fixed`) to what it offers.
2. Delete every stack's copy. Names are one namespace across `extends`: a stack patch named like a base patch stops the stack loading, and `weft patch amend` in a stack refuses an inherited patch, so the base copy is the only one anyone can change.
3. Two roots cannot create the same file. A stack whose own root wrote a `.gitignore` or README turns it into a hunk on base's, or deletes base's and creates its own. The stack's root, renamed after the stack since `base` is taken, depends on base's root.
4. Base's hooks cannot name a stack's. A stack setup hook that must run ahead of a base hook, such as the `git-commit` that makes the first commit, declares `before` on it.
5. `weft check` base, then every template that extends it. Render each stack before and after into two directories and `diff -r` them; the only differences should be ones you chose.

Verified 2026-09-30 on the house templates: a project scaffolded from each stack before the move, with default answers, updated onto it with `weft update` without conflicts and then matched a fresh render.

## Do not

- Split because the file count is high.
- Promise an overlay, or a parent patch that edits a mounted child's files without depending on the child's patch that owns them.
- Move a patch without its dependency closure, or leave a parent patch anchoring on a moved file without depending on the child's patch that owns it.
- Rename answers in the moved ops without declaring them in the child's `weft.toml`.
- Split the variants of one project into sibling templates; that is what gates and presets are for.

## Before you finish

- The outcome was named (include, the shared base, gates and presets, or no change) with the reason.
- For an include: the child checks on its own, the parent checks, and the before/after renders differ only where intended.
- For the shared base: the scaffolding lives in base alone, no stack keeps a copy, and base and every template that extends it pass `weft check`.
- Every moved path, glob and `cd` lost its mount prefix; every renamed answer exists in the child.
- Titles and descriptions were carried over, and both templates' `AGENTS.md` were regenerated.
