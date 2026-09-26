---
name: idea-garden
description: Keeps ideas as plain Markdown in the user's Obsidian vault, grouped into project folders, with one short document per idea and one table per project holding every status. Matches a new thought against the ideas that already exist, across every project, before writing another. Reopens a rejected idea when evidence contradicts the reason it was rejected. Names material that is not an idea, such as a bug or a task already decided, and offers to route it rather than filing it. Use when the user offers a thought to keep, asks to process the inbox, asks to build ideation from a repository that already exists, asks what ideas they have or what has been built, wants an idea updated, revived, or dropped, or drops an idea during other work. Not when "idea" is idiomatic, as in "no idea why this fails". For work already decided, and for creating the tracker items, issues or pull requests it needs, use work-orchestration.
compatibility: Needs read and write access to the user's Obsidian vault, through direct filesystem access, a filesystem MCP server, or an Obsidian MCP server. Degrades to returning the text when none is available.
---

# Idea garden

The user drops thoughts in. **The agent files them.**

Every idea belongs to a project. There is no other kind.

## Layout

Ideation lives in the user's Obsidian vault, the directory that contains `.obsidian/`. Find it before writing anything. **Never create these files in a code repository or the working directory.** If no vault is found, say so and return the text rather than choosing a location.

```
Ideation/
  Inbox/                  anything the user writes. No rules
  Projects/
    Heddle/
      Heddle.md           the project, with a table of its ideas
      Ideas/
        <idea>.md         one idea
```

No frontmatter. No database files. **The document named after its folder is the project document.**

**These are the only files.** No source briefs, no copy of the original text, no index, no README. Material that is not an idea, such as a schema, a milestone plan, or a bug someone has already decided to fix, has no home here. Name it, then **offer it to `work-orchestration`**, which picks between a tracker item, an issue, a pull request, documentation, and nothing. **Route it only on a yes.** Noticing that something belongs elsewhere is your judgement; filing it is the user's.

## Where things belong

| What you have | Where it goes |
| --- | --- |
| A thought about what could be built | Ideation |
| Work you have decided to do | Linear, through `work-orchestration` |
| The change itself, under review | The pull request |
| A choice that constrains future work | A decision record |
| An idea you gave up on | Ideation, marked `rejected` with the reason |
| An idea that shipped | Ideation, marked `built` with what shipped |

**Ideation is everything before deciding to build.** After that, `work-orchestration`
owns it.

Two rules that get broken most:

- **Never update an idea to track progress.** Progress lives in Linear. An idea that
  mirrors a tracker is a second backlog. Sharpening an idea is not progress: one is the
  idea getting better, the other is a report on work already under way.
- **Always update an idea when it is dropped.** Mark it `rejected` and say why, or dead
  ideas look alive forever.

## How to write

Ideas are read fast or not at all. What makes one hard to read is usually how many concepts it carries, not how long its sentences are.

- **One idea is one independently useful change.** If two parts could be built, rejected, or discussed separately, they are two ideas.
- **Extract the idea, not the source prose.** A repository, an issue, a pasted conversation: all of it is evidence. Rewrite it into the simplest accurate explanation.
- **The title names the concrete thing.** The mechanism, capability, rule, document, or state someone would choose to have. "Claim labels for technical issues", not "Issues that mark what is fact and what is a guess". Not the person with the problem, and not a description of the source. A table of ideas is read as a menu, so a row that names nobody's deliverable is a row nobody can choose.
- **The first sentence says what the thing does and where.** `[Thing] [does something] [in some context].` "Technical issues label uncertain claims by type", not "A shape for issues where claims are marked". No history, motivation, or architecture in the opener.
- **The problem names one observable failure and its consequence.** What goes wrong, and what follows from it. Not the architecture, and not a tour of the workflow before the failure appears.
- **Never inventory what exists.** An idea proposes, it does not report. No counts of what is already written, no "it already builds", no sizes, no address for the existing work. The problem names the gap, and `## What shipped` is the only place that describes what is there.
- **Keep supporting mechanisms out.** A second independently useful mechanism is another idea, not a paragraph in this one.
- **Short sentences**, one point each, about twenty words.
- **Plain words.** "Mixes up", not "conflates". "Text", not "serialisation format".
- Explain a technical word in four or five words the first time, and drop project shorthand wherever ordinary words carry the same meaning.
- Write it the way you would say it to a colleague.
- **Write for someone outside the project.** They have not seen the code, used the tool, or heard the conversation. A sentence that is accurate and tells them nothing, like "it explains itself twice", has failed even though every word is true.
- **A paragraph is one line.** Obsidian treats a single newline as a line break, so wrapped prose displays broken. Let the editor wrap it.
- **Never run a formatter over these files.** Prettier reflows prose and rewrites tables.

