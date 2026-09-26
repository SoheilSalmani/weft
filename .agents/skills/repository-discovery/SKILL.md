---
name: repository-discovery
description: Sets or reviews the repository metadata that drives findability, meaning the description, topics, homepage, and social preview. Covers researching the terminology people actually search before writing any of it. Use when asked to make a repository discoverable, to write its description, or to choose topics.
---

# Repository discovery

**GitHub's default repository search reads only the name, description, and topics.** README content is reachable only through the `in:readme` qualifier, which almost nobody types.

Three short fields therefore carry all discovery inside GitHub, and they are usually the least considered part of a repository. That is where the effort belongs.

## First, establish the mode

```bash
gh api repos/{owner}/{repo} --jq '{private, visibility, description, homepage, topics, license: .license.spdx_id}'
```

**A private repository gains nothing from any of this.** Nobody browses topics they cannot see, GitHub does not analyse private repositories for topic suggestions, and search does not reach them. For a private repository, produce recommendations to apply at publication and change nothing.

For a public repository, continue.

## Research terminology before writing anything

Never write a description from the codebase's own vocabulary. Codebases name things after their internals; searchers name things after their problem.

Build a search-intent map first, following `references/search-intent.md`. It is an internal artefact and **never appears in the README or the description**.

Verify candidate topics against reality rather than guessing:

```bash
gh api "search/repositories?q=topic:<candidate>&per_page=1" --jq '.total_count'
gh search repos --topic <candidate> --limit 5 --json fullName,description
```

A topic with a handful of repositories is either wrong or too new to help. A topic with thousands tells you the established spelling and shows you what a browser of that page expects to find.

## Description

The highest-value field in the repository. It has to make sense with no README, and it becomes the page context for external search.

Cover category, primary function, and where space allows a differentiator or the target user.

Method: **generate three to five candidates, then judge them**, on immediate comprehension, specificity, search relevance, truthfulness, and whether it reads naturally aloud. Pick one and say why.

Avoid slogans, vague adjectives, `Official repository for...`, `A simple tool for...`, repeated keywords, and emoji that carry no meaning. GitHub publishes no length limit; keep it short enough to survive truncation in lists.

The description and the README's opening sentence overlap in meaning and should not be the same string. The description is read with no context at all, in a list of results; the opening sentence is read by someone already on the page. Write each for its own reader, and let them differ.

## Topics

Up to 20, lowercase letters, numbers, and hyphens, 50 characters each. Use far fewer than 20.

Cover the dimensions that a browser would use: category, ecosystem, primary use case, problem domain, and a protocol or standard where it is a real identifier. GitHub's own guidance is to classify "the repository's intended purpose, subject area, community, or language".

The test for every topic: **would someone browsing that topic page be glad to find this repository?** If not, drop it.

Do not add a topic per dependency, per buzzword, or per synonym. Do not add competitor names. Language topics are usually redundant with GitHub's own language detection. Prefer an established spelling over an invented one.

## Homepage

One scarce, prominent slot, and it should answer the visitor's most likely next step off GitHub: documentation, a live demo, or the product.

Leave it empty rather than filling it with something incidental. An empty field is honest; a link to a stub costs trust.

## Social preview

PNG, JPG, or GIF under 1 MB, at least 640 by 320, with 1280 by 640 recommended. Without one, a shared link expands to basic repository information and the owner's avatar, which is a reasonable default.

Worth creating when the repository is public and gets shared. It carries the name and a short category phrase, legible at small sizes. Not a marketing poster, and not a claim the project cannot support. If it is generated, the generator becomes something to maintain, which has to be worth it.

## Do not churn

Metadata that already works is left alone. Terminology research will always surface another plausible synonym, and rewriting a good description each time it does produces movement without improvement, while making the project look unsettled to anyone watching.

Change a description or a topic set when it is inaccurate, misleading, or missing an established term the audience actually uses. Not because a different phrasing is available. Say explicitly when the current values are already adequate.

## Honesty

Do not promise ranking, stars, or recommendations. Stars are documented as affecting rankings and recommendations, and that is the extent of what can be claimed.

Content produced primarily to manipulate search violates the platform's own spam guidance, and on fields this short it is obvious to readers as well. Use the researched vocabulary naturally, once, where it improves comprehension.

## Mutation safety

Each field is a live repository setting, and each needs its own authorisation.

| Act | Default |
| --- | --- |
| Read current metadata, research terminology, propose values | Free |
| Update the description | Ask, showing the exact string |
| Update topics | Ask, showing the exact set |
| Change the homepage | Ask |
| Upload a social preview | Ask. It cannot be set through the API and needs the web interface |

`gh repo edit --description "..."` and `gh repo edit --add-topic ...` apply changes. Show the command and the value before running it, and never batch several settings into one unreviewed call.

## Before you finish

- The mode was established, and a private repository received recommendations rather than changes.
- Every topic was verified to exist with real repositories behind it.
- The description was chosen from several candidates, with a stated reason.
- No keyword appears twice, and nothing was written for a crawler.
- The homepage points at a real next step, or stays empty.
- Nothing was applied without showing the exact value first.
