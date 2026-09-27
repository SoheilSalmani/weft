---
date: 2026-08-28
record-type: Baseline
---

# ADR-0000: Architecture Baseline

## What this record is

**This is not a decision record.** It describes the architecturally significant state of the system on 2026-08-28, the date this log was adopted, and it records no decision, no rationale, and no recommendation.

It may be cited as evidence of what the architecture was on that date. It may not be cited as evidence that any of it was decided, endorsed, or recommended. It is written once and is not updated as the system changes, so later records will move away from what is described here while this document goes on describing the starting point.

## What the system does

Weft is a project scaffolding tool. It produces a new project directory from a template, and it keeps that project able to take later template changes without losing the work done since it was created.

Its users are the author of a template and the developer who scaffolds from one, often the same person. A template author edits real, rendered files in a worktree and Weft captures the change as a patch. A developer scaffolds a project by answering the template's questions, and returns later to merge a newer version of the template into the project.

Templates contain no template language. There is no markup inside the files a template carries, so every file in a template is a file a project can run. What varies between projects is stored as recorded operations against answers, not as placeholders in the text.

The capabilities the tool offers, and the parts that carry each:

- **Scaffolding a project** from a template and a set of answers, carried by the engine's render and state pinning over the core's pure renderer.
- **Recording a template change** by editing files in a session worktree and committing the difference as a patch, carried by the engine's session, staging, and commit modules with value abstraction.
- **Updating a scaffolded project** to a newer template by three-way merging the new render over local edits, carried by the engine's update module over the core's merge.
- **Validating a template**, including proving that patches declared independent commute, carried by the engine's check module.
- **Composing templates** out of other templates, mounted at a path and optionally repeated as a fleet of instances, carried by the engine's compose and instance modules.
- **Editing recorded history**, meaning amending a patch, squashing several into one, and resyncing patches whose content comes from a generator command.
- **Publishing and consuming templates** by owner, name, and version through a registry, carried by the command-line crate's registry client and a lockfile.
- **Serving the tool to other programs**, through a machine-readable contract, a Model Context Protocol server, and a language server for template authoring.

## How it is built

The system is one Rust workspace producing a single binary. The separation between its crates is by purity rather than by feature: one crate holds the data model and computation, one holds expression evaluation, one holds everything that touches the outside world, and one holds the interface.

### Composition

Four library and binary crates, plus an end-to-end test package, with dependencies pointing only downward:

- **weft-core** holds the data model, the patch algebra, rendering, and three-way merge. It is deliberately free of input and output: no filesystem access, no prompting, no process spawning. It forbids unsafe code.
- **weft-lang** evaluates Starlark expressions used for conditions, defaults, and derived values. It depends only on the core.
- **weft-engine** orchestrates everything: templates, sessions, staging, commit, update, check, composition, hooks, secrets, and state. All policy and all input and output live here.
- **weft-cli** is the `weft` binary. It owns argument parsing, interactive forms, the terminal user interface, the registry client, the language server, and the Model Context Protocol server.
- An **end-to-end test package** is a further workspace member, and it drives the built binary rather than the libraries.

The process boundary is the single binary. The language server and the protocol server are subcommands of it, not separate services.

### Technology

The implementation rests on a Rust toolchain and a small set of libraries, named here where a different major version would imply a different architecture:

- **Rust, 2021 edition**, with a release profile using link-time optimisation and symbol stripping.
- **Starlark 0.14** as the expression language for conditions, defaults, and derived values. Shell exists only inside declared hook actions, never in the manifest.
- **BLAKE3** for patch identity, and **SHA-256** for registry artefact verification. The two are separate mechanisms and are not interchangeable.
- **rmcp 2** for the Model Context Protocol server, over standard input and output, with **tokio** as its runtime.
- **tower-lsp** for the language server, also over standard input and output.
- **ratatui** for the interactive completion forms, and **dialoguer** for simple prompts.
- **clap** for the command line, **serde** with JSON and TOML for serialisation, **similar** for diffing and merging, and **ureq** with **tar** and **flate2** for registry transport.

### Persistence

Weft has no database. Everything it stores is a file, in one of three places, with different ownership and different lifetimes.

A template directory holds its manifest, its presets, and its patches, one JSON file per patch, with dependencies referenced by patch name. Patch identity is a BLAKE3 hash of the canonical body, so patches form a Merkle graph. Metadata such as a patch's title, description, and tags sits outside the hashed bytes, which means documenting a patch does not change its identity. Hooks are likewise outside the hash.

A scaffolded project holds its own state directory. That directory records the template reference, the pinned base, the answers, and one entry per include instance. Answers are stored as concrete values with one exception: secrets are stored as references to their source and never as values. Beside the state, the project stores the patch bodies its tree was last rendered from, which is what allows a merge base to be reconstructed after the template's own history has been rewritten.

A template being edited holds session worktrees, each a named directory with its own staging index. An older single-session layout is recognised and migrated to the current one on first use.

### Deployment

There is no deployed service. The tool is a binary a user installs.

Releases build the binary for four targets, x86_64 and aarch64 on both Linux and macOS, and publish each as a compressed archive with a checksum. A shell installer resolves the target from the running machine and fetches the archive. Building from source is the path that works today; prebuilt binaries, crates.io publication, and Homebrew are prepared but await a first tagged release.

Continuous integration runs formatting, linting with warnings denied, and the full test suite on Linux for every push and pull request. The same gate on macOS is available on demand rather than by default. Snapshot tests are configured to fail rather than rewrite themselves in that environment.

## How it works

Two operations are the inverse of each other and account for what the tool does. Rendering turns a base, a set of answers, and a graph of patches into a tree of files. Recording turns a tree a person edited back into a patch. The mechanisms below are built around that pair.