### The five-second test

Read only the title and the first sentence. Someone outside the project should answer three questions in about five seconds: what is the thing, where does it apply, and what does it do.

If any answer needs the source material or the project's history, rewrite it. **The title and the first sentence carry the comprehension**, not the sections under them.

### Splitting ideas

An idea document is the smallest independently understandable thing worth remembering from the source. It is not a summary of a README section, an issue, or a decision record.

Two concepts are two ideas when any of these holds:

- One could ship without the other.
- One could be rejected without rejecting the other.
- They solve different problems.
- Someone could reasonably want to discuss one without the other.
- They would have different useful titles.

Steps and details that only explain how one mechanism works stay together. **Split on separate choices, never on structure**: a bullet list, a set of files, or three named components is not three ideas.

## Project document

```markdown
One sentence saying what this is.

## Problem

One paragraph, two or three sentences. What is wrong today, and for whom.

## Ideas

| Idea | Status | What it is |
| --- | --- | --- |
| [[Typed object channel]] | exploring | Programs send data on a side channel |
| [[Renderer registry]] | building | Picks which viewer shows each kind of data |
| [[Typed exit codes]] | built | Commands say why they failed, not just that they did |
```

The table is the index. **Status lives only there**, so there is one place to change it.

## Idea document

```markdown
One sentence saying what this is.

## Problem

One paragraph, two or three sentences.

## How it might work

- Three to five short bullets, each a sentence.
```

**No heading at the top.** The filename is the title, so repeating it wastes the first line.

**An idea you cannot picture is not an idea yet.** When no title names a deliverable, the idea is vague, not the name. Say so and ask, rather than filing it behind a better sentence.

**The first sentence must work on its own**, because the filename is the only title.

Bullets are sentences: a capital at the start, a full stop at the end. A second sentence is fine when a point needs it. **Needing a third means it is prose, or it is two bullets.**

`## Problem` is required. An idea with no problem is a solution looking for one, and saying so beats making one up. Drop `## How it might work` when there is nothing solid. **Never leave an empty heading.** Under about twenty-five lines.

## Status

`exploring`, `parked`, `rejected`, `building`, `built`.

A `rejected` idea needs `## Why rejected` in its document, one or two lines. Say what was learned: the limit that killed it, or the thing that proved false. A rejection with no reason is just a deletion.

A `built` idea needs `## What shipped` in its document, a short paragraph. **Describe the capability or rule that now exists**, and say where the result differed from the idea. Name a component only where it helps someone outside understand what exists; a history of how it was built is not what shipped. An idea marked built with nothing under it is just a tick, and one line saying "done" is the same tick with more words. Several shipped mechanisms that are independently useful are several built ideas.

**A rejection can be undone by evidence, never by enthusiasm.** When something contradicts the recorded reason, because the limit is gone or the thing that proved false is now true, move the row back to `exploring` and say in the document what changed. Keep the old `## Why rejected` underneath, with a line saying when it stopped applying, because the rejection was real and its reasoning is the most useful thing in the file. Wanting it again is not evidence, and neither is time passing.

`rejected` and `built` are the endings, and each is written when the work is over. `built` is final. A rejection can be reopened, on evidence and nothing else. Anything in between is progress, and progress lives in Linear.

Drop an idea while building? Mark it then. Ideas nobody mentions again look alive forever.

Asked what has shipped, read the project tables and answer from them. **Write no index.** The tables are the index, and a second copy is stale the moment a row changes.

## Match before you write

A new thought is usually about an idea you already have. **Search the whole vault before writing a document**, not only the project you had in mind, because a thought lands in the project you were not thinking about often enough to be worth the read.

Four outcomes:

- **It sharpens an idea that exists.** Add to that document rather than starting another. `## Everything can be edited` has the rules for touching one.
- **It touches several ideas.** Update each one. A note that changes three ideas across two projects is three edits, not a new document.
- **It contradicts the reason an idea was rejected.** See `## Status`.
- **Nothing covers it.** Write a new document.

**Adding nothing is a result.** When the idea already says it, say so and leave the document alone. A note that restates an idea is not a sharpening, and padding a document with it makes the idea longer without making it better.

Say which of the four happened, per idea, in the reply.

