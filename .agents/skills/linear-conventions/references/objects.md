# Choosing the Linear object

## Contents

- Three questions that resolve most cases
- Use X when
- Common situations, resolved
- Splitting and merging

## Three questions that resolve most cases

**Does it have an end state someone outside the work could verify?** If it never will, it is a view, a label, or a document. It is not a project.

**Would someone who cannot read the codebase need this to make a decision?** If not, it belongs in the pull request, a repository document, or an ADR.

**Is the reader ever different from the writer?** If not, and never will be, it may not need to be written at all.

## Use X when

| Use | When | Not when | The test |
| --- | --- | --- | --- |
| Initiative | Two or more projects share one outcome and someone makes tradeoffs between them | You want a filter, or there is one project | Can you write an initiative update that is not a concatenation of project updates? |
| Project | The work has a definable end and spans enough issues to need coordination | The work is ongoing with no end. Linear expects projects to have a start and a finish | Can you name the day it is finished? |
| Milestone | A project passes through states a stakeholder would notice | You want to group by discipline, team, or phase number | Is the name a sentence that becomes true? |
| Issue | One reviewable outcome with one owner at a time | It is a step of a single change you will do in one sitting | Would you open one pull request for it? |
| Parent issue | One outcome that must split across people, teams, or pull requests | You want a checklist. Use a checklist | Does each child make sense in a list without its parent's title? |
| Sub-issue | A slice someone else could take. Sub-issues inherit team, priority, and project, but not labels | You are enumerating your own steps | As above |
| Cycle | A team commits to near-term work and wants throughput signal | One person with no throughput question | Do you ever ask what was finished last cycle? |
| Project update | Health, expectations, blockers, or scope changed | Nothing changed. Linear already marks projects overdue for an update | Does it contain a sentence not derivable from the issue list? |
| Comment | A question, answer, piece of evidence, handoff, or decision at a point in time | The information is now the truth of the object. Promote it into the description | Will this read well in six months as a dated event? |
| Document | Depth that outlives an issue and would bloat a description: a spec, a research write-up, a contract draft | It is a decision, which is an ADR, or code detail, which is the repository | Would more than one issue link to it? |
| Label | A view or automation depends on it, and no property encodes it | It restates type, status, priority, team, or project | Name the view. If you cannot, do not create it |
| Relation | A real ordering or duplication exists | Two things are merely about the same area | Does `blocked by` change what someone works on today? |
| View | A recurring question with a computable answer | Membership is a deliberate, curated commitment, which is an initiative | Can a filter produce the membership? |

## Common situations, resolved

| Situation | Where it goes | Why |
| --- | --- | --- |
| User pain point | The first sentence of a project or issue, not its own object | A pain point is a reason, not a unit of work |
| Feature | Issue when one pull request does it. Project when it needs milestones, several people, or a launch | Size decides, not category |
| Small enhancement | Issue, two sentences, no headings | |
| Bug | Issue. Through Triage when reported by anyone other than the person fixing it | |
| Regression | Issue, with a `related` relation to the change that caused it | The cause link is the useful fact, not a label |
| Technical debt | Issue when bounded. A project only when it has a verifiable end state | "Reduce debt" cannot end, so it cannot be a project |
| Refactor | Issue whose first sentence is what it unblocks. If the only reason is tidiness, it is a pull request and no Linear object | |
| Architectural migration | Project with outcome milestones, plus an ADR for the decision to migrate | The decision and the execution are different artefacts |
| Research or spike | Issue titled with the question, carrying a timebox and the decision it unblocks. Findings go in a comment, then a document or ADR if durable | |
| Incident follow-up | One issue per remediation, each linked to the incident record | Each remediation has its own owner and end |
| Customer request | A Linear customer request attached to the existing issue or project | Not a new label, and not one issue per report |
| Operational task | An issue only if it must be tracked. Recurring ones use recurring issues | |
| Documentation work | An issue when the document is the deliverable. Otherwise it is acceptance criteria on the change that needs it | |
| Multi-month strategic objective | Initiative, only if it holds at least two projects | |
| Multi-team effort | One project shared across teams. Issues stay owned by one team each | |
| Project phase | A milestone if it names a state. If it names a stage of process, question whether it should exist | |
| Implementation task | Usually the pull request. See `boundary.md` for when a GitHub issue earns its place | |

## Splitting and merging

An issue is too large when it carries more than one reviewable outcome, when two people would work on it at once without coordinating, or when its acceptance criteria fall into groups that could ship separately.

When work is too large, propose a parent issue and a small set of children, and wait for approval before creating them. Do not silently create a tree.

Linear can convert an issue that has outgrown its size into a project, and a milestone that has outgrown its size into a project. Prefer conversion to recreating the object by hand, because conversion carries the content across.

When two issues describe the same outcome, mark one a duplicate of the other rather than closing it silently. The duplicate keeps a banner pointing at the survivor, which is what a later reader needs.
