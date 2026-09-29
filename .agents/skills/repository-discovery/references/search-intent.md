# Search-intent method

An internal design artefact. It shapes the description, the topics, and the README's opening sentence. **It never appears in the repository**, not as a keyword list, not as a synonym paragraph, not as a table.

## Contents

- The seven questions
- Where the evidence comes from
- Using the vocabulary
- A worked map

## The seven questions

Answer each in a phrase, from evidence rather than intuition.

1. **Primary audience.** Who has this problem, described by their role or task.
2. **Primary problem.** In the words they would use, not the words the codebase uses.
3. **Category.** What would someone call this who had never heard of it?
4. **Primary terminology.** The one phrase most likely to be searched.
5. **Secondary terminology.** Adjacent terms that are accurate.
6. **Ecosystem and platform terms.** Named products, standards, and runtimes.
7. **Differentiator.** The true thing that would make someone choose this.

Add an eighth, which prevents more damage than the others prevent: **terms to avoid**, meaning words that would attract the wrong reader or overstate what the project is.

## Where the evidence comes from

- **GitHub topic pages and search.** `gh api "search/repositories?q=topic:<t>&per_page=1" --jq .total_count` tells you whether a term is established. `gh search repos --topic <t> --limit 5 --json fullName,description` shows what a browser of that page expects.
- **The ecosystem's own documentation.** Official docs settle the spelling of a standard or product name. Use the term the ecosystem uses, not a paraphrase.
- **Package registries**, where the project ships one.
- **Comparable repositories**, for terminology only. Read their descriptions to learn what words the audience recognises. Do not copy their prose, their structure, or their claims, and do not assume a high star count means the writing is good.

## Using the vocabulary

Once each, where it improves comprehension. In the description, in the first sentence of the README, in a heading where it is the honest heading, and in topics.

Never in a repeated paragraph, never in a list of synonyms, never in a hidden comment. Google's guidance is explicit that content produced primarily to manipulate ranking violates the spam policies, and there is no word count to hit.

The test for any placement: **would this sentence be written this way if search did not exist?** If not, rewrite it until it would be.

## A worked map

An illustration for Stillwater, a fictional command-line linter for PostgreSQL migrations, with topic counts checked on 2026-09-25:

| Question | Answer |
| --- | --- |
| Primary audience | Backend developers and platform engineers who deploy schema changes to PostgreSQL |
| Primary problem | A migration takes a lock or rewrites a table and the application stalls during deploy |
| Category | Database migration linter |
| Primary terminology | `database migrations`, the phrase the audience searches for |
| Secondary terminology | Schema changes, migration safety, zero-downtime deploys |
| Ecosystem terms | PostgreSQL, Rails, Django, Flyway |
| Differentiator | Flags locking and rewriting statements before deploy, runs offline against migration files, no database connection needed |
| Terms to avoid | "database tool", which excludes nothing; "migration framework", which it is not; "guarantees zero downtime", which is untrue |

Candidate topics, each verified to exist with real repositories behind it: `postgresql` (about 123,000), `database-migrations` (about 760), `linter` (about 5,200), `zero-downtime` (about 200), `cli` (about 131,000).

`zero-downtime` is small but semantically precise, and precision beats volume for a topic: a browser of a 200-repository page is looking for exactly this. `cli` is large enough to be nearly meaningless and would be the first to drop if the set needed trimming.
