# The increments that recur

Read this while writing the patch list, before the first session. Names and titles follow `weft-conventions`, which also records the house stack the choices below come from (surveyed 2026-09-25); a newer template in the templates directory outranks anything here.

## Contents

- Shape of a healthy template
- Increments common to every stack
- Python (uv)
- Node (pnpm)
- Java (Gradle, Kotlin DSL)
- Scala
- dbt
- Generated roots

## Shape of a healthy template

```text
base (inherited) ── fastapi ── app ── docker (when use_docker)
                      ├── ruff
                      ├── pytest
                      ├── ci (when use_ci)
                      └── renovate (when use_renovate)
```

A star around the root, which sits on base's root, and one chain where the Dockerfile needs the app. Every gated node is a leaf or the head of its own small chain, so switching it off removes exactly its files. Base's own patches (the skills, editor and git configuration, mise, the MCP files) come with `extends`; the template records none of them.

## Increments common to every stack

| name | title | gate | base | owns hooks |
| --- | --- | --- | --- | --- |
| the template's name | `<generator> project` | | `base` | `verify-<tool>` (pre, check) |
| `app` or the stack's first source | `<Framework> application` | | the root | `<tool>-sync` / `<tool>-install` (post, setup, `glob:` on the manifest, `--after mise-install`, `--before git-commit`) |
| `formatter` | `<Tool> formatting` | | the root | `<tool>-format` (post, setup, `--after` the install hook) |
| `linter` | `<Tool> linting` | | the root | |
| `testing` | `<Framework> tests` | | `app` | |
| `ci` | `GitHub Actions CI` | `use_ci` | the root | |
| `renovate` | `Renovate dependency updates` | `use_renovate` | the root | |
| `docker` | `Docker image` | `use_docker` | `app` | |

Base owns `git-init` and `git-commit`; a setup hook whose output belongs in the first commit declares `--before git-commit`. Base's README is a setup recipe; a stack with a README of its own deletes base's and creates it, and puts its ignores at the top of base's `.gitignore` as a hunk.

Gates are for things whose presence varies between projects. Whether the house wants a formatter is not one of them, so `formatter` is ungated.

## Python (uv)

```sh
weft session new fastapi --base base --answer "project_name=Demo Service" \
  --exec 'uv init --bare --name ${package_name} --python 3.13'
```

`package_name` is a computed question: `project_name.lower().replace(' ', '-')`. Then `app` adds the framework (`uv add "fastapi[standard]>=0.115"` inside the session edits `pyproject.toml`; `uv.lock` and `.venv/` are in `.weftignore`), `main.py` with a health endpoint, and the Python ignores as a hunk on base's `.gitignore`. Hooks: `verify-uv` on the root; `uv-sync` with `uv lock && uv sync`, `glob:pyproject.toml`, `--after mise-install` and `--before git-commit` on `app`. Docker uses `ghcr.io/astral-sh/uv:python3.13-bookworm-slim`, `uv sync --frozen --no-dev`, and depends on `app`.

## Node (pnpm)

`base` is the workspace root (`package.json` with `packageManager` pinned, `pnpm-workspace.yaml`, `turbo.json`, `.prettierrc`, `.gitignore`, `README.md`); `web` is the app under `apps/web`; each opt-in package is one gated patch under `packages/<name>` with `use_<tool>`; a generated component patch uses `--exec 'pnpm dlx shadcn@latest add button'` one component per patch. Hooks on `base`: `verify-node`, `git-init`, `pnpm-install` (`glob:**/package.json`, `--after git-init`), `format`, `git-commit`.

