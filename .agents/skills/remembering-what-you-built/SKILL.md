---
name: remembering-what-you-built
description: Decides what about a project you are building is worth remembering without looking it up, then hands those facts to anki-flashcards to write. Covers the gate for which changes deserve cards at all, and finding cards an earlier change made false. Use after a change lands that adds a command, a constraint, a term, or a decision, when asked to make flashcards about a repository or about your own project, or when a rename or a reversal may have invalidated cards you already have. For authoring cards from an article, a document, or a conversation, use anki-flashcards directly.
compatibility: Needs read access to the repository, and anki-flashcards for everything that touches the collection. Without Anki reachable it still produces the selection and says the collection was not searched.
---

# Remembering what you built

You forget your own projects. Not the ideas, which stick, but the surface: the flag you added in March, the order two things have to happen in, the word you chose for a thing and then stopped using consistently.

This skill decides **which facts are worth carrying in your head**, and hands them over. `anki-flashcards` writes them. Every rule about card quality, note types, formatting, and the approval step lives there, and is not repeated here.

**Most changes are worth no cards at all.** That is the expected answer, not a failure to find something.

## The change is the trigger, not the source

A diff says what moved on one day. A card has to be true for years.

So use the change to decide **what to look at**, then write from the thing that outlives it: the command's own help output, the README, the decision record, the constraint as it now stands in the code. A card written from a diff is a card about a day, and it starts lying at the next commit.

Read the current state before selecting anything. If the change is three commits old, the answer may already have moved again.

## Does anything here deserve a card

Take it when **at least one** holds:

- **You will have to type it unaided.** A command, a subcommand, a flag, an environment variable, a keybinding.
- **The project invented the word.** Vocabulary you must use consistently, or the code and the conversation drift apart.
- **You could break it while making a locally sensible change.** The constraint that looks arbitrary until you violate it. This is the most expensive kind of forgetting, and the cheapest to prevent.
- **It is a decision with a reason.** Otherwise the same argument returns in three months with nobody remembering it was settled.
- **It is a mechanism you cannot infer.** What triggers what, and in what order.

Leave it when any of these holds:

- **The code answers faster than recall.** Signatures, file layout, the full option list. Looking it up is the correct workflow.
- **Something loads it for you.** A rule that lives in a skill and fires when the task matches is not a rule you have to hold. Card it only where you apply it yourself, away from the agent.
- **It is still moving.** A design in flight becomes a wrong card, and a wrong card is worse than no card, because it is rehearsed.
- **A test already enforces it.** You do not need to remember what the machine checks on every run.
- **You expect to rewrite it.** Implementation detail carded is implementation detail you will have to unlearn.

`anki-flashcards` puts a whole article at five to fifteen cards. A single change is a fraction of that, and usually zero. **Say "nothing here is worth remembering" plainly, with the reason**, rather than finding something to justify the run.

## Two kinds of fact, and they need different questions

**Using it.** The surface you drive from memory. What command does this, what does this flag do, what is the default, what has to be set before it runs.

**Building it.** What holds the thing together. What may only this component do, what does this word mean here, why this instead of the obvious alternative, what happens before what.

Both are about one project, so both carry the project as their context value. Which note type expresses each is decided by `anki-flashcards`, which knows the collection; do not choose it here.

## The project is the source tag

`anki-flashcards` requires exactly one source tag per card, naming where the fact came from, so that a fact which later goes stale can be found again. For a project, that tag is the project's own slug, the same one every time.

**This is the whole reason the next section can work.** A project's cards with no shared tag cannot be swept, and an untagged card about a moving target is a card nobody will ever correct.

## Cards the change just made false

A renamed command, a flipped default, a reversed decision: the cards claiming the old thing are now teaching you something false, on a schedule, with spaced repetition making it stick.

**Sweep before writing anything new.** A rename usually means an edit, not a new card.

1. Search the collection for the project's source tag, plus the old name or the affected term.
2. Report what turned up, per note, saying what it claims and what is now true.
3. Propose the narrowest fix: **update** where the fact simply moved, **suspend** where the feature is gone but may return, **delete** only when asked.

`anki-flashcards` owns the editing mechanics, including the warning that splitting a note loses its review history. Follow it rather than reinventing it.

When Anki is unreachable, say the collection was not searched. Never report a clean sweep you did not run.

## Where this stops

This skill selects facts. It writes nothing to the collection.

Hand the selection to `anki-flashcards` with the source you read and the project slug, and let it draft, format, audit, and show its approval table. That table is the single gate before anything is added, and routing around it is the one failure that costs months of review time.

## Before you finish

- Nothing was carded from a diff. Every fact came from something that outlives the change.
- Every card taken clears one of the five tests, and the test it clears could be named.
- Nothing still in flight was carded.
- "Nothing worth remembering" was genuinely available, and was used when it was true.
- The sweep for cards the change invalidated ran before any new card was proposed.
- Every card carries the project's slug as its source tag.
- No note type, field, or formatting rule was decided here.
- Nothing reached the collection except through `anki-flashcards` and its approval table.
