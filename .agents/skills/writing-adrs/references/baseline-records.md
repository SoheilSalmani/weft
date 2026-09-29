# Baseline records

## What ADR-0000 is

`ADR-0000: Architecture Baseline` is a special record written once, when a decision log is adopted for a system that already exists. Use that exact title. It is generic on purpose, so a reader meeting it in any project knows immediately what it is.

It is **not an architectural decision**. It freezes the architecturally significant state of the existing system at the moment the log begins, so that later records have a clear historical starting point.

**It may be cited as evidence of baseline state. It may never be cited as evidence that a decision, rationale, or recommendation was made.** A reader who finds a design described here has learned that the design existed on that date, and nothing else. Not that it was chosen deliberately, not that it was endorsed, and not that anyone recommended keeping it.

It is written once and is never updated to reflect later changes.

Normal records begin at `ADR-0001`. Only ADR-0000 carries these semantics.

## Why the case exists

A log adopted for an existing system has no starting point. Every record refers to a structure that predates the log, and nothing in the log says what that structure was. Later records supersede parts of it without anything recording what was there first.

An ordinary decision record cannot fill the gap. The reasoning behind an existing system is usually not written down anywhere. A record reconstructing it would be invention presented as history, and once written it cannot be told apart from evidence.

A record that only describes structure is not a decision record either. It decides nothing, so it has no consequences, and consequences are where a decision record earns its keep.

The resolution is to stop calling it a decision.

## Depth

A baseline is **substantially more descriptive than a normal record**, and this is the one place that is correct.

The goal is not a high-level summary. It is to give a future reader enough technical context to understand the starting architecture without reconstructing it from the repository. Someone who joins in two years and needs to interpret ADR-0007 should be able to read the baseline and know what ADR-0007 was talking about.

The limit is architectural significance, not length. Do not produce an exhaustive repository inventory. Include what establishes architectural context or is likely to matter when interpreting later records, and leave out implementation trivia that has no architectural consequence.

The test for a given detail: **would a later record need this to make sense?** If a property could change without any record noticing or caring, it is trivia.

## What to inspect

This is a checklist for what to establish, not an outline for the document. The two are different: a checklist decides what you go and find out, and an outline decides how a reader is walked through it. Using the categories as section headings produces a document with a heading every dozen lines and no order a reader can follow. The section structure is fixed separately, further down.

Work through these and record what applies. Not every project has all of them, and a category with nothing in it is worth saying so where its absence is architecturally significant.

1. **Purpose and capabilities.** What the system is for, who uses it, and what it can do. Then a mapping from each capability to the components that implement it. A reader who does not know what the system does cannot interpret any of the categories below. The mapping is what turns a list of components into an explanation of how the thing works. Do not list a capability the system does not actually perform, even when its own documentation claims it.
2. **Composition.** Applications, packages, and modules. Important dependency directions. Runtime and process boundaries.
3. **Adopted technology.** Frameworks, libraries, language runtimes, databases, protocols, infrastructure components, model providers, and SDKs.
4. **Persistence.** Storage technologies and the data semantics that matter: what is mutable, what is append-only, what cascades, what is versioned.
5. **Interfaces.** Public and internal interfaces, protocols, event mechanisms, and integration boundaries.
6. **Execution.** Important execution paths and how work is orchestrated.
7. **External systems.** What the system talks to, and how.
8. **Deployment and operation.** Runtime assumptions and known operational constraints.
9. **Security boundaries.** Authentication, authorization, tenancy, secrets, and trust boundaries.
10. **Architectural mechanisms.** Adapters, registries, queues, sandboxes, schema validation, caching, plugin systems, and anything comparable.
11. **Present but unused.** Subsystems that exist and are currently unused, bypassed, incomplete, or disconnected.
12. **Inconsistencies.** Known contradictions or duplicated architectural mechanisms that later records may need to resolve.
13. **Documented rationale.** Reasoning for inherited choices that was actually written down somewhere, kept clearly apart from inference.