### Rendering

Rendering is deterministic. The same base, answers, and patches produce a byte-identical tree, and the update mechanism depends on that property. Patches apply in dependency order with ties broken by patch identity, so the order never depends on how a directory happened to be listed.

A patch is a list of operations against paths and lines, where the varying parts are stored as references to answers or as expressions rather than as literal text. A patch may carry a gate, which is a Starlark condition deciding whether the patch applies at all.

File content is either text or bytes. The variant is chosen when content is read, so a rendered tree and the same tree read back always agree, and a binary file is legitimate template content rather than something to exclude.

### Patch composition and commutation

Patches form a dependency graph, not a sequence. Two patches that declare no dependency on each other are asserted to be independent, and validation proves that assertion by applying every such pair in both orders and comparing the results. A patch whose content cannot honour a claim of independence fails at the moment it is committed.

A patch identity is a hash of content, so changing one patch changes every identity beneath it. Editing recorded history is bounded by that property.

### Recording sessions

A session materialises a pinned base into a worktree, which is a directory the author edits. Several sessions can exist at once, each with its own staging index, in the manner of git worktrees. A dirty working directory elsewhere cannot leak into a recording, because the worktree is rendered from the pinned base rather than copied.

Committing derives a patch from the difference between the base and the staged tree, and proposes which literal values in the change correspond to which answers. After a commit the session stays open, and the next patch either stacks on the last one or becomes an independent sibling.

A directory that already exists can be linked as a session worktree instead of one being rendered, which is how code written in a real project is promoted back into the template it came from.

### Updating a project

An update reconstructs the tree the project was originally rendered from, using the patch bodies the project stored rather than the template's current patches, then renders the new template and three-way merges the result over the local working tree. Overlapping edits become conflict markers and a non-zero exit. An update never silently overwrites local work.

### Composition

A template can mount another template at a path prefix. A mount declared repeatable becomes a fleet keyed by instance, where instances live in the project's state rather than in the template, so adding one grows the fleet and a single update updates all of it. Answers can be seeded from parent to child by binding. Nesting is supported to three levels.

### Hooks

A side effect belongs to the patch that implies it rather than to the template as a whole, so it runs only when its patch is active. Each hook declares a phase, before or after rendering, and an effect that classifies its blast radius as read-only, idempotent local setup, or external and irreversible. Hooks are ordered by their own dependency graph. Pre-render hooks run before any file is written, so a failure leaves nothing behind. Hooks are executed through the system shell.

## What it connects to

Weft reaches four things outside itself, and only the first involves a network.

A **template registry** is reached over HTTP, with its base address supplied by a flag or an environment variable. Templates are addressed by owner, name, and version. Versions are immutable, which is what allows the client to keep a verified local cache in the user's home directory and work offline for anything already pinned. Downloads are verified against the checksum published in the registry index and refused on mismatch. A lockfile pins every child template by checksum. The registry server itself is not part of this repository.

**Editor tooling** connects to the language server subcommand. The repository carries a Neovim configuration and a VS Code extension, both thin clients over that server, alongside JSON Schemas for the manifest and patch formats.

**Agents and other programs** connect to the Model Context Protocol server subcommand, which exposes listing, describing, scaffolding, and checking, plus the session and commit loop, as typed tools over standard input and output.

The **system shell** is invoked for three purposes: running hooks, resolving a secret declared as a command, and running the stored generator command of a patch whose content comes from a tool rather than a keyboard.

## Security boundaries

There is no authentication, no authorization, and no tenancy. Weft is a local tool run by one user with that user's privileges, and it has no notion of an account beyond an owner name on a registry, which is a token in a configuration file.

The trust boundary that does exist is around executing template content. A template can run arbitrary shell through its hooks, and installing a template from a registry therefore means accepting whatever shell it carries. Three mechanisms bound this. Hook effects classify each action, so a caller can run the read-only ones and stop at the irreversible ones. The contract emitted for agents lists every hook a template will run, in execution order, before anything is scaffolded. The protocol server refuses to run template tasks at all during scaffolding.

Secrets are references, never values. A question marked secret stores only its source, an environment variable, a command, or an interactive prompt, and resolves at render time. The secret value type refuses to serialise by construction, so a resolved secret cannot reach a state file, a patch, or the emitted contract even by mistake. Values derived from lists containing a secret are excluded from serialisation on the same basis.

## What is incomplete

Several capabilities exist in a partial state, and later records may need to resolve them:

- A project pinned to a published template cannot follow patch identities that the template rewrote through amend, squash, or resync. The self-contained base makes the project survive such a rewrite, but identity migration itself is unbuilt, and this is the constraint on using history editing with published templates.
- Splitting a patch, re-parenting one, and a guided interactive rebase are named as intended and are not implemented.
- Nested include instances are pinned one level deep. State for instances below that level is not recorded.
- Stray-key validation in the manifest is not performed at deserialisation, because the format's flattened question kind prevents it, and is deferred to the validation command.
- The manifest and patch JSON Schemas exist in two copies, one set at the repository root and one inside the VS Code extension, and the two have diverged: the root copies carry fields the extension copies do not.
- Prebuilt binaries, crates.io publication, and Homebrew installation are prepared in the release workflow and installer but are not yet available, because no version has been tagged.

## Where rationale was recorded

Reasoning for the architecture described above was recorded in two documents in the repository, and this record does not restate or endorse either.

The plan document holds the design as it stood before implementation, including its stated non-goals. The progress log holds running notes per milestone and is where deviations from the plan were captured as they happened, including several explicitly labelled as decisions, covering the content model, the patch file format, value abstraction, and per-occurrence choices.

Beyond those two documents, the reasoning behind the current structure is not recorded anywhere in the repository, and this record does not reconstruct it.
