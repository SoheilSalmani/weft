# Writing Linear content

## Contents

- How much structure
- Voice
- Mechanics
- Issue descriptions
- Projects
- Initiatives
- Milestones
- Comments

## How much structure

Choose the least structure that improves comprehension.

Prose, no headings, when the outcome is obvious to the team, there is one reader, the work fits one pull request, or the object closes within a week. This covers most issues.

Structure when more than one person must agree on scope, the work is delegated to an agent, the object outlives the current context, or someone will audit it later. Structure starts at roughly 150 words, and it starts because the content grew, never because a template offered it.

Concision is not a style preference. Measured against the same content written conventionally, concise text improves comprehension by more than half, scannable layout by nearly as much, and neutral rather than promotional language by a quarter. Promotional writing measurably slows readers down, because they spend effort filtering it.

## Voice

Calm, factual, present tense, specific. Write to a competent colleague who was not in the room.

Do not use: promotional adjectives, "successfully", "robust", "seamless", "leverage", "utilise", "in order to", "it is important to note that", three-part lists used for rhythm, "not only, but also", em dashes, or any sentence whose removal changes nothing.

Do not open by restating the title. Do not end with a summary of what was just said.

## Mechanics

| Element | Convention |
| --- | --- |
| First sentence | States what changes and for whom. Stands alone when truncated. Never repeats the title |
| Paragraphs | One idea. Three sentences is usually the ceiling |
| Bullets | For enumerable, parallel items only. Never for a chain of reasoning, which loses its connective tissue when broken into fragments |
| Headings | Only when there are at least two, and only above roughly 150 words |
| Links | Prefer an `@` mention of the Linear object. Note the side effect: mentioning an issue in a description creates a `related` relation, so do not scatter mentions |
| Dates | Absolute and ISO, `2026-08-13`. Never "next week", "recently", or "soon" |
| Metrics | Number, unit, source, and as-of date. "p95 login 2.4s, measured in staging, 2026-08-11" |
| Acceptance criteria | Only when done-ness is genuinely contested, or an agent will execute it. Otherwise the outcome sentence is the criterion |
| Open questions | Phrased as questions, each naming who can answer. Delete when answered |
| Risks | Only when the risk would change a decision |
| Non-goals | Only when someone would reasonably expect them to be in scope |
| Dependencies | Use relations and project dependencies. Prose only for things Linear cannot model, such as a vendor contract |
| Long evidence | A collapsible section, opened by `>>>` in the editor, keeps a log or trace available without burying the object |
| References | Attach as resources or links rather than as a wall of URLs in the body |

## Issue descriptions

The description answers what is owed and how we will know, as of now.

Enough context is: a reader who has never seen the codebase understands why this exists and what changes, and whoever implements it knows where to start and what not to touch. More than that belongs elsewhere.

When new information arrives, edit the description only if it changes the outcome, the scope, or the definition of done. Evidence and discussion stay in comments.

Implementation notes accumulate because there is nowhere obvious to put them. There is: notes about how go in the pull request, notes about what changed go in a comment, and only the current definition of the outcome lives in the description.

## Projects

Seven questions, each with one home and no duplication.

| Question | Answered in |
| --- | --- |
| What are we trying to change? | The summary, one sentence, at most 255 characters |
| Why does it matter? | First paragraph of the description |
| What does success mean? | The description, with a measure and its baseline |
| What is in and out of scope? | The description, out-of-scope only where it would surprise someone |
| Where are we now? | The latest update and milestone progress. Never the description |
| What is uncertain or blocked? | The latest update, and project dependencies |
| Where is the deeper information? | Resources and documents |

The summary is the only part most people read, because it is what appears in lists, cards, exports, and tool results. Write it last, when you know what the project is, and make it a sentence rather than a fragment.

The description runs roughly 150 to 400 words. Any block that would be empty is omitted rather than left as a heading.

Keep execution detail out of a project unless a reader needs it to understand a product or scheduling implication. "The migration cannot be reversed after the cutover" belongs in a project. The migration script does not.

## Initiatives

Two to four short paragraphs: what changes at the company level, why now, what this will not do, and how we will know it worked. No mission statements. No restating the projects underneath, which are visible.

## Milestones

A milestone exists when the project passes through states a stakeholder would notice. A project with three issues does not need any.

The name is the test. Ask whether the name can be true. `All existing tenants migrated` can. `Backend` cannot. Discipline names and phase numbers are the common failure, because a project is never half-done in a way that maps to "frontend".

A description on a milestone is usually unnecessary. Add one only when the entry condition is not obvious from the name.

## Comments

Comments carry time-stamped things: questions, answers, evidence, handoffs, decisions, changed assumptions, and agent reports.

Promote a comment into the description when it changes the outcome, the scope, or the definition of done. Promote it into an ADR when it settles something expensive to reverse. Promote it into a document when several issues will need it. In every case, leave the comment where it is and let the canonical location be canonical.

A decision recorded in a comment is one line: what was decided, why, when, and by whom. If it is architectural and expensive to reverse, the comment links to the ADR rather than containing the reasoning.