## Be concrete

Name the technology, not the category:

| Write | Not |
| --- | --- |
| `SQLite through libSQL, single file, WAL journaling` | "a database" |
| `server-sent events over a ReadableStream` | "streaming" |
| `Next.js App Router, React 19` | "a web framework" |
| `AES-256-GCM at rest, key from the environment` | "encrypted" |
| `Zod schemas validate every structured model response` | "input is validated" |

Vague description is the main way a baseline fails. A reader cannot act on "a message queue"; they can act on the name and version of the one you used.

## Versions

Include a version only when **a different major version would imply a different architecture**. Where it would not, the name alone is enough.

This rule is easy to state and easy to ignore, so apply the test to each one:

| Keep | Why |
| --- | --- |
| `Next.js 16, App Router` | The major decides the routing and rendering model |
| `React 19` | The major decides the component and concurrency model |
| `Vercel AI SDK v7` | The major redesigned the interface a later record would migrate |
| `Node 22 or later` | A runtime floor that constrains what can be used |

| Drop | Why |
| --- | --- |
| `JSZip 3.10` | Nothing architectural turns on the version |
| `Drizzle ORM 0.45.2` | The choice matters, the patch number does not |
| `tokenlens 1.3` | A utility with no architectural consequence |

**Never record a patch version.** A record listing patch numbers has become a dependency manifest, which is generated elsewhere, is correct there, and is wrong here the moment anything is upgraded.

## Interfaces, and how deep to go

A baseline names the interfaces a system exposes, with enough for a reader to tell what each is for. For an HTTP surface that means the route, its methods, and one line of purpose.

Stop there. Request and response schemas, status codes, parameters, and error behaviour are reference material. They are the fastest-rotting content a system has, and a frozen record is the worst place to keep them.

The test: could a reader tell **what the system offers**, without being able to call it correctly from the record alone? That is the right depth.

## Rationale

Where the repository shows what exists but not why, **state the architecture as fact and leave the rationale unknown.** Say so explicitly rather than staying silent, so a later reader knows the gap was checked rather than overlooked.

Where reasoning was genuinely recorded somewhere, note that it exists and where, without restating it as a decision. A later record can pick it up properly and decide it.

Never blur the two. Documented rationale and inference must be separable by a reader who was not there.

## What to leave out

Leave out anything answering why, beyond noting where recorded rationale exists.

Leave out file paths, line numbers, function names, and code. Naming a technology is required; naming the file it is configured in is not, and paths date faster than anything else in the document.

Leave out assessment, whether praise or criticism. A baseline that grades the system is arguing rather than describing. Record what is missing or unfinished plainly, as fact.

**Leave out the document talking about itself.** A record states facts about the system. It does not explain why its own content was selected, does not refer to its own sections, and does not predict what later records will do. `Seven mechanisms recur, since later records are likely to touch them` says one thing about the system and two about the author; only the first belongs.

The selection criterion is guidance for whoever writes the record, and it lives here in the skill. What reaches the reader is the fact that survived it.

## Tables, lists, and prose

A baseline enumerates far more than a decision record does, so the choice of shape comes up constantly. It is not a matter of taste.

The choice comes down to what the items are:

- **A table**, when every item shares the same attributes and the reader gains from comparing across them. The value of a table is the column: a reader scans down one attribute across all items and learns something. Eight agents each with a role, a model, and a trigger condition is a table.
- **A list**, when items are parallel but do not decompose into shared columns. Each needs a sentence or two of its own and there is no second attribute to line up. Operational constraints, data semantics, and unused subsystems are lists.
- **Prose**, when the items are not parallel at all, when one leads to the next, or when the relationship between them is the thing worth saying. Reaching for a list here loses the connective reasoning, which is usually the part that mattered.

Three failure signs:

