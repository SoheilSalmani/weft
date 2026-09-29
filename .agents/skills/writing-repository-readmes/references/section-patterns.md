# Section patterns

## Contents

- Openings by project type
- The badge budget
- Star and follow wording
- Maintainer attribution
- Status and deprecation
- Before and after

## Openings by project type

The shape is the same, the emphasis differs.

| Type | First sentence names | Then |
| --- | --- | --- |
| Library | What it does and for which language or runtime | One code example: input, call, output |
| CLI | What it does and what you run it on | One command and its representative output |
| Framework | The kind of application it builds, and the opinion it holds | The smallest working app |
| SDK | Which service it wraps and which language | Authenticate and make one call |
| Developer tool | The workflow it changes | A screenshot if there is a interface, otherwise one command |
| Agent tool | Which agent runtimes it targets, using their real names | The shortest path to a working artefact |
| Infrastructure | What it provisions or runs, and on what | The minimal deployment |
| Research project | The question it answers and the artefact it produced | How to reproduce the result |

Two failing openings, and why:

> A powerful next-generation solution for modern developers.

Names no category, no problem, no audience. Nothing here is checkable.

> Stillwater Migration Guard is a tool.

Names a category so broad it excludes nothing, and introduces a second name for a project called `stillwater`.

A working shape:

> Stillwater is a command-line linter that checks PostgreSQL migrations for table locks and rewrites before they reach production, so schema changes ship without downtime.

Category, action, ecosystem, and the differentiator, in one sentence a reader can check.

## The badge budget

Four at most, and each has to answer a question a visitor actually has.

| Badge | Keep when |
| --- | --- |
| CI status | Tests exist and run on the default branch |
| Package version | The package is published |
| Licence | The repository is public |
| Documentation | A documentation site exists |
| Coverage | The number is trustworthy and maintained |
| Compatibility | Supported versions genuinely constrain adoption |

Drop: visitor counters, technology logos, one badge per workflow, social counts already visible on the page, and anything whose value has not changed in a year.

A row of badges is a dashboard, and a dashboard above the first sentence delays the only thing the reader came for.

## Star and follow wording

Public repositories, after the value is visible, once.

The mechanics that make a star line honest: GitHub documents that starring helps the reader find the repository again, that many repository rankings depend on star count, and that stars feed recommendations. Both halves of the sentence, reader benefit and project benefit, are therefore true.

Working shape, to be rewritten in the project's own voice rather than pasted:

> If Stillwater is useful to you, starring it keeps it in your list and helps other people running PostgreSQL migrations find it.

Failing shapes:

> ⭐ STAR THIS REPO ⭐ — shouting, and no reason given.
>
> Star the repo to support continued development — implies maintenance is conditional on stars.
>
> Please star if you found this helpful 🙏 — guilt, and it appears three times in the file.

For following, the only documented effect is that a follower sees public activity on their dashboard, and that a followed user's stars may produce recommendations. So:

> Follow `<display name>` on GitHub to see new projects as they appear.

is accurate. This is not:

> Follow me to get notified about new releases.

Releases come from watching the repository with a Releases-only custom setting, which is a different control. If release notification is what the reader wants, point them at that.

## Maintainer attribution

One line near the end for a single-maintainer public project:

> Maintained by [`<display name>`](https://github.com/<login>).

A section with a biography, an avatar, and links to five social networks turns a project page into a personal page. The project stays primary.

Resolve the profile from the platform every time. The owner login comes from the repository, and the display name and URL come from the user endpoint. Never construct a profile URL from a name.

## Status and deprecation

An experimental project says so plainly, near the top, without hiding the value:

> Stillwater is experimental. Interfaces change without notice, and the rule configuration format is not yet stable.

A deprecated project puts the replacement above everything else, before the description, because that is the only thing a visitor needs:

> This project is no longer maintained. Use [successor](link) instead. Migration notes are in MIGRATING.md.

Never bury either in a section halfway down.

## Before and after

**Before**, a first screen that answers nothing:

```
# stillwater

[8 badges]

## Table of Contents
...

## Background
This project started in 2025 when we noticed that...
```

**After**:

```
# Stillwater

Stillwater is a command-line linter that checks PostgreSQL migrations
for table locks and rewrites before they reach production.

[one command and its output]

## Quick start
...
```

The badges that survived moved below the opening sentence, the table of contents went away because GitHub generates one, and the background moved to a documentation page where someone who wants it can find it.
