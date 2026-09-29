---
name: choosing-what-to-build
description: Turns a set of ideas into an ordered shortlist of what to build next. Sizes each one from evidence in the code, works out which ideas unblock others, and separates quick wins from long hauls. Asks the owner for value and urgency rather than inventing a priority. Use when asked what to build next, which ideas are quick wins, how big a piece of work is, or to turn a project's ideas into a plan. Not for creating the tracker items afterwards, which work-orchestration owns.
compatibility: Needs read access to the repository being sized, and to the ideation vault when the ideas live there. Sizes only what it can read, and says what it could not.
---

# Choosing what to build next

Ideation stops at a pile of ideas. Execution starts once something is chosen. **This skill is the crossing, and nothing else owns it.**

It takes a set of ideas and returns an ordered shortlist. It creates nothing.

## Where this sits

`idea-garden` holds the ideas and hands them off only when the user says so. `work-orchestration` decides which artefacts a chosen piece of work needs, and says ideation is not work. Neither of them picks.

In: a set of ideas, usually a project table in the vault. Out: a shortlist with the evidence behind each size. Then stop.

## What you may derive, and what you may not

| Derived from evidence | The owner's to give |
| --- | --- |
| How big it is | How much it matters |
| What it unblocks | How urgent it is |
| What it risks breaking | What a deadline demands |
| Whether anything can test it | What they want to be working on |

Everything in the left column cites what was read. Everything in the right column is asked for, or left out. `tracking-work-in-linear` refuses to invent a priority for exactly this reason: a guess wearing a number stops looking like a guess.

**Never emit one blended score.** A single number hides which half of it was evidence. Keep the two apart, and the owner can overrule the order without unpicking arithmetic.

## Sizing from evidence

Read the code an idea would touch before saying anything about its size, and name the files you read.

Bigger than it looks:

- A new variant in a data model that other code matches on exhaustively.
- Anything that changes a content hash, an id, or a stored format someone else has already written to disk.
- A migration for artefacts that are already out in the world.
- Work that has to land in more than one repository at once.
- Nothing exists that could test it.

Smaller than it looks:

- Purely additive, with existing output unchanged byte for byte.
- One module, one seam.
- A command that composes calls the engine already exposes.
- A suite already covers the behaviour around it.

**An idea you cannot size says so.** "I could not size this without reading how X works" is a useful answer. A number pulled from the air is not.

## What unblocks what

Prerequisite order is a fact about the ideas rather than a preference, and it usually decides more than size does. Take it from the idea documents and confirm it in the code.

A quick win that unblocks a long haul beats a quick win that unblocks nothing. Say which one it is, because that is the part the owner cannot see from the table.

## The shortlist

About five. A ranked list of thirty is a backlog, and the garden already refuses to be one.

Give each: the idea, its size and the evidence, what it unblocks, and what would make it not worth doing. Then a recommended order, and one line on why the first is first.

## Ask rather than score

One or two questions, and only where the answer changes the order. "These two cost the same and this one unblocks three others, so it goes first unless the other is what you actually want to be using next week."

If the owner has already said what matters to them, use that and do not ask again.

## Handing off

Only once they pick. Give `work-orchestration` the idea document and the project, and let it choose the artefacts. The row in the garden becomes `building` then, and not before.

Never create an issue here. Never set a priority field. Never write a rank into the vault, because status lives in the project table and nowhere else.

## Before you finish

- Every size cites a file that was actually read.
- No output mixes evidence with the owner's judgement.
- Anything that could not be sized is named as such.
- The shortlist is short.
- Nothing was created, and no vault row moved.