- **A two-column table whose second column holds a paragraph.** That is a list wearing a table's clothes. The header earns nothing and the row heights make it unreadable.
- **Cells of wildly uneven length.** Nothing lines up, so nothing can be scanned, which was the only reason to use a table.
- **A list of bare noun phrases.** Either it is a table missing its columns, or it is prose missing its verbs. Decide which.

Keep full sentences inside list items. A list is a visual aid, not permission to drop the reasoning.

Prefer a list over a table when the content does not need comparison. Tables are harder to read on a narrow screen, harder to hear, and harder to edit later. A table whose cells run to a sentence each will also render wide; a list with one entry per line gives the same scannability without that cost.

**A short enumeration of bare names stays inline.** Six build tasks or four status values are a sentence, not six and four lines. Promote an enumeration only when each item earns a description. If the items deserve nothing more than their own names, a list of single words is worse than the sentence it replaced.

The threshold is what a reader does with it. Names they will cross-reference against code, such as a stage vocabulary or a route set, are looked up rather than read, and looking up needs one entry per line.

**Introduce every structure.** A table or list is preceded by a sentence saying what it enumerates, ending in a colon. A section never consists of a heading and a structure alone, because a reader meeting a table cold has to infer what it compares before they can read it.

**Never open a section with a subsection.** A heading followed immediately by another heading gives the reader nothing to confirm they are in the right place, and someone navigating by heading lands on an empty stop.

The text in between must state a fact about the subject that frames what follows: what unites the subsections, or what the reader should carry into them. Do not restate the subheadings, and do not describe the document's own arrangement. `This section covers composition, technology, and persistence` is the table of contents written twice.

If the only sentence available restates the subheadings, that is a finding rather than a formatting problem. It means the grouping is arbitrary, and the fix is to change the grouping.

The document title is not a section, so it may be followed directly by the first heading. The rule applies from the first real section down.

## Paragraphs

Aim for two to four paragraphs in a section, each carrying one idea and following from the one before. Sentences within a paragraph should connect; if they could be reordered without loss, they are a list that has not admitted it yet.

The failure to watch for is a run of bolded one-paragraph blocks. It looks organised and reads badly: the bold acts as a heading it is not, each block sits alone, and the reader gets a sequence of assertions with nothing joining them. A section built that way is one of two things, and you have to decide which:

- **Parallel items**, in which case write a list with a lead-in and let the markers do the work the bold was faking.
- **Connected reasoning**, in which case write the paragraphs properly and cut the bold, since the connections are the part worth reading.

There is a third case that looks identical and is not a fault. A **uniform catalogue**, where every entry repeats the same labelled parts in the same order, is a structure rather than a stub stack, and the repetition is what makes it scannable. The anti-pattern catalogue in `anti-patterns.md` is one: ten entries, each with a failure, a test, and a fix, always in that order.

The distinction is uniformity. Repeated identical labels across every entry are a format. Ad hoc bolded lead-ins that differ each time are a section that has not decided what it is.

Bold is for a label inside a list item, for a repeated catalogue label, or for the rare sentence that must not be missed. Two consecutive ad hoc bold-led paragraphs is a warning; more than that means the section needs restructuring rather than reformatting.

## Diagrams

Prose first. Write the structure down, and add a diagram only where the structure resists being written, never as decoration or as a summary of what the text already said.

A diagram earns its place when a relationship is genuinely hard to hold in sentences, such as a topology with many crossing connections. Most baselines do not have one, and a well-written composition section usually removes the need.

When one is used, the boundaries must accurately represent deployment and process boundaries, internal components, persistence, and external systems. **They must not imply runtime separations that do not exist.** See `diagrams-in-records.md`.

## Rules

Six rules hold regardless of the project:

