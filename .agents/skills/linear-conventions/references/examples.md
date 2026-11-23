# Worked examples

Paired examples, weak version first. The rule each pair teaches is stated once, after the pair.

## Contents

- Titles
- A small issue
- A bug
- An issue drowning in implementation detail
- An issue where implementation detail is correct
- A project
- Milestones
- A project update
- An update when nothing moved

## Titles

| Weak | Better |
| --- | --- |
| `Authentication improvements` | `Reduce failed SSO logins for enterprise users` |
| `Fix auth` | `SSO login fails for accounts with two identity providers` |
| `[BUG] P1 - Login broken (frontend)` | `Login returns a blank page after the identity provider redirect` |
| `Q3 Auth Project` | `Make enterprise sign-in dependable` |
| `Database stuff` | `Move the schema source of truth to Drizzle` |
| `Investigate` | `Find out why staging response times doubled after 2026-08-04` |
| `Update docs` | `Document the token exchange contract` |

The weak versions fail in different ways, but all of them fail the same test: read alone in a notification, none of them tells you what will be different when the work is done. Prefixes, priorities, and component tags are properties, and they take up the space where the change should be.

## A small issue

**Weak**

> **Title**: Fix skill loading
>
> ## Context
> We have discovered that skills are not loading correctly in some cases.
>
> ## Overview
> This issue tracks the work required to fix the skill loading problem.
>
> ## Acceptance criteria
> - [ ] Skill loading is fixed
> - [ ] Tests pass

**Better**

> **Title**: Accept skill frontmatter that closes with more than three dashes
>
> Skill files whose frontmatter closes with a longer dash run are rejected as malformed, so the skill silently does not load. Accept the longer fence, and cover the malformed case in a test.

Three headings, two of them empty of content, and a criterion that restates the title. The rewrite is shorter than the template that surrounded the original, and it is the only version that says what is actually wrong.

## A bug

**Better**

> **Title**: SSO login fails for accounts with two identity providers
>
> A user with two identity providers configured sees a blank page after the redirect, and the request returns 400. Expected: a provider chooser.
>
> Seen in production on 2026-08-11 by two enterprise tenants. Not reproducible with a single provider.
>
> >>> Error line
> `AuthError: ambiguous provider binding for subject 4f21…`

Observed, expected, where it was seen, and one line of evidence. The full trace lives in the pull request or an attachment. The collapsible keeps the evidence available without making the object about the evidence.

## An issue drowning in implementation detail

**Weak**, roughly 1,500 words in the original, of which this is the shape:

> **Title**: Fix search
>
> ## Context
> `issueSearch.ts` calls `buildFilter()` at line 240, which passes `includeArchived` but not `includeArchivedProjects`. See also `searchRanking.ts`. The SQL currently emitted is:
> ```sql
> SELECT ... WHERE project.archived_at IS NULL ...
> ```
> ## Migration plan
> 1. Add a column ...
> ## Test plan
> 1. Add a unit test in `search.test.ts` asserting ...
> ## Alternatives considered
> We could rewrite the ranking layer ...

**Better**

> **Title**: Issue search omits issues from archived projects when "Include archived" is on
>
> Turning on "Include archived" still hides issues that belong to archived projects, so people cannot find work they know exists. Search should honour the toggle for archived projects as well as archived issues, without changing ranking, pagination, or project search.
>
> Technical work and the migration plan: [#412](<issue URL>)

Everything removed was relocated, not deleted. The SQL, the file references, the test plan, and the alternatives moved to the repository issue, and the one alternative that had product consequences moved to a comment. What is left is what a person who cannot read the codebase needs in order to agree the work is worth doing, plus the constraint that matters.

## An issue where implementation detail is correct

> **Title**: Make issue search respect the archived-projects flag
>
> Issue search excludes issues from archived projects even when "Include archived" is enabled.
>
> Update the issue search path so it respects the existing `includeArchivedProjects` flag, reusing the filtering behaviour already used elsewhere in search rather than implementing it again. Do not change search ranking, pagination, or project search behaviour.

This issue is being handed to an agent, and the named flag and the explicit non-goals are what stop it exploring or over-reaching. The detail constrains the work rather than describing it. Both this and the previous example are correct, for different readers. The mistake is writing the first version for the second reader.

## A project

**Weak summary**

> AI-powered platform for smarter customer conversations.

on a project named `Build a library of approved support replies`.

**Better summary**

> Give support agents a searchable library of approved replies so they stop copying answers between tickets.

The weak summary describes a different project from the one it sits on, which is worse than an empty summary, because a reader believes it. A summary is one sentence naming the change and who it is for, and it is what appears in every list and every tool result.

**Better description**

> Support agents copy answers out of old tickets, so a correction to one answer reaches none of the others. A library gives them one place to find an approved reply and one place to fix it.
>
> Success: an agent can find a reply by topic or by the customer's question, and send it without copying from an old ticket. Measured by the share of tickets answered with a library reply rather than a pasted one.
>
> Not in scope: automatic reply suggestions, translations of individual replies, and anything that requires changing the helpdesk.
>
> Open: do teams need private replies, or is the library shared? Nobody has asked for private ones yet.

Why, what success means, what is out, and what is uncertain. No status, because status lives in the update and in milestone progress. No headings, because it is under 150 words.

## Milestones

| Weak | Better |
| --- | --- |
| `Backend` | `Library serves replies over the API` |
| `Frontend` | `Agents can search the library` |
| `Phase 2` | `First team stopped pasting replies` |
| `Migration` | `All support teams moved to the library` |

Each better version is a sentence that becomes true on a particular day, and its progress bar means something. `Backend` is never true, and its progress measures nothing anyone would report.

## A project update

**Weak**

> This week the team continued to make great progress on the replies library project. We completed 7 of 12 issues (58%). The library is an important initiative that will help support agents share answers. Next week we will continue working on the remaining issues.

**Better**

> **At risk.**
>
> The API now serves replies, so `Library serves replies over the API` is done. Search is blocked: the search index needs a schema change we have not scoped.
>
> Target date moves from 2026-09-12 to 2026-09-26 unless the schema work turns out to be a day. I will know after the spike on 2026-08-18.
>
> Decision: private replies are out of scope for the first release, taken 2026-08-12.

The weak version restates the project, reports a percentage that Linear already attaches to the update, and says nothing about the date. The better version tells you the one thing you could not have worked out yourself: the date is moving, and here is when that becomes certain.

## An update when nothing moved

> **At risk.**
>
> No movement since 2026-08-06. Still blocked on the schema decision, now 12 days old. It needs a decision from whoever owns the search index.

Three lines, and the health value carries the rest. Do not write an update whose content is "work continues", because it turns a true unknown into a false healthy signal. When nothing has changed and nothing is blocked, post nothing: Linear marks projects that are overdue for an update on its own.
