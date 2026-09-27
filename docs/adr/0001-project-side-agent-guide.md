---
status: Rejected
date: 2026-08-28
decision-makers: [Soheil Salmani]
---

# ADR-0001: Write the project-side agent guide into the state directory

## Why this was rejected

Rejected on 2026-08-30, before anything was built on it.

The three options below share an assumption that was never examined: that the explanation belongs inside each scaffolded project. Under that assumption the record reached a defensible answer, and then had to state in its own consequences that agent tooling would not load the result. A recommendation whose cost defeats its purpose means the option set was wrong.

The constraint that settles it is that the explanation should have the same form for every agent, contain nothing specific to a project, and be present before an agent reads anything. Per-project content fails all three, so every option here fails, including the one recommended.

This record stays in the log because `AGENTS.md` inside the state directory is the obvious thing to reach for, and the reason not to reach for it is not obvious.

## Context

Weft describes itself in two forms today, and both address someone working on a template. A generated guide can be written into a template directory, covering that template's questions, patches, hooks, and the commands to scaffold it. A Model Context Protocol server exposes the same contract as typed tools. Both require the reader to already know that Weft exists and to be pointed at a template.

A scaffolded project is the other side of that, and it carries no such statement. It holds a state directory recording the template it came from, the pinned base, and the answers. An agent opening that project finds files it did not generate, a directory it does not recognise, and nothing saying that a tool produced them, that the tool can merge a newer template over local work, or that editing the files freely is safe.

Two constraints bound the placement. Agent tooling conventionally loads a file named `AGENTS.md` from a repository root, and does not read directories whose names begin with a dot. The state directory is committed with the project, because updating requires it, so anything placed there travels with the repository and is reachable by search.

A further capability, not built, would render a project's own conventions from its active patches into an agent-facing file. If both that and this one write agent instructions, they need one agreed location, and this decision constrains that choice.

It is not known how reliably an agent discovers a file inside a dot-directory when nothing points at it. No measurement has been made.

## Options considered

### A. The project's state directory

The guide is written to `AGENTS.md` inside the state directory, beside the pinned base and answers it describes. The directory is already owned by Weft and is already excluded from the tree walker, so nothing placed there can enter a patch or a render.

### B. A file at the project root

The guide is written to a file named for the tool at the top of the project. Any agent listing the repository sees it, and so does any person. It adds a file the template did not produce to the root of every scaffolded project.

### C. A managed block inside the project's own agent instructions

The guide is written between markers inside the root `AGENTS.md`, which agent tooling already loads. The file belongs to the template or the user, so Weft would be editing content it does not own, and every regeneration becomes a merge into a file someone else may have restructured.

## Decision

**This is a recommendation, not an accepted decision.**

The system will write the project-side agent guide to `AGENTS.md` inside the project's state directory, generating it during scaffolding and regenerating it during an update, with a command to regenerate it on demand.

Option C is rejected because it makes Weft a writer of a file it does not own, and turns every regeneration into a merge that can fail. Option B is rejected because it places an unrequested file at the root of every project Weft produces, which is a cost paid by every user in exchange for discoverability that has not been measured. Option A confines the guide to territory Weft already owns, where regeneration is an unconditional overwrite and no collision is possible.

## Consequences

The guide cannot collide with template content, user content, or the render, because the state directory is outside the tree Weft reads and writes when applying patches. Regeneration needs no merge, no markers, and no ownership negotiation, so the write is the same operation every time.

Agent tooling will not load the guide automatically. An agent finds it by searching the repository or by being told, which means the guide may go unread by exactly the reader it was written for. This is the cost of the decision and it is not mitigated by anything in this record.

The root `AGENTS.md` of a scaffolded project stays unaware of Weft. The unbuilt capability that would render project conventions from active patches therefore has to write somewhere else, or revisit this decision, rather than extending what this one produces.

The guide is committed with the project, so it appears in the initial commit and in the diff of every update. A reader reviewing an update sees the guide change alongside the code it describes.

Adding a root file that points at the guide remains available later and would not contradict this decision, since it changes discoverability without moving the guide.
