---
name: writing-repository-readmes
description: Writes and improves a repository README as a landing page and navigation layer rather than a documentation home. Covers the first screen, which sections earn their place, badges, maintainer attribution, and star or follow invitations. Use when creating or improving a README, when deciding where a piece of documentation belongs, or when asked whether content belongs in the README.
---

# Writing repository READMEs

A README is a decision aid and a switchboard. A visitor arrives with one question, *is this relevant to my problem*, and the README's job is to answer it fast and then point at the right door. It is not the documentation.

**GitHub's default repository search reads only the name, description, and topics.** It does not read the README. So a README earns nothing from keyword work inside GitHub; it is written for humans who already arrived, and for external search engines that crawl the rendered page.

## First, establish the mode

Never write a public-facing README for a private repository.

```bash
gh api repos/{owner}/{repo} --jq '{private, visibility, license: .license.spdx_id, homepage, topics}'
```

| Mode | What applies |
| --- | --- |
| Public, community-driven | Everything below |
| Public, source-available | Discovery and onboarding. Contribution invitation restrained. No star pressure |
| **Private or internal** | **No SEO, no star or follow invitation, no social preview.** Optimise for a colleague or a future self: what it is, how to run it, where the docs are |

State which mode you detected and what you therefore left out.

## The first screen

In order, and only what applies:

1. **Project name**, matching the repository name. One name for one project, everywhere.
2. **One sentence** naming the category and what it does. `X is a <category> that <does what> for <whom>.`
3. **One supporting sentence** on what makes it different, when there is something true to say.
4. **A visual**, when the project produces something visible. Highest value for user interfaces and command-line output. Skip for libraries.
5. **Quick start**, copy-pasteable, the shortest path that works.
6. **A documentation link** with descriptive text.

Badges come after the name only when each answers a reader question, four at most.

Never open with architecture, project history, contribution instructions, exhaustive installation, sponsorship, a full-width logo, or a mission statement.

`references/section-patterns.md` holds worked openings by project type, the badge budget, and call-to-action wording.

## Which sections earn their place

For each candidate ask: **does this help a first-time visitor understand, evaluate, start, or navigate?** If not, it belongs elsewhere and gets a link.

Almost always out of the README: complete API or CLI reference, every configuration option, every environment variable, long troubleshooting, architecture history, the full contribution workflow, the changelog.

**Linking is not an excuse for an empty landing page.** A README consisting of a title and `See our docs` fails the visitor completely: they cannot tell what the project is, whether it is relevant, or whether the click is worth making. The floor is the first screen above, held locally, whatever else lives elsewhere. Emptiness and bloat are the same failure seen from opposite sides.

Do not add a table of contents unless the README is genuinely long. GitHub generates one from the headings, reachable from the Outline control, so a hand-maintained copy is a second thing to keep correct.

## Verify before you write

Do not write commands from memory. Read `package.json`, the actual scripts, and the current CLI before putting any command in the README, and check that links resolve. `references/freshness.md` covers the checks.

If a README statement contradicts the code, fix or remove it. Git history preserves the old version; a wrong instruction on the landing page costs every new reader.

## Routing content out

`references/routing.md` decides where a piece of content belongs: the decision tree, the authoritative home for each kind of information, and when local beats external. When moving content out of a README:

1. Identify which copy is authoritative, which is not always the README.
2. Keep a short local summary saying what the destination contains and when to use it.
3. Replace the depth with a descriptive link.
4. **Never silently delete information that exists nowhere else.** If there is no destination yet, say so and propose one rather than dropping it.

Link text names the destination: `Production deployment guide`, not `here` or `docs`.

## Tone and claims

Specific, technically accurate, calm, welcoming. Every claim carries evidence or does not appear.

Do not write fastest, best, secure, production-ready, enterprise-grade, lightweight, scalable, blazing-fast, seamless, revolutionary, next-generation, supercharge, or unlock without a number or a definition behind it.

Do not repeat the project name for search. Google does not reward it and readers notice.

## Maintainer, stars, and following

Public repositories only, and after the reader has seen the value, never above the quick start.

**Resolve the maintainer from the platform, never guess.** `gh api repos/{owner}/{repo} --jq .owner.login`, then `gh api users/{login} --jq '{login, name, html_url}'`.

**Stars** are documented as helping the reader find the project again, and as feeding GitHub's rankings and recommendations. One conditional line, once, plain text.

**Following** is narrower than most READMEs claim. GitHub documents only that followers see public activity on their dashboard. **There is no documented release notification.** Never write that following gives release alerts; if a reader wants releases, the mechanism is watching the repository with a Releases-only setting.

Order of invitations, and it matters: use the project, read the docs, contribute, then star, then follow. A social ask above the quick start costs the reader the thing they came for.

**Personal attribution suits a personally maintained project.** On an organisation-owned repository a personal follow invitation is usually out of place, and the maintainer line becomes the team or is omitted. Check `.owner.type` before writing one.

## Before you finish

- The mode was detected, and public-only material is absent from a private repository.
- One project name, used consistently.
- The first sentence names the category and what it does.
- Every command was verified against the repository.
- Every link resolves and its text says what is behind it.
- Nothing unique was deleted without a destination.
- No claim lacks evidence, and no keyword is repeated for search.
- Any star or follow line is single, conditional, and accurate about what GitHub does.
