---
name: extracting-weft-templates
description: Turns a real, working project into Weft template patches. Decides whether a house template for the stack already exists, adopts the project (or a disposable copy of it) as a session worktree with a scope, declares the questions whose values appear in the files, sorts every file into generic, project-specific, half-and-half or junk, promotes the generic parts one concern per patch, keeps the graph a star with --depends-on base, derives hooks from the project's own setup commands, and diffs a fresh render against the project to show what stayed behind. Use when asked to extract, promote, adopt, templatize or "turn into a template" an existing repository, or to push code written in a scaffolded project back into its template. With nothing to start from, use creating-weft-templates. Load weft-conventions alongside it.
---

# Extracting a Weft template from a project

A project is not a template with the names changed. Most of it is the reason the project exists, and none of that belongs in a patch. The work is triage: decide what the next project should start with, and record only that. `weft session adopt` links a real directory as a session's worktree, and `weft status` in it is a drift report, the difference between what the template renders and what the project became. Everything else is the ordinary loop from `weft-conventions`.

## Which of the two cases

```sh
cat .weft/state.toml 2>/dev/null       # present: weft scaffolded this project
weft --version
ls ~/Desktop/Projects/templates        # is there a template for this stack already?
```

**The project was scaffolded by weft.** Adopt it in place, with no arguments: the state file supplies the template, the pinned base and the answers. `weft status` then lists exactly the files that drifted, and `weft add -p` can take part of a file because the base already contains the rest.

**Weft never saw the project.** Two rules follow. Work on a **disposable copy** (`cp -R proj /tmp/proj-extract && rm -rf /tmp/proj-extract/.git`): against an empty base every file is one new-file hunk that `add -p` cannot split, so the way to promote the generic half of a file is to edit the copy down to that half before staging, and you do not edit a real project into a template. And pass `--scope`, or every file in the directory reads as new.

Either way, if a template for the stack exists, adopt against it and add to it. Two templates for one stack is a maintenance bill.

## Declare the questions first

Abstraction only replaces values of answers the session already knows, so the questions come before the adoption. Read the project for its identity literals: the name in `pyproject.toml`, the package or module path, the organisation in the build file, the display name in the README title. Each becomes a question, and the adoption passes the project's own literal as the answer (`--answer "project_name=Payments"`), which is what makes `Payments` turn into `{"answer": "project_name"}` in every promoted file. A slug that appears (`payments`) is a computed question, not a second string.

Do not declare a question for a tool, a version, or a value that is the same in every project of the house; those are pinned. A credential found in `.env` becomes a `secret` question with an `env:` source; the `.env` file itself is never promoted, an `.env.example` is.

Then seed `.weftignore` from the project's `.gitignore`, plus lockfiles and anything a hook will regenerate.

## Adopt with the full scope

```sh
weft init ~/Desktop/Projects/templates/svc --name svc     # only when no template exists yet
weft session adopt /tmp/proj-extract -n base --template ~/Desktop/Projects/templates/svc \
  --answer "project_name=Payments" --answer use_docker=true --answer use_ci=true \
  --scope pyproject.toml --scope main.py --scope README.md --scope .gitignore \
  --scope Dockerfile --scope '.github/**'
weft status
```

Three things learned the hard way:

- **List every path you intend to promote in the scope now.** A commit that leaves nothing in scope ends the session, and a second adoption starts from the template's new latest. Widen later with `weft session scope --add`, never narrower than the next patch needs.
- **Answer every gate true.** A patch committed with `--when use_docker` while the session has `use_docker=False` is refused when changes remain.
- **Adopted sessions always stack**, and `--sibling` is refused. To keep the graph a star, pass `--depends-on base` on every commit after the first; weft verifies the patch applies with only `base` present and refuses otherwise, which is the right answer when the patch really does anchor on a sibling.

## Triage, one file at a time

