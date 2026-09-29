---
name: tracking-work-in-linear
description: Creates or restructures work in Linear, the issue tracker. Covers issues, projects, milestones, initiatives, sub-issues, comments, and documents, and decides whether something belongs in Linear at all, or in a GitHub issue, a pull request, or an ADR. Use when asked to file, plan, split, rewrite, or move a specific piece of work. For a bulk audit of existing Linear content, use reviewing-linear instead.
---

# Tracking work in Linear

Work gets filed as whatever object is nearest, with detail invented to fill a template, and then rewritten later by an agent that had not read what was already there. This skill exists to stop those three things.

The conventions live in `linear-conventions`. Invoke that skill if it is not already loaded, and read the one reference file that answers the question in front of you rather than all of them.

## Three modes, and which one you are in

| Mode | What you produce | Authorised by |
| --- | --- | --- |
| Draft | Text in the conversation. Nothing touches Linear | Any request to write, plan, or rephrase something |
| Propose | A preview of the exact object, its properties, and its body, with unknowns listed | The default when someone asks for work to be filed |
| Write | A call to the Linear MCP server | An explicit yes to the preview, or an instruction that plainly authorises writing |

Default to propose. Never move to write because the draft came out well, because an existing object could be phrased better, or because the change looks obviously right. A better version is not authorisation.

An instruction such as "create an issue for this" authorises creation, and still gets a preview first, because the preview is where the wrong project, the invented date, or the missing unknown gets caught. An instruction such as "just create it, no preview" removes the preview and nothing else.

## Before writing anything, read

Spend the reads. Almost every bad object comes from writing before looking.

1. **The request**, for what is actually being asked.
2. **The repository**, for what is true. The state of the code decides scope more often than the request does.
3. **Existing Linear objects.** Look for an issue or project that already covers this. If one exists, return it and say whether it covers the request, rather than creating a near-duplicate.
4. **The target team's statuses and labels**, so you use what exists rather than inventing vocabulary.
5. **Existing ADRs**, in `docs/adr/`, when the work could touch an architectural decision.

The Linear MCP server is configured as `linear-server`, and its tools appear as `mcp__linear-server__*`. If it is unavailable, produce the object as markdown for someone to paste, and say that is what you did.

## Choosing the object

`.agents/skills/linear-conventions/references/objects.md` holds the decision framework. The short form:

- One reviewable outcome with one owner is an **issue**.
- An outcome with a definable end, spanning enough issues to need coordination, is a **project**.
- Two or more projects sharing an outcome, with real tradeoffs between them, is an **initiative**.
- A state a project passes through that a stakeholder would notice is a **milestone**.
- A recurring question with a computable answer is a **view**, not an object.
- Something with no end state is not a project, whatever it is called.

If the work turns out to be technical execution detail rather than an outcome, route it. `.agents/skills/linear-conventions/references/boundary.md` decides between a GitHub issue, the pull request, and an ADR. Say where you routed it and why, rather than filing it in Linear anyway.

If the work is too large for one issue, propose a parent and a small set of children. The same holds for any structure you would create in one go, such as a project with milestones or an initiative with projects: show the whole shape first, and create none of it until it is approved.

### Sometimes the answer is no object

Filing work nobody will read costs more than it saves. Recommend creating nothing when the change is small, self-contained, and has no consequence anyone would ask about later: renaming an internal key and its call sites, a dependency bump, a lint fix, a comment correction. The pull request is the record, and it is a better one, because it shows the change.

Say so plainly, with the reason, and offer the pull request as the alternative. If the user wants it tracked anyway, track it. This is advice, not a veto.

## Writing it

Titles follow `.agents/skills/linear-conventions/references/naming.md`. Bodies follow `.agents/skills/linear-conventions/references/writing.md`. Shapes for the common kinds of work are in `references/archetypes.md`, and they are patterns to draw from, not templates to fill.

Choose the least structure that improves comprehension. For most issues that is a title and one or two sentences.

Set only the properties you were given or can read from the system. Leave the rest empty. An empty property is a legible unknown; a guessed one is a fact nobody chose.

## Editing something that already exists

1. **Read the current state**, including comments and linked objects, before proposing a single change.
2. **Understand why it says what it says.** A sentence that reads oddly is often the one fact someone needed.
3. **Preserve every useful fact.** Anything you cannot show to be obsolete stays.
4. **Relocate rather than delete.** Implementation detail moves to the pull request or a GitHub issue, evidence moves to a comment, decisions move to an ADR. Say in your response what moved where.
5. **Do not rewrite for style alone.** Rewriting is justified by information that is wrong, missing, stale, or in the wrong place. "I would have phrased it differently" is not a justification.

**Leaving it alone is a result.** When an object is accurate, current, and readable, say so and change nothing. A rewriting skill that always rewrites is worse than no skill: it burns attention, churns history, and teaches people to stop reading the diffs.

When you remove a superseded requirement, keep the one line that explains why the direction changed, if that reason still shapes the current scope. Delete the dead requirement, not the reason it died.

Prefer patch operations over resending a whole description. The Linear MCP tools accept a list of edits anchored on exact text, applied atomically, which keeps the parts you did not touch exactly as they were. Send a full description only when you are genuinely replacing all of it.

When an assumption has been invalidated, remove it from the description and leave a comment saying what changed and when. Do not leave contradictory text in place for the sake of history: Linear keeps the history, and the description is the only thing anyone reads.

## Linking a Linear issue to a pull request

Linear links through the branch name, the pull request title or body, or a commit message. **The verb decides what happens on merge**, because Linear's pull request automations are configured per team and the documented defaults move the issue to In Progress when a pull request opens and to Done when it merges.

| Intent | Words |
| --- | --- |
| Merging completes this work | `close`, `closes`, `fix`, `fixes`, `resolve`, `resolves`, `complete`, `completes`, `implement`, `implements` |
| Merging contributes to it | `ref`, `refs`, `references`, `part of`, `contributes to`, `toward`, `towards` |
| Merely related | `relates to`, `related to` |
| Do not link at all | `skip` or `ignore` before the identifier |

Use a closing word only when merging genuinely finishes the work. **Check the team's configured automations and its workflow states before assuming**: a team whose states stop at Done has nowhere to move afterwards, so a closing word there makes merge equal completion. A team with a Released or Deployed state can afford one earlier.

## What you may never invent

The full list, with the reasoning, is in `references/accuracy.md`. Read it before your first write in a session.

Never invent an owner, a date, a status, a priority, a metric, a customer's demand, a decision, a dependency, a blocker, or the fact that something is finished. If a value would be a guess, leave it unset and say so in the preview.

Distinguish what you know from what you worked out. Mark facts read from a system, statements made by a person, and inferences you drew, and never let an inference become a fact because it made the sentence read better.

## The preview

Before any write, show:

- Object type, title, and target team or project
- Properties you intend to set, and the ones you are deliberately leaving empty
- The full body as it will appear
- Unknowns, each with what would resolve it
- Anything you are routing out of Linear, and where to
- For an edit, what changes, what you are removing, and where it goes instead

Keep it scannable. The preview is the last point where a wrong project or an invented date is cheap to fix.

## Before you finish

- The object type is the one the framework chose, not the one that was easiest.
- The title stands alone in a notification, and repeats no property.
- The first sentence says what changes and for whom, without restating the title.
- No heading holds less than a paragraph, and no section was added by habit.
- Nothing in the body would be false once the pull request merges.
- Every date, number, and name came from a system or a person.
- Unknowns are stated rather than filled.
- Nothing was written to Linear without approval.
