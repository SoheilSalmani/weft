# Triage: what leaves the project and what stays

Open this when a file does not sort itself. The question for every file is the same: **would the next project of this kind want to start with this, unchanged except for answers?** Yes is generic. No is product. Partly is trim-then-promote.

## Contents

- Tells
- Generic by default
- Product by default
- Trim then promote
- Questions worth declaring
- From scripts to hooks
- The coverage diff

## Tells

| Tell | Column |
| --- | --- |
| the file would be identical in a sibling repository once the name changed | generic |
| the file names a customer, a domain object, a business rule, a price | product |
| the file is listed in the project's `.gitignore` | junk |
| the file is produced by a command in the setup steps | junk, and the command is a hook |
| the file holds a credential, even a test one | never; a `secret` question |
| the file is the tool's own default output, untouched | generic, and usually the generator's `--exec` output |
| the file is a tool default with three house edits | generic; the edits are the point |
| half the sections are about running it, half about what it does | trim |

## Generic by default

`pyproject.toml` trimmed to framework and dev tools, `package.json` and workspace manifests, `build.gradle.kts`, `Cargo.toml`, tool configuration (`ruff.toml`, `.prettierrc`, `.scalafmt.conf`, `.editorconfig`), `.gitignore`, `.python-version` and other pins, `Dockerfile` and `.dockerignore`, `.github/workflows/*`, `renovate.json`, `.vscode/extensions.json`, `mise.toml` with placeholders (never `mise.local.toml`), `.agents/skills/**` (as the portable skills patch, if the house has one), the entry point with a health endpoint, one smoke test that proves the toolchain.

## Product by default

Routes and handlers beyond health, models, schemas, migrations, seed data, domain modules, the tests of those, fixtures and recorded cassettes, notebooks, generated clients, any file whose name is the product's vocabulary. The `src/<package>/` tree is product even when the package name is an answer; the template ships the package directory with an `__init__.py` at most.

## Trim then promote

Edit the disposable copy to the skeleton, then stage:

- `pyproject.toml`: keep `[project]` metadata, the framework dependency, the dev group, tool tables. Drop product dependencies (`stripe`, a client SDK).
- the entry point: keep app construction and the health route; drop imports of product modules and their routes.
- `README.md`: keep the title, the run and setup recipes, the environment variables section; drop the product description and the paragraph about who to ask.
- CI: keep the steps that run on every project (sync, lint, test); drop deploy jobs that name an environment, or make them a gated patch with a `deploy` hook.
- `Dockerfile`: keep as is unless it copies product-only paths.

For a project weft scaffolded, the base already holds the skeleton, so `weft add -p FILE` picks hunks instead: `y` take, `n` leave, `s` split where there is context between two edits.

## Questions worth declaring

| Literal in the project | Question |
| --- | --- |
| the project's name, display name | `project_name` (string, with `example`) |
| its slug, package name, module path | `package_name` / `package_path`, `computed` from the name |
| organisation, group id, reverse-DNS prefix | `organization` (string) when the house has no single value |
| a feature that is present here and would not be everywhere (Docker, CI, an ORM) | `use_<concern>` bool, default matching the house preference |
| one of several exclusive backends (`duckdb` / `postgres`) | one `choice` |
| a token, DSN, API key | `kind = "secret"`, `source = "env:VAR"` |
| a tool or version | nothing; pinned |
| a value that is the same in every project | nothing; literal |

Pass each declared answer at adoption time with the project's own literal, so abstraction has something to match. Values under two characters are never matched; an identity value that is a common word (`app`, `api`) will produce coincidental candidates that need `--keep-literal`, so prefer the distinctive literal the project actually uses.

## From scripts to hooks

Read `Makefile`, `justfile`, `package.json` scripts, `scripts/`, the CI workflow and the README's setup section, and map:

| Found | Hook |
| --- | --- |
| a `command -v`, a `which`, a README line "install X first" | `verify-<x>` pre / check on the patch that first needs X |
| `uv sync`, `pnpm install`, `gradle build`, `dbt deps`, code generation | `<tool>-<verb>` post / setup with `--input glob:<the manifest it reads>` |
| `pre-commit install`, `husky`, hooks installation | post / setup with no inputs (once) |
| a format target | `<tool>-format` post / setup, `--after` the install hook |
| `git init`, first commit | `git-init`, `git-commit` on `base`, no inputs, `--after` the setup hooks |
| deploy, publish, release | post / deploy, `--when deploy_to_<target>`, described as external and irreversible |
| a target that runs the product (`make run`, `make test` on product tests) | not a hook; leave it |

The Makefile itself is not promoted unless the house wants Makefiles; the hooks carry its setup half.

## The coverage diff

```sh
weft new TEMPLATE /tmp/fresh --answer "project_name=<the project's name>" [--answer use_x=true ...] --skip-tasks --non-interactive
diff -rq /tmp/fresh ~/code/project --exclude=.weft --exclude=.venv --exclude=node_modules --exclude=uv.lock
```

Read every line. `Only in project` should be product, secrets or junk. `Files differ` on a promoted file means a trim or an abstraction is off; open it with `diff` and fix the patch (`weft patch amend`) or the question. `Only in fresh` means the template adds something the project never had, which is fine when intended.