| The file | Do |
| --- | --- |
| tool configuration, build manifest, editor config, CI workflow, Dockerfile, `.gitignore` | generic: promote |
| the entry point, a health endpoint, a smoke test | generic once trimmed: edit the copy down to the skeleton, promote |
| domain code, routes, models, migrations, data | the reason the project exists: leave it |
| README | usually half: keep the run and setup sections, drop the product prose |
| `.env`, credentials, tokens | never; a `secret` question and an `.env.example` instead |
| lockfiles, caches, virtualenvs, build output | junk: `.weftignore` |
| Makefile, justfile, `scripts/` | not files to promote but hooks to derive (below) |

`references/triage.md` has the longer catalogue and the tells for each column. When a file's dependency list mixes framework and product (`fastapi` and `stripe`), the trimmed copy keeps the framework line only.

## Promote in pieces

```sh
weft add pyproject.toml main.py README.md .gitignore
weft commit --name base --title "FastAPI service on uv" \
  --describe "Adds the uv project with FastAPI, a health endpoint, ruff and pytest as dev tools, and the README run recipe." --yes
weft add Dockerfile
weft commit --name docker --title "Docker image" --describe "…" --when use_docker --yes
weft status                     # nothing left staged before the next add
weft add '.github/**'
weft commit --name ci --title "GitHub Actions CI" --describe "…" --when use_ci --depends-on base --yes
```

Before each commit, `weft diff --staged --abstracted` shows what will be stored: every project literal must appear as an answer reference, and a coincidental match (`payments` inside an import path) is kept literal with `--keep-literal package_name@main.py:3` using the keys from `weft diff --json`. After a failed commit the staged set is still staged; `weft reset` before staging the next piece.

For a project weft scaffolded, `weft add -p README.md` walks the hunks and `y`/`n` chooses; `s` splits a hunk that has context inside it.

## Derive the hooks

The project's own setup is written down somewhere: `make setup`, the CI workflow, the README's install section. Each step maps to a hook on the patch whose files it reads:

| In the project | Hook |
| --- | --- |
| a tool must exist (`uv`, `pnpm`, `flyctl`) | `verify-<tool>`, pre, check, on the patch that introduces the tool |
| `uv sync`, `pnpm install`, `gradle build` | `<tool>-<verb>`, post, setup, `--input glob:<manifest>` |
| `git init` and a first commit | `git-init`, `git-commit` on `base`, no inputs, chained with `--after` |
| `make deploy`, `flyctl deploy` | `deploy-<target>`, post, deploy, `--when deploy_to_<target>` on an explicit bool defaulting to False |

```sh
weft hook add base --id verify-uv --phase pre --effect check --label "Verify uv is installed" --action "command -v uv" --no-tui
weft hook add base --id uv-sync --phase post --effect setup --label "Lock and sync the Python environment" \
  --action "uv lock && uv sync" --input "glob:pyproject.toml" --no-tui
```

## Finish and prove

```sh
weft session end base --discard          # only if changes remain; an adopted directory is unlinked, never deleted
weft check --answer "project_name=x" --answer use_docker=true --answer use_ci=true
weft graph --answer "project_name=x"
weft new ~/Desktop/Projects/templates/svc /tmp/fresh --answer "project_name=Payments" \
  --answer use_docker=true --skip-tasks --non-interactive
diff -rq /tmp/fresh ~/code/payments --exclude=.weft --exclude=.venv --exclude=uv.lock
weft describe --agents-md
```

The diff is the report: everything listed is what stayed behind, and each line should be product code, a secret, or junk. Anything else is a generic file you forgot. Then scaffold once more with hooks and run the stack's own build.

## Do not

- Adopt a real project without `--scope` and commit the whole worktree as one patch.
- Edit a real project's files down to a template skeleton in place; use the copy.
- Promote `.env`, a lockfile, or a file whose only content is the product.
- Declare a question because a literal happened to differ from the example; declare it because the next project will answer it differently.
- Leave the session open on a directory the user is working in.

## Before you finish

- Every promoted file is something the next project should start with, and the fresh-render diff lists only product code, secrets and junk.
- The graph is a star around `base` unless a patch really anchors on another.
- Every identity literal is an answer reference in the patches; `weft diff --abstracted` was read before each commit.
- Hooks cover what the project's own setup did, on the patches that own the files.
- `weft check` passed under every gate combination the template offers, and the adopted directory is unlinked and untouched.
