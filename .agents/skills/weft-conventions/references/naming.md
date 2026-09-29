# Names, titles and descriptions that went wrong

The table in `SKILL.md` is the rule. This is the catalogue of what people actually type, and the rewrite. Open it when a name feels off and you cannot say why.

## Contents

- Patch names
- Titles
- Descriptions
- Questions and hooks
- Where each string is read

## Patch names

| Written | Problem | Rewrite |
| --- | --- | --- |
| `add-docker` | verb repeats what every patch does; reads wrong in `--base add-docker` | `docker` |
| `initialize-fastapi` | the root of a template is `base`; `initialize` says nothing about what is there | `base` |
| `configure-sbt` | verb-first, and `configure` hides that this swaps the build tool | `sbt` |
| `patch-003` | the auto-name; says nothing | pass `--name` |
| `docker-and-ci` | two concerns | `docker`, `ci` |
| `docker-v2` | a fork of a patch that should have been amended | `weft patch amend docker` |
| `DockerImage`, `docker_image` | not kebab-case | `docker` |
| `postgres`, `sqlite` as unrelated names for two exclusive adapters | the axis is invisible | `db-postgres`, `db-sqlite`, gated on `database == 'postgres'` |
| `shadcn` for a patch recorded with `--exec 'npx shadcn add button'` | one generated patch per component keeps resync and gating honest | `shadcn-button` |
| `feature`, `misc`, `rest` | leftovers committed as one patch | stage by concern and name each |

## Titles

| Written | Problem | Rewrite |
| --- | --- | --- |
| `Add Docker support` | an action; the node persists, the action never happened | `Docker image` |
| `Initialize project` | what is initialised? | `uv project`, `Gradle build (Kotlin DSL)` |
| `Initalize Astro project.` | typo, trailing period | `Astro project` |
| `Configure Snowflake connection` | imperative | `Snowflake connection` |
| `This patch adds the Dockerfile and the dockerignore file` | a sentence, and it narrates ops | `Docker image` |
| `Docker` | too thin to tell an image from a compose file | `Docker image` |
| (none) | `weft patch ls` shows a blank | any of the above |

Keep it to five words. The title is a label; the description carries the sentence.

## Descriptions

A description is read by the next person or agent asking "what does this give me, and why is it like this?" It should say what the patch adds and the decision it embodies, in one or two sentences, ending with a period. It should not list the files (the ops do that) or restate the title.

| Written | Problem | Rewrite |
| --- | --- | --- |
| `Adds a Dockerfile.` | restates the title | `Adds a uv-based Dockerfile that installs from the lockfile and runs the app as a non-root user.` |
| `Initialize a FastAPI project with a simple example containing 3routes.` | imperative, typo, counts routes nobody will keep | `Adds FastAPI with a health endpoint and the README run recipe; routes live in the project, not the template.` |
| `Configure sbt.` | says nothing the name did not | `Replaces the scala-cli entry point with an sbt build, because multi-module projects outgrow scala-cli.` |
| `TODO: what this template scaffolds.` | the `weft init` placeholder, on a template | write the sentence |
| `Creates Dockerfile, .dockerignore, updates README` | file list | as the first rewrite |
| `Docker support for the project when the user wants it` | the gate already says when | as the first rewrite |

Third person present (`Adds`, `Replaces`, `Registers`) reads well in `describe --json` and AGENTS.md, where the description is the line shown for each patch.

## Questions and hooks

| Written | Problem | Rewrite |
| --- | --- | --- |
| `configure_sbt` | a bool that should read as a toggle | `use_sbt` |
| `use_postgres`, `use_sqlite` | two bools for one exclusive choice | `database` as a `choice` |
| `java_version` | a version question; pin it | none, pin `25` in the patch |
| `profile_name` with `prompt = "Project name"` | prompt copied from another question | `Profile name` |
| `Do you use Jira` | a bool without `?` | `Do you use Jira?` |
| hook `format` | verb only; which tool? | `pnpm-format` |
| hook `deploy` | which target? | `deploy-vercel` |
| label `Runs uv sync.` | describes rather than instructs, trailing period | `Lock and sync the Python environment` |

## Where each string is read

| String | Shown by |
| --- | --- |
| patch name | file name, `depends_on`, `--base`, `--when` neighbours, `weft graph`, `weft patch ls`, every CLI argument |
| title | `weft patch ls`, `weft describe --json`, the studio node |
| description | `weft describe --json`, the generated AGENTS.md patch list, the studio inspector |
| question prompt / description / example | the wizard, `describe --json`, AGENTS.md question table, the scaffold command in AGENTS.md |
| hook label | the run log (the command is not echoed when it interpolates), `weft hook ls`, AGENTS.md hooks table |