A near-duplicate is what this prevents, and it is invisible once it exists: two documents, two rows, and neither one complete.

## Inbox

Takes anything. A sentence, a link, a whole pasted conversation. **Never impose a shape on it.**

To process a file:

1. Read it. **Separate the ideas from everything else**, before deciding anything about projects. A file often holds both, and a bug, a task, or a decision already taken is not an idea however it is phrased. `## Layout` says what happens to the rest: name it, offer it to `work-orchestration`, and route it only on a yes.
2. For the ideas, work out which project they belong to: an existing folder, or a new one.
3. For a new project, **take the name the thing already has**. A repository, a tool, or a folder of personal configuration brings its own name, and a second name in the vault only makes the two disagree. Run `brand-naming` only when the project has no name and will need one outside this vault. Then create the folder and project document.
4. Match each separate idea against what exists, then write the ones nothing covers as their own document, with a row in the project's table.
5. **Delete the Inbox file** once every part of it has a home: the ideas written, and anything else routed or declined. A file holding one idea and one bug is not finished when the idea is written, and deleting it there loses the bug.
6. Report one line per idea, and one line per thing routed or offered. A file that produced no ideas still gets a report.

A long conversation usually becomes one project document plus several idea documents.

Call a **coined** name provisional **in the reply**, not in the document. It arrived unasked, so changing it should feel easy. A name taken from something that already had one is not provisional.

## Starting from a repository

Given a project that already exists as code, build its ideation from what is there.

**The project takes the repository's name.** It already names the thing, so naming it again produces two names for one project.

**The repository is evidence, not writing to preserve.** A README, an issue, a TODO, a decision record: each may use shorthand that means nothing outside the project. Translate it into plain language and keep the meaning.

1. Read the README, docs, decision records, open issues and TODO comments.
2. Collect candidates: changes, capabilities, rules, and directions.
3. **Split them with the independent-choice test before writing anything.** One section of a README is routinely several ideas, and bundling them is the failure that makes extracted ideas unreadable.
4. Give each a concrete title and a first sentence that passes the five-second test.
5. Write the project document, then one idea document per independently useful direction. On a repository you have extracted before, match first, so a second pass updates rather than duplicates.
6. Work the repository shows as finished is an idea too. Record it as `built` with `## What shipped`. This is the one place built is inferred rather than told, because a repository is evidence and a tracker is not.
7. **Only record what is actually there.** Mark an inference as one where the repository supports an idea without stating it. Do not invent a problem the repository never states.
8. Work already tracked as execution is not an idea. Leave it out.

Most of the value is in what was intended and never built, which is usually scattered
through TODOs and stale issues.

## Everything can be edited

The user edits these by hand, and that is normal.

- **Read before writing.** Never write over something from a search snippet.
- **Never reword what they wrote.** This protects what is already in the vault. Source material in a repository or an inbox file is evidence, and paraphrasing that is the job.
- Add to the right section, do not rewrite the file. Add a row, do not rebuild the table.

## Handing work off

Only when the user says so. Give `work-orchestration` the idea document and the project. **It picks what to create.** Then set the row to `building`. Tracker status never comes back.

The user says when it shipped, and only then does the row become `built` and the document gain `## What shipped`. Never read a tracker to find out, and never infer it from a merged pull request.

## Safety

- **Never delete an idea.** Park or reject it. Inbox files are the exception, once every part of them has a home.
- **Never say something was saved when it was not.** If the vault is unreachable, say so and return the text.
- Keep it local. Web research sends the question, never the notes.

## Before you finish

- The files are in the vault, not in a repository.
- Only project documents and idea documents were created.
- Every paragraph is a single line, and every `## Problem` is one paragraph.
- Short sentences, plain words, no empty headings.
- **Every idea is one independently useful concept**, and no two separate choices were bundled into one document.
- **Every title and first sentence pass the five-second test**, so an outsider can say what the idea is without opening the source.
- No idea reports the state of what it proposes, beyond the gap its problem names.
- Project shorthand was removed, or explained where it had to stay.
- Source material was paraphrased, and nothing already in the vault was reworded.
- Every idea has a one-line opener and a `## Problem`.
- Nothing was written that an existing idea already covered, and the reply says what was updated.
- Nothing was filed in another system without a yes, and nothing that was not an idea was silently dropped or forced into one.
- Anything you could not picture was flagged, not filed quietly.
- Every idea is a row in its project's table.
- Anything rejected says why, and anything built says what now exists rather than how it was built.
