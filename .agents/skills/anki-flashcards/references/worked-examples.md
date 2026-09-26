# Worked examples

Every "before" below is a real card that was added to this collection and then deleted for being bad.
They came from a Confluence page on Sanofi R&D structure. Read these to calibrate how short a good
Back is and how much gets cut.

## 1. Two facts in one card

The single most common failure. A Front with "and" in it is two cards.

Before, note type `Basic`:

```
Front: Sanofi R&D: what does the committee <b>IDCC</b> stand for, and what does it decide?
Back:  <b>Integrated Development &amp; Commercial Council</b> — <b>major investment decisions</b>.
```

Four problems: two questions in one Front, the topic duplicated in the Front, bold used as decoration,
and an em dash. After, two notes:

```
Q&A by topic
Topic: R&D governance
Front: Which committee makes major investment decisions?
Back:  The IDCC.
Tags:  confluence-rcgrwe

Cloze by topic
Topic: R&D governance
Text:  {{c2::IDCC}} stands for {{c1::Integrated Development and Commercial Council}}.
Tags:  confluence-rcgrwe
```

The cloze note gives both directions of the acronym from one note, which is SuperMemo rule 17
redundancy rather than duplication.

## 2. The topic prefix

Before:

```
Basic
Front: Sanofi R&D: within what deadline must the GPH distribute committee decisions to all core team members?
Back:  <b>7 business days.</b>
```

The `Basic` type has no context field, which is why the author reached for a prefix. Switching note
type removes the need, and the bold does nothing.

After:

```
Q&A by topic
Topic: R&D governance
Front: Within how long must the GPH distribute committee decisions to core team members?
Back:  7 business days.
Tags:  confluence-rcgrwe
```

## 3. A set that should not be a card

Before:

```
Basic
Front: Sanofi R&D: how is R&D organized at the <b>top level</b>?
Back:  Around <b>5 main functional pillars</b>, each with its own <b>Function Head</b>:<br><br>Research · Development (DEV) · CMC · TMU · Regulatory Affairs
```

Asking for five unordered members at once is SuperMemo rule 9. The order will differ every review.
The pillars are a closed list, so one cloze note gives five cards, each retrieving one member with the
others as context:

```
Cloze by topic
Topic: Sanofi R&D
Text:  The five R&D pillars, each with its own Function Head: {{c1::Research}}, {{c2::Development}}, {{c3::CMC}}, {{c4::TMU}}, {{c5::Regulatory Affairs}}.
Tags:  confluence-rcgrwe
```

Keep the count in the stem. "The five" tells the reviewer when they are done.

## 4. Interference between siblings

Before, seven cards of this shape in one batch:

```
Front: Sanofi R&D: what does the committee <b>TWG</b> stand for, and what does it decide?
Front: Sanofi R&D: what does the committee <b>RWG</b> stand for, and what does it decide?
Front: Sanofi R&D: what does the committee <b>DWG</b> stand for, and what does it decide?
...
```

TWG, RWG, DWG, TARC, IDCC, BRAC, PSC in identical wording. Under review these blur into each other,
which is the interference failure of SuperMemo rule 11. Worse, the batch also contained five reverse
cards ("which committee makes major investment decisions"), so twelve cards covered seven facts with
heavy overlap.

The fix is to card fewer of them and to differentiate by content rather than by label:

```
Q&A by topic
Topic: R&D governance
Front: Which committee decides whether a target moves from M0 to M1?
Back:  The Target Working Group.
Tags:  confluence-rcgrwe

Q&A by topic
Topic: R&D governance
Front: What decides whether a drug candidate advances between milestone gates?
Back:  Endorsement by an R&D committee. Nothing advances from M0 through to File without it.
Tags:  confluence-rcgrwe
```

The second card carries the idea that actually matters. The remaining five acronym expansions are
glossary lookups, not knowledge worth review time. Cut them, and say you cut them.

## 5. A card that answers itself

Before:

```
Front: Sanofi R&D: what does the pillar <b>CMC</b> stand for?
Back:  <b>C</b>hemistry, <b>M</b>anufacturing &amp; <b>C</b>ontrol
```

The bolded initials in the answer make it a spelling exercise, and the Front already labels CMC as a
pillar. As a plain acronym pair it is fine, but ask first whether it earns a card at all. If the user
works with CMC daily they will learn it by exposure. If they do not, they can look it up.

Kept version, if kept:

```
Cloze by topic
Topic: Sanofi R&D
Text:  {{c2::CMC}} stands for {{c1::Chemistry, Manufacturing and Control}}.
Tags:  confluence-rcgrwe
```

## 6. Binary card

A real card from the collection, kept here as a counterexample:

```
Tool:  Helm
Front: True or False: Multiple chart versions may contain the same app version.
Back:  True
```

Fifty percent recall from guessing, and it tests nothing about the relationship. Rewrite as an open
question:

```
Tool:  Helm
Front: What is the relationship between a chart version and the app version it packages?
Back:  Many-to-one. Several chart versions can package the same app version.
```

## 7. Bundled comparison that is genuinely one card

Not everything long is wrong. This card from the collection stays as it is:

```
Tool:  Vim
Front: What is the difference between a temporary setting and a permanent setting in Vim?
Back:  A temporary setting (e.g., <code>:set number</code>) is lost when Vim closes. A permanent setting is written to vimrc and persists across sessions.
```

The contrast is the fact. Splitting it into two cards would make each half ambiguous, and the two
halves would then interfere. One retrieval, two clauses.

## 8. Volatile fact needing a date stamp

The source page was generated by an AI tool and last modified in March 2026, and it describes an org
structure. Both facts belong in the card:

```
Q&A by topic
Topic: Sanofi R&D
Front: How many functional pillars was R&D organised around as of March 2026?
Back:  Five, each with its own Function Head.
Tags:  confluence-rcgrwe
```

Without "as of March 2026" this card silently becomes wrong after the next reorganisation, and there
is no way to notice during review.

## What the whole set should have looked like

The original run produced 27 notes and 34 cards from that page. A disciplined pass yields roughly
10 to 12 notes: the GPH role and its two hard obligations, the GPT and its purpose, the project
manager analogy, the pillar list as one cloze note, the milestone gate mechanism, the matrix
explanation, and two or three committee facts chosen for what they decide. Everything else on that
page is a glossary, and glossaries are for looking things up.
