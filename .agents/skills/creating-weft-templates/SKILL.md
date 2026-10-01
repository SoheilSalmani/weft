---
name: creating-weft-templates
description: Builds a new Weft template from nothing. Plans the increments as a list of patches before recording anything, writes weft.toml with only the questions that vary between projects, records the root with the stack's own generator so it can be resynced, records every further increment as a gated sibling in its own session, attaches toolchain hooks, saves presets, generates AGENTS.md and proves the graph with weft check and a real scaffold. Use when asked to create, bootstrap, design or set up a Weft template for a stack (a FastAPI service, a Gradle app, a dbt project, a Next.js site) with no existing project to start from. When a real project already exists and should become the template, use extracting-weft-templates. Load weft-conventions alongside it.
---

# Creating a Weft template

A template built from nothing goes wrong in one of two ways: everything lands in one patch recorded on `latest`, or every choice the author was unsure about becomes a question. Both are avoided by writing the list of patches before opening a session. The rules for what a patch may contain and how things are named live in `weft-conventions`; this is the order of operations.

## Before anything

```sh
weft --version
ls ~/Desktop/Projects/templates        # or wherever this machine keeps templates
```

If a template for the stack already exists, stop and extend it. A second `python-service` next to `fastapi` is a maintenance bill, not a template.

Confirm the generator for the stack is installed (`command -v uv`, `pnpm`, `gradle`), because the root patch is recorded from it.

## Plan the increments on paper

Write the list first, one line per patch, in the form the graph will take:

```text
fastapi    uv project                      --base base      --exec 'uv init --bare --name ${package_name} --python 3.13'
app        FastAPI application             --base fastapi
ruff       Ruff configuration              --base fastapi
pytest     pytest configuration            --base fastapi
ci         GitHub Actions CI               --base fastapi   --when use_ci
docker     Docker image                    --base app       --when use_docker
```

Every house template extends the `base` template, which already ships the README, the `.gitignore`, editor and git configuration, mise, the MCP files, every Agent Skill and the `git-init` and `git-commit` hooks, so none of them is on the list. Names are one namespace across `extends` and `base` is taken, so the root is named after the template and recorded on base's own root, `--base base`.

Rules for the list, from the conventions: nouns for names, noun phrases for titles, a `--when` only for something whose presence varies between projects, `--base` on the root unless the patch anchors on another patch's lines or needs it at runtime (the Dockerfile runs the app, so `docker` sits on `app`). A tool the house has already chosen is not a question and not a gate. `references/increment-catalogue.md` lists the increments that recur per stack and the hooks each one owns; take names and order from there.

Then write the questions the list needs, and only those. Identity first, then the `use_*` toggles the gates mention, then secrets. Every optional question gets a default.

## Set up the template

```sh
weft init ~/Desktop/Projects/templates/fastapi --name fastapi
$EDITOR weft.toml          # extends = "../base" under [template], description, questions; delete the TODO
$EDITOR .weftignore        # .venv/ uv.lock __pycache__/ node_modules/ target/ build/
```

Write `.weftignore` before the first recording: whatever the generator drops into the worktree is otherwise recorded.

Base's questions are asked by every template that extends it. Reword or narrow one for the stack with a `[refine.<id>]` table instead of declaring it again, a new `description` for `project_name` or narrowed `choices`; `[refine.stack_skills]` with `choices = []` unless base has skills for this stack. `weft describe` shows what is inherited.

## Record the root

Let the stack's generator write it, with the project name passed explicitly so nothing is named after the worktree directory:

```sh
cd $(weft session new fastapi --base base --answer "project_name=Demo Service" \
       --exec 'uv init --bare --name ${package_name} --python 3.13')
weft diff --abstracted     # every project-specific literal must show as ⟨answer⟩; grep for "worktree"
weft commit --name fastapi --title "uv project" \
  --describe "Bare uv project pinned to Python 3.13, recorded from uv init so patch resync can regenerate it." --yes
```

Keep the generator patch pure. Anything you would add by hand (the first source file, a README of the stack's own) is the next patch, so `weft patch resync` can regenerate the root from the generator without losing hand edits.

## Record each increment in its own session

```sh
cd $(weft session new ruff --base fastapi --answer "project_name=Demo Service")
# write the real config, real values
weft commit --name ruff --title "Ruff configuration" \
  --describe "Adds ruff as the linter and formatter with a 100-column limit." --yes
weft check --answer "project_name=Demo Service"
```

For a gated increment, record with the gate true and pass `--when`:

```sh
cd $(weft session new docker --base app --answer "project_name=Demo Service" --answer use_docker=true)
weft commit --name docker --title "Docker image" --describe "…" --when use_docker --yes
```

Between commits: `weft check` with the answers, and `weft graph --answer …` to see the shape. If the check names a non-commuting pair, one of the two anchors on the other; re-record it with `--base <that patch>` or move the hunk three lines away.

If one editing sprint touched two concerns, stage them apart: `weft add ruff.toml` then commit, `weft add ".github/**"` then commit. The default after a commit with changes left over is a sibling, which is what you want.

## Hooks, presets, contract

```sh
weft hook add fastapi --id verify-uv --phase pre --effect check --label "Verify uv is installed" --action "command -v uv"
weft hook add app --id uv-sync --phase post --effect setup --label "Lock and sync the Python environment" \
  --action "uv lock && uv sync" --input "glob:pyproject.toml" --after mise-install --before git-commit
weft presets save with-docker --answer use_docker=true
weft describe --agents-md
```

The patch that introduces a tool owns its `verify-<tool>` check; the patch whose manifest a setup step reads owns the step. Base's `git-init` leaves the scaffold as a repository and its `git-commit` makes the first commit; base cannot name its extenders' hooks, so a setup hook whose output belongs in that commit declares `--before git-commit`, and one that needs the pinned toolchain `--after mise-install`.

## Prove it

```sh
weft check --answer "project_name=Demo Service"
weft check --preset with-docker --answer "project_name=Demo Service"
weft new . /tmp/demo --answers-json '{"project_name":"Demo Service","use_docker":true}' --non-interactive
cd /tmp/demo && uv run python -c "import main"      # or gradle build, pnpm build: whatever the stack's own proof is
```

`check` proves the render and commutation under the answers given. Only running the scaffolded project proves the template produces something that works, and only running it with hooks proves the hooks. Do the second once per preset.

## Do not

- Record everything on `latest`. A chain is what you get when nobody passes `--base`.
- Ask which tool or which version. Pin them; `weft patch amend` bumps them for every project.
- Hand-write `patches/*.json` for content a session can record.
- Store a lockfile, a `.venv`, a `node_modules`, or a binary a hook could fetch.
- Leave `description = "TODO: …"` in `weft.toml`, or a patch without a title and description.
- Ship without `weft describe --agents-md`; the AGENTS.md is how the next agent learns the template.

## Before you finish

- The template extends `base`, declares none of base's questions again, and its graph is a star around its root, or every chain link anchors on the link before it.
- Every question is identity, a `use_*` toggle a gate mentions, a secret, or a value with no house default, and every optional one has a default.
- `weft check` passed with the defaults and with every preset.
- A fresh scaffold ran with hooks and the stack's own build or test passed.
- `.weftignore` was written before the first recording and no patch carries a lockfile, cache or the word `worktree`.
- `AGENTS.md` was generated and the template description is a real sentence.
