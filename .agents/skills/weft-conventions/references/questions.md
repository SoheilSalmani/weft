# Questions, presets and secrets

Open this when writing or changing `weft.toml`, when an expression fails, or when deciding whether something should be asked at all. The rule in `SKILL.md` stands: ask what varies between projects, pin the house stack.

## Contents

- The manifest
- Kinds
- Defaults, gates and the eager-binding rule
- Computed questions
- Multichoice
- Secrets
- Presets
- Wording

## The manifest

```toml
[template]
name = "fastapi"                  # equals the directory name
weft-version = "0.1"
description = "A FastAPI service on uv, with an optional Docker image."

[[question]]
id = "project_name"
kind = "string"
prompt = "Project name"
description = "Human-facing name; drives the README title and the package slug."
example = "Demo Service"

[[question]]
id = "package_name"
kind = "string"
computed = true
default = "project_name.lower().replace(' ', '-')"

[[question]]
id = "use_docker"
kind = "bool"
prompt = "Ship a Dockerfile?"
default = "False"

[[preset]]
name = "with-docker"
file = "presets/with-docker.toml"
```

Questions are processed in declaration order, and a `default` or `when` may only mention earlier questions. `section = "…"` is display grouping for the wizard and the studio; the CLI ignores it.

## Kinds

| kind | value | extra keys |
| --- | --- | --- |
| `string` | text | |
| `bool` | `True`/`False` | |
| `int` | integer | |
| `choice` | one of `choices` | `choices = [...]` |
| `multichoice` | a subset of `choices`, as a list | `choices = [...]` |
| `secret` | a reference, never a value | `source = "env:VAR"`, `"cmd:…"`, or `"prompt"` |

Nothing else: no `validate`, `regex`, `min`, `max`, `required`, `help`. `required` appears only in `weft describe --json`, meaning "no default, not computed, not secret".

**A `choice` value is abstracted wherever it appears**, including prose. Verified 2026-09-25 on the `base` template: recording the Agent Skills with `tracker = 'linear'` turned every lowercase `linear` in the skill text into `{"answer": "tracker"}`, and the Linear skills alone would have needed hundreds of `--keep-literal` flags on every amend. When the alternatives are product or tool names that the recorded files will mention, use one `bool` per alternative (`use_linear`, `use_jira`) instead: bools and ints are never abstracted. Reserve `choice` for values that are exclusive *and* never appear verbatim in content.

## Defaults, gates and the eager-binding rule

`default` and `when` are Starlark source strings, so a string default is quoted twice (`default = "'api'"`), a bool is `"True"`, a list is `"['issues']"`. Answers are globals named by their ids. The standard library only: string methods, arithmetic, comparisons, `if`/`else` expressions, comprehensions. There are no weft builtins.

Every name in an expression is bound before evaluation, including the untaken side of `and`/`or`. So `use_prisma and prisma_driver == 'postgresql'` fails when `prisma_driver` is gated off and has no default. A gated-off question with a default still resolves to that default. The fix is always the same: **give every optional question a safe default** (`''`, `False`, `[]`).

A question with a default is never prompted; only default-less, unanswered, non-secret questions prompt. On `weft update`, new questions with defaults are filled silently and default-less ones prompt or fail under `--non-interactive`, so a question added to a template in the field wants a default.

## Computed questions

`computed = true` requires a `default`, is never prompted, cannot be a secret, and takes part in abstraction: a directory named `demo-service/` becomes `{"answer": "package_name"}/` at commit. Use it for every slug, path and derived flag rather than asking twice.

## Multichoice

The answer is a list. A raw list never renders; project it with an expression (`' '.join(components)`) or gate one patch per option (`--when "'button' in components"`). On the CLI the value is comma-separated (`--answer components=button,card`); in `--answers-json` it is an array.

## Secrets

```toml
[[question]]
id = "stripe_api_key"
kind = "secret"
source = "env:STRIPE_API_KEY"
prompt = "Stripe API key"
when = "use_stripe"
```

A secret value is never written anywhere: not in answers, state, sessions, patches or the hook log. It reaches content only through `{"answer": "stripe_api_key"}`, and every occurrence is abstracted at commit whether you like it or not. Expressions cannot mention a secret. `--answer`, answers files and presets cannot set one. Prefer `env:` so the tool that needs the value reads it itself; `prompt` requires a TTY. Name the id after the tool and the kind: `resend_api_key`, `prisma_database_url`.

## Presets

A preset is a partial answer file that **locks** what it answers: the wizard skips those questions, `--answer` with a different value is an error, and a multichoice can carry `fixed` and `blocked` options, giving `(selection ∪ fixed) − blocked`.

```sh
weft presets save with-docker --answer use_docker=true --non-interactive
weft presets save with-shadcn-ui --answer use_shadcn_ui=true --fix components=button --block components=chart --non-interactive
```

Name them `with-<feature>` for additive locks and `no-<feature>` for the opposite. Run `weft check --preset NAME` for each one you ship, since commutation is only tested under the answers supplied.

## Wording

- id: snake_case. Feature toggles are `use_<concern>`, named after the patch they gate (`use_docker` gates `docker`); a two-way decision is one `choice` (`adapter = duckdb | postgres`), not two bools.
- prompt: a bool asks a question and ends with `?` (`Ship a Dockerfile?`); a string is a noun label (`Project name`, `Base package`).
- description: one sentence on what the answer drives, for the agent reading `describe --json`.
- example: a realistic value in display form (`Demo Service`, `com.example.demo`), which also seeds the scaffold command in AGENTS.md.