- **Number it 0000**, outside the decision sequence, so it never reads as a decision and never consumes a decision number.
- **Title it `ADR-0000: Architecture Baseline`**, exactly.
- **State in the first line that it is not a decision record**, along with what it may and may not be cited for.
- **Give it a date and no status.** A description cannot be accepted or rejected.
- **Treat it as immutable.** Never update it to match the current system. Its value is that it keeps describing the starting point while the system moves away from it.
- **Write only one, only at adoption.** A second baseline means the first was treated as living documentation.

## Living documentation

A baseline and current architecture documentation do different jobs and should not be merged.

Current architecture documentation tracks what is true now and is updated as the system changes, which means it stops describing the origin almost immediately. A baseline is dated and frozen, so it continues to. Keeping both is reasonable. Keeping one and expecting it to do both is not.

## Structure

The eight top-level sections below are the same in every baseline, in this order. They follow how a reader meets an unfamiliar system rather than the order the categories were inspected in.

Subsections are chosen per project. A system built around a job queue, a rendering pipeline, or a set of agents puts that under `How it works` and names it for what it is. Only the top level is fixed:

| Section | Holds | Fixed |
| --- | --- | --- |
| `What this record is` | The not-a-decision statement, what it may be cited for, immutability | Wording is near-boilerplate |
| `What the system does` | Purpose, then the capabilities and what implements each | Mandatory |
| `How it is built` | Composition, technology, persistence, deployment | Subsections flexible |
| `How it works` | Execution, interfaces, the mechanisms specific to this system | Subsections flexible |
| `What it connects to` | External systems and how the system reaches them | Mandatory |
| `Security boundaries` | Authentication, authorization, tenancy, secrets, trust boundaries | Mandatory, state absence plainly |
| `What is incomplete` | Present but unused, known inconsistencies | Mandatory |
| `Where rationale was recorded` | Documented reasoning, and that the rest has none | Mandatory |

## Template

```markdown
---
date: YYYY-MM-DD
record-type: Baseline
---

# ADR-0000: Architecture Baseline

## What this record is

**This is not a decision record.** It describes the architecturally significant
state of the system on [date], the date this log was adopted, and it records no
decision, no rationale, and no recommendation.

It may be cited as evidence of what the architecture was on that date. It may
not be cited as evidence that any of it was decided, endorsed, or recommended.
It is written once and is not updated as the system changes, so later records
will move away from what is described here while this document goes on
describing the starting point.

## What the system does

[What the system is for, in plain terms, before any component is named. Who
uses it and what they do with it. Two or three paragraphs.]

[Then the capabilities, introduced by a sentence and written as a list, each
naming what the user does, what happens, and which parts carry it.]

## How it is built

### Composition

[Applications, packages, modules. Dependency directions. Process boundaries.]

### Technology

[Frameworks, runtimes, libraries, databases, protocols, providers. Versions
only where a different major would imply a different architecture.]

### Persistence

[Storage technology, and the data semantics later records will rely on.]

### Deployment

[Runtime assumptions. Say plainly when nothing is deployed.]

## How it works

### [The system's central mechanism]

[Name this for what the system actually does: the pipeline, the queue, the
render loop, the agents. This is where a baseline earns its length.]

### Interfaces

[Public and internal interfaces, protocols, event mechanisms. For an HTTP
surface: route, methods, one line of purpose. No schemas or status codes.]

### [Recurring mechanisms]

[Adapters, registries, validation boundaries, caching, concurrency control.]

### Operational constraints

[Timeouts, ceilings, limits, and the ones that are absent.]

## What it connects to

[External systems and how the system reaches them.]

## Security boundaries

[Authentication, authorization, tenancy, secrets, trust boundaries. Where a
control does not exist, say so plainly and without judgement.]

## What is incomplete

### Present but unused

[Subsystems that exist and are never reached.]

### Known inconsistencies

[Contradictions and duplicated mechanisms later records may need to resolve.]

## Where rationale was recorded

[The choices whose reasoning was written down, and where. Then state plainly
that no rationale was recorded for anything else, and that none is supplied.]
```
