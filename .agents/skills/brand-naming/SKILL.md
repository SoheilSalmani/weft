---
name: brand-naming
description: Names companies, platforms, products, developer tools, CLIs, libraries, SDKs, frameworks, protocols and open-source projects to brand quality. Builds a naming brief, derives conceptual territories, generates broadly, then eliminates through linguistic, collision, domain, registry and preliminary trademark screening and picks the best name rather than handing back a menu. Also audits or compares names the user already has. Use when asked to name or rename a project, product, tool or company, to check whether a name is good or available, or when an idea becomes a real project concept needing a durable name. Not for naming variables, functions, files, branches or issues, not for something that already has a name such as a repository, and not for personal tooling or configuration with no audience beyond the person using it.
compatibility: Needs web search for collision and trademark screening, and network access for domain and registry checks. Degrades by reporting which screens could not run.
---

# Brand naming

The output is **not a list of attractive strings**. It is a small number of names that survived scrutiny, each with its strategy, its evidence, and its worst problem stated plainly.

```
understand → brief → territories → generate broadly
→ eliminate cheaply → screen → verify finalists → recommend
```

**Generate broadly, eliminate aggressively.** Dozens of candidates internally; three to seven shown. Never show the raw pool.

## When not to name

**A raw idea keeps a descriptive title.** Naming begins when something is a real project or product concept: it has a thesis, an audience and an archetype. "Maybe agent-aware migrations" is not ready.

**Something that already has a name keeps it.** A repository, a directory, a command or a collection of configuration is named by the thing itself. Take that name and say that is what you did. Coining a second one leaves the disk, the notes and the conversation disagreeing about what one project is called.

**Not everything wants a brand.** Personal tooling, dotfiles, configuration and internal collections need a name someone can type, not one that survives screening. Where there is no audience beyond the person using it, the descriptive name is the right answer, and saying so is the whole job.

If positioning is too thin to name responsibly, **shape the positioning first and say so**. Inventing a coined word from five ambiguous words is the failure this skill exists to prevent.

Do not trigger on variable, function, file, branch, column or issue naming.

## Intake

Establish: what it is, who it is for, category, core value, tone, **archetype**, expansion horizon, markets, open-source or commercial, and technical ecosystem.

**Read the context first.** An idea-garden concept note, a repository, or a README usually answers most of this. Ask only what is genuinely unresolved and would change the naming, and never run a questionnaire.

## Archetype decides the weighting

This changes the answer more than anything else. Read `references/archetypes.md`.

| Archetype | Weighted heavily |
| --- | --- |
| Company, platform | Extensibility, trademark strength, broad resonance |
| Product, consumer app | Memorability, pronunciation, emotional fit |
| Developer tool, OSS | Search ownership, registry, speech in docs |
| **CLI** | Typing effort, shell collision, lowercase form |
| Library, package | Registry availability and normalisation, import ergonomics |
| SDK | Relationship to the parent brand, rarely standalone |
| **Protocol, standard** | Descriptive credibility. Distinctive branding may be wrong here |
| Internal codename | Memorability only. Minimal ceremony |

## Territories before candidates

Derive four to eight **conceptual territories** before generating anything. Territories must differ semantically, not be synonyms. Generating straight from the product description produces fifty variants of one obvious compound.

Then generate across strategies: coined, arbitrary, suggestive, metaphorical, compound, lexical transformation, technical metaphor, morphology and etymology. `references/strategies.md` holds the taxonomy, the generation methods, and the cliché list.

## Screening funnel

Cheapest eliminations first, because trademark and international screening are expensive and must never run on names that fail a free test. `references/screening.md` has the detail.

```
0 strategic fit and archetype      free
1 linguistic quality               free
2 cliché and saturation            free
3 web, GitHub, registry collision  cheap
4 domain screening                 cheap
5 preliminary trademark            expensive
6 international                    finalists only
7 history and reputation           finalists only
```

```bash
node scripts/availability.mjs NAME --tlds com,dev,io --registries npm,pypi,crates
```

The script gathers evidence and makes no judgement. It registers, purchases and publishes nothing, and **nothing in this skill ever does**. Researching availability is not authorisation to acquire.

## What the evidence may and may not say

**Never state availability you did not look up.** Asserting that a domain or package is free without running the check is the worst failure this skill can produce, because the user will act on it. If the check did not run, say the check did not run.

**"free" from a registry or RDAP means no record was found. It does not mean the name is clear.** Registry policy can withhold a name that RDAP reports as unregistered, and a name can be free everywhere and still be unusable because a famous company owns the concept.

**`invalid` means the candidate cannot be a domain label as written**: spaces, dots, leading or trailing hyphens, or non-ASCII characters. Non-ASCII is treated as invalid rather than screened, because a Cyrillic lookalike of a famous brand is a homograph attack, not a naming option. If an internationalised name is genuinely wanted, that is a deliberate decision needing punycode and its own confusability review.

**`unknown` is never `available`.** If a source fails, say the screen did not run. "WIPO unreachable" must never become "no trademark collision".

Trademark screening produces one of five verdicts, never a binary: no obvious conflict found, potential conflict, material conflict, requires legal review, screening unavailable. **Never say a name is legally safe.** Say the disclaimer once, not after every candidate, and do not impose clearance ceremony on a small utility.

Collision is graded by prominence and category adjacency, not by exact match. An abandoned repository is negligible; a same-category project with heavy adoption is fatal; a famous adjacent brand is fatal regardless of registry status.

**Search ownership is its own criterion.** A name can be linguistically perfect and trademark-clear yet impossible to find. Reject on that, and say so.

## Decide, do not ask

**Pick the name.** The user does not want to arbitrate between finalists, and asking them to is handing back the work.

Report in a few lines: the chosen name, what it means and how it is pronounced if that is not obvious, its strategy and distinctiveness class, availability with the verification timestamp, preliminary collision and trademark status, and **its single worst downside**. Name the two runners-up in one sentence with why they lost.

The user can override, and a stated preference wins over a close call. They are simply not asked to referee.

**If nothing survives, say "no candidate from this round is strong enough" and generate new territories.** Never pick the least bad name because work was already done. Deciding does not mean settling.

## Existing names, comparison, renaming

**Audit mode**: given a name, screen it fully before proposing alternatives. Often the right answer is that the existing name is good and should stay.

**Rename mode**: weigh migration cost: adoption, published packages, URLs, documentation, user familiarity. A marginally better name rarely justifies a rename. Renaming technical identifiers is not automatic; route that impact analysis to `work-orchestration`.

## Handing off

Naming owns brand identity and produces a **brand plus a factual descriptor**. It owns nothing downstream.

- **idea-garden**: write the chosen name back to the concept note, preserving the original text and the previous title as an alias. Naming is not promotion; create no execution work.
- **work-orchestration**: on deliberate promotion, pass the brand, the descriptor and the concept link. The brand becomes the project name and is not downgraded to a generic title.
- **repository-presentation**: owns repository name, description, topics, README opening and discoverability. Provide brand, descriptor and positioning; do not restate its rules.

## Before you finish

- The archetype was established and drove the weighting.
- Territories preceded candidates, and the runners-up differed strategically.
- No expensive screen ran on a candidate that failed a free one.
- Every availability claim carries its source and timestamp.
- Failed screens were reported as failed, never as clear.
- The worst downside of each finalist is stated.
- Nothing was registered, purchased or published.
- No availability was asserted that a check did not actually return.