A single app without a workspace (`slides`) is flatter: the root, `slides`, is the Vite + React app itself, with the shadcn/ui runtime dependencies (`radix-ui`, `class-variance-authority`, `cn`, `lucide-react`) and the `shadcn` CLI already in `package.json`. Each component is then a generator patch recorded with `--exec 'pnpm install --silent && pnpm exec shadcn add button --yes'`: the CLI version is the one `package.json` pins, and a component whose dependencies are already there leaves `package.json` alone, so the component patches commute as a star on the root. A component that imports another (`dialog` uses `button`, `command` uses `dialog`) is recorded with `--base` on that one, or the CLI writes the shared file into two siblings and they stop commuting. Hooks on the root: `verify-pnpm` and `pnpm-install` (`glob:package.json`, `--after mise-install`, `--before git-commit`); `git-init` and `git-commit` are base's. The features on top of the app form a chain (`deck → code → diagrams → mdx-types → export`) because each adds scripts or devDependencies next to the last one's lines in `package.json`, as `java`'s build patches do; the optional ones (`twoslash`, `github-actions`, `github-pages`) hang off its tail as gated leaves, and the stack's own skill, `writing-slides`, lives in base behind `stack_skills` like `dbt-skills`, fixed on by `[refine.stack_skills]`. `code-files` hangs off `export` too, only because its devDependency lands next to `playwright-core`; `react-flow` needs just `diagrams` and `writing-slides`. `talk-guide`, the how-to guide for people writing a talk, is ungated and hangs off `export` and `writing-slides` as `twoslash` does, since it names `pnpm check` and the skill; its README hunk sits in the quick start, where no optional patch writes. `react-flow` adds the `edges:` frontmatter key as hunks on `deck`'s `src/deck/types.ts` and `context.ts`, so a deck without React Flow has no key it cannot use, and its README row fits the frontmatter table's column widths: a wider cell would make Prettier realign the whole table, and every other patch's anchors in it with it. `twoslash` rewrites the whole line block of `code`'s `CodeLines` and the `CodeLine` interface, so `code` draws a fold's own line as tokens marked `hunk` and `fold-summary` and keeps fold state out of `CodeLine`: a change inside that block, or a field on `CodeLine`, means re-recording `twoslash`. `dbt-lineage` builds on `react-flow`: its question is asked only `when = "use_react_flow"`, and the patch is gated on both answers, so an explicit `--answer use_dbt_lineage=true` without React Flow renders nothing rather than an orphan. Siblings that each add a demo slide need different neighbours: two slides inserted after the same slide share an anchor and stop commuting, which is why the `twoslash` slide closes the code slides and the `flow` slide follows the Mermaid one. The demo slides all go into the tour, `chapters/tour.mdx`, which the scaffolded `slides.mdx` only includes, so `weft update` changes the tour and never the talk a user writes in `slides.mdx`; the tour's snippet fences read `../snippets/`.

## Java (Gradle, Kotlin DSL)

The root, `java`, is the Gradle build with the wrapper committed (`gradlew` recorded with mode 493, the jar as a binary op), a `README.md` in place of base's, the Gradle ignores and line-ending rules as hunks on base's `.gitignore` and `.gitattributes`, and the hello-world source. Then `testing` (JUnit + AssertJ), `formatter` (Spotless + palantir-java-format), `linter` (Error Prone): these three each add lines to the `plugins {}` and `dependencies {}` blocks of `build.gradle.kts`, which `java` creates. `weft share` does not apply, because it shares only a file several patches each create, so they form a legitimate chain or are recorded with hunks three lines apart. Optional: `spring-boot` (`use_spring_boot`, replaces the entry point), `rest-api` on `spring-boot`, `testcontainers` (`use_spring_boot and use_testcontainers`), `github-actions`, `renovate`. Pin the Java version in the build; do not ask it.

## Scala

The root, `scala`, is a scala-cli entry point with `.scalafmt.conf`, a README in place of base's, and `verify-scala` / `verify-scalafmt` pre checks. `sbt` (`use_sbt`) replaces it with a build (`delete_file` of the entry point plus the build files, one patch); `zio` on `sbt` (`use_zio`).

## dbt

The root, `dbt`, is `dbt_project.yml`, `packages.yml`, `pyproject.toml` on uv, `.python-version`, the empty `models/`, `macros/`, `seeds/` with `.gitkeep`, `.vscode/extensions.json`. One connection patch per adapter, gated on a single `adapter` choice, with the DSN as a `secret` question gated on the adapter. `mise.toml` carries `[env]`, `mise.local.toml` is gitignored. Hooks: `verify-dbt`, `dbt-deps` with `glob:packages.yml` and `--before git-commit`.

## Generated roots

When a generator produces the root (`uv init`, `astro dev init`, `pnpm dlx create-next-app`, `gradle init`), pass the project name explicitly and keep the patch pure so `weft patch resync` can regenerate it after a generator upgrade. Everything you would edit by hand goes in the next patch. Interpolate values (`${package_name}`); for structural variation (which components, which adapter) record one gated generated patch per variant rather than interpolating a list.
