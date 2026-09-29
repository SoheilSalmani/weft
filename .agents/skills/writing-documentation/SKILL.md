---
name: writing-documentation
description: Write or restructure technical documentation using Diátaxis. Use when creating a tutorial, how-to guide, reference, or explanation, when a document is trying to be more than one of those, or when documentation exists but readers cannot follow it.
---

# Writing documentation

Documentation fails more often from being the wrong kind of document than from being badly written. A tutorial that stops to explain, a reference that gives advice, a how-to guide that teaches: each fails the reader. They came for one thing and were handed another.

Diátaxis names four kinds. Decide which one you are writing before you write a sentence, because almost every other decision follows from it.

## The compass

Two questions settle it.

| If the content | and serves the reader's | it is |
| --- | --- | --- |
| informs action | acquisition of skill | a tutorial |
| informs action | application of skill | a how-to guide |
| informs cognition | application of skill | reference |
| informs cognition | acquisition of skill | explanation |

**Action or cognition?** Is the reader doing, or thinking?

**Acquisition or application?** Is the reader studying, or working?

A reader studying while doing is learning, and needs a tutorial. A reader working while doing has a goal, and needs a how-to guide. A reader working while thinking needs a fact, and needs reference. A reader studying while thinking wants to understand, and needs explanation.

## One mode per document

This is the rule that does the work. Everything else is detail.

A document serving two modes serves neither. The reader following steps does not want a paragraph on design history. The reader trying to understand a design choice does not want to be told to run a command. Each interrupts the other.

When a document is doing two things, split it and link the parts. Splitting is almost always the right answer, and it is almost always resisted, because the material feels related. Related is not the same as belonging together.

## The four modes

| Mode | Serves | The reader is | Title form | Fails by |
| --- | --- | --- | --- | --- |
| Tutorial | Learning by doing | New, needs confidence | `Getting started with x` | Explaining instead of doing |
| How-to guide | Achieving a goal | Competent, has a problem | `How to x` | Teaching instead of directing |
| Reference | Describing the machinery | Working, needs a fact | The name of the thing | Advising, or explaining |
| Explanation | Understanding | Reflecting, not at a keyboard | `About x` | Instructing |

Depth for each mode is in `references/tutorials.md`, `references/how-to-guides.md`, `references/reference-material.md`, and `references/explanation.md`. Read the one you are writing.

**Titles carry the mode.** A reader scanning a list of documents should be able to tell what each one will do for them. `How to configure caching` promises directions. `About caching` promises understanding. `Caching` promises neither and delivers whichever the author felt like.

## Prose

Diátaxis decides what a document is. It says nothing about the sentences. These rules apply in every mode:

- **Address the reader as `you`.** Use `we` only in a tutorial, where it carries the relationship between teacher and learner.
- **Write in the present tense.** The command does not "will return" a value; it returns one.
- **Use the active voice**, so it is clear what performs the action. "The server rejects the request" beats "the request is rejected".
- **Put the condition before the instruction.** "To enable caching, set the flag" and not "set the flag to enable caching". A reader who does not want caching stops reading at the comma.
- **Sentence case for titles and headings.** Consistent, and easier to read.
- **Describe where a link goes.** Never `click here` or `this page`. Link text is read out of context by people scanning, and by screen readers listing every link on a page.
- **Never say above, below, or on the left.** Position changes with the display and disappears in translation. Say `earlier`, `following`, or name the section.

Fuller rules on style, formatting, and accessibility are in `references/style.md`.

## Code, commands, and output

In developer documentation the code is often most of the page, and it is the part the reader copies. Four rules do most of the work:

- **Use the names a real user would type**, spelled out: a template called `python-service`, not `tpl`. No throwaway values such as `x`.
- **Put a sentence before every block.** Two blocks never touch, and output is introduced before it appears.
- **Files the reader writes, they write in their editor.** Say which file and where, then show a block titled with its path. Never `printf`, a heredoc, or `sed -i` for a file the reader is meant to write.
- **Show only the flags and output the reader needs.** Flags for running without a person stay in scripts; output appears when it confirms a step or holds something to notice.

Read `references/code-and-commands.md` for any page that contains code, commands, or output. It has the rest, with detection tests.

## Every page is the first page

Readers arrive from a search engine, not from your table of contents. Any page may be the first one someone sees, so no page can rely on being read after another.

Every page states what it is about in its first sentence, names its assumptions, and links to what a reader needs next. This does not mean repeating other pages. It means never leaving a reader stranded because they entered in the middle.

## When a script checks the page

A page replayed by a test script is more trustworthy than one that is not, and the script is still not the page. The page is written for a person at a keyboard; the script adapts to it, never the reverse.

- The script runs the page's commands and writes the page's files exactly as shown. It may assert more than the page shows.
- Nothing appears on the page only because the script needs it: flags for running without a terminal, a check after every step, a setup script in place of the previous page, shell edits in place of the reader's editor.
- Where the tooling allows, keep the files the reader writes in one place that both the page and the script use, such as an include. A copy kept by hand drifts.
- Run the script along every path the page offers into it. A page that says "continue from the previous tutorial" is tested from where that tutorial ends, not only from an empty directory.

## When not to write documentation

Do not write a page that restates what the code already says. It will drift, and a wrong document is worse than a missing one because it is believed.

Do not start a frequently asked questions page. It grows by accretion, mixes all four modes, and its structure records the order questions arrived rather than anything useful to a reader.

Do not document a thing you intend to change this week.

## Improving documentation that already exists

Do not plan a restructure. Diátaxis is applied by asking what one thing could be improved now, doing it, and asking again. A large reorganisation delays every benefit until it is finished, and it usually is not.

`references/improving-existing-docs.md` covers diagnosing a document that mixes modes, and splitting it safely.

## Before you finish

- The document is one mode, and you can say which.
- The title tells a reader which mode it is.
- Nothing in it belongs to another mode.
- It states what it is about in its first sentence.
- A reader arriving cold is not stranded.
- Every instruction puts its condition first.
- Every link says where it goes.
- Nothing depends on position on the page or on colour.
- Every claim is true of the current version, and you checked rather than assumed.
- Every example name is one a real user would type, spelled out, and none is a throwaway.
- Every code block has a sentence before it, and no command carries a flag the reader does not need.
- Every file the reader writes is an editor step that names the file.
- If a script checks the page, it ran along every path into the page, and nothing is on the page only for the script.

## Sources

- Diátaxis, by Daniele Procida, at `diataxis.fr`, for the four modes and the compass.
- The Google developer documentation style guide, for prose, formatting, and accessibility.
- Write the Docs, for documentation kept beside the code and reviewed with it.
- *Every Page Is Page One*, Mark Baker, for readers who arrive by search.
- *Docs for Developers*, Bhatti and others, for treating documentation as an engineering responsibility.
- The Kubernetes and MDN code style guides, for example names with meaning.
- DigitalOcean's technical writing guidelines, for files written in the reader's editor and every command introduced.
