# Note types in this collection

Counts taken from the live collection on 2026-08-10. "Current" excludes the `_Legacy` deck tree.
Re-check with `anki_cards.py inspect` rather than trusting these numbers indefinitely.

## Pick from these

| Note type | Fields | Current / total | Use for |
| --- | --- | --- | --- |
| `Q&A (for a tool)` | `Front`, `Tool`, `Back` | 1738 / 1819 | Software, CLIs, libraries, services, frameworks, SaaS products |
| `Cloze (for a tool)` | `Text`, `Tool` | 927 / 971 | Tool facts that read better as one sentence with a blank |
| `Q&A by topic` | `Front`, `Topic`, `Back` | 811 / 828 | Everything that is not a tool: concepts, domains, finance, org structure, science |
| `Cloze by topic` | `Text`, `Topic` | 559 / 559 | Sentence-shaped facts, acronym pairs, closed lists, numeric values in context |
| `Keyboard Shortcut` | `Front`, `Tool`, `Shortcut`, `Shortcut (macOS)`, `Shortcut (Windows)`, `Shortcut (Linux)` | 66 / 67 | Key bindings, with per-OS variants when they differ |
| `Q&A (for a license)` | `Front`, `License`, `Back` | 64 / 64 | Driving and professional licence material |
| `Cloze (for a license)` | `Text`, `License` | 25 / 25 | Same, sentence-shaped |
| `Image Occlusion (for a topic)` | `Occlusion`, `Topic`, `Image`, `Header`, `Back Extra`, `Comments` | 6 / 6 | Diagrams, maps, anatomy, UI screenshots |
| `Image Occlusion (for a tool)` | same, with `Tool` | 5 / 5 | Tool UI screenshots |
| `Q&A (for a body part)` | `Front`, `Body part`, `Back` | 11 / 11 | Anatomy and physical training |
| `Cloze (for a body part)` | `Text`, `Body part` | 4 / 4 | Same, sentence-shaped |

The first four carry 96 percent of current notes. Default to `Q&A (for a tool)` or `Q&A by topic`
unless the fact genuinely reads better as a cloze.

## Do not use for new cards

| Note type | Why |
| --- | --- |
| `Basic` | Legacy. 3773 of its 3922 notes sit in `_Legacy`. It has no context field, so authors end up prefixing the Front with the topic by hand, which is the exact mistake this skill exists to prevent. |
| `Cloze` | Legacy, 10 notes. Superseded by `Cloze by topic`. |
| `Basic (and reversed card)` | Legacy, 9 current notes. For a two-direction fact use one cloze note with `c1` and `c2` instead. |
| `Basic (optional reversed card)`, `Basic (type in the answer)`, `Basic (and reversed card) (without question in answer)`, `Alphabet` | Zero notes. Not part of this collection's practice. |
| `Language Reactor - Phrase` | Written by the Language Reactor browser extension. Do not hand-author. |
| `Recipe`, `Coding Exercises`, `Practice Activity`, `Tutorial (platform-specific)`, `Keyboard Layout`, `Vocabulary (Chinese)`, `Alphabet (Vietnamese)` | Purpose-built for one narrow workflow each. Use only when the user asks for that specific kind of note. |

## How the context field renders

This is the reason the Front must not repeat the topic. `Q&A by topic` template:

```html
Front: <p id="topic">{{Topic}}</p>
       <p>{{Front}}</p>
Back:  {{FrontSide}}
       <hr id=answer>
       {{Back}}
```

`Q&A (for a tool)` and the other scoped variants are identical with `{{Tool}}`, `{{License}}`, or
`{{Body part}}` in place of `{{Topic}}`. The cloze variants render the same context line above
`{{cloze:Text}}` on both sides of the card.

So the reviewer already sees the topic on its own styled line. A Front of
`"Sanofi R&D: which committee approves Gate 4?"` renders as:

```
R&D governance
Sanofi R&D: which committee approves Gate 4?
```

Write `"Which committee approves Gate 4?"` and let the field do its job. This is SuperMemo rule 16,
context cues simplify wording, built into the note type.

## Choosing the context value

`Topic` and `Tool` are free text, so consistency is on you. Reuse a value that already exists in the
collection instead of coining a near-duplicate. Check with:

```bash
python3 scripts/anki_cards.py topics --like "R&D"
```

Existing values are short noun phrases naming the subject, not sentences and not hierarchies:
`Causal Inference`, `Ableton Live`, `Mutuelle`, `PEA`, `Bot Detection`, `Server-sent events`,
`Music Theory`, `Ingredient`. Match that register. The value usually mirrors the leaf deck name.

## Two traps that break a batch

1. `Cloze by topic` and `Cloze (for a tool)` have no `Back Extra` field. Only `Text` and the context
   field. The MCP `modelFieldNames` tool helpfully returns an `example` object mentioning
   `Back Extra` for anything cloze-shaped, which is wrong for these two. Passing that key rejects
   the whole batch. The plain legacy `Cloze` type is the only one that has it.
2. `mcp__anki__addNotes` takes one `deckName` and one `modelName` per call, capped at 100 notes.
   A mixed set of Q&A and cloze cards therefore needs one call per model. Duplicates are skipped
   individually, but a validation error such as an empty required field or a bad tag rejects the
   entire batch, so lint before sending.

## Cloze conventions used in this collection

Taken from real notes, verbatim:

```
{{c2::FR}} est l'acronyme de {{c1::<i>Frais Réels</i>}}.
A <code>scope="module"</code> fixture defined in {{c1::conftest.py}} is {{c1::shared by all test modules}} in that directory.
In the metric system, the prefix {{c1::deci-}} means {{c2::1/10}}.
```

Three patterns worth copying:

- Reuse the same index (`c1` twice) for blanks that must be recalled together on one card.
- Use `c1` and `c2` on the two halves of a pairing to get both directions from a single note. This is
  the legitimate redundancy of SuperMemo rule 17, and it replaces the legacy reversed note type.
- Blank the whole meaningful chunk, not one word out of a phrase. `{{c1::shared by all test modules}}`
  rather than `shared by all {{c1::test modules}}`.

On index count: three is the ceiling when the blanks are separate facts sharing a sentence, because
past that the note is a bundle wearing a cloze costume. A genuine closed list is the exception, and
one index per member is correct there even at five or six. The linter warns above three either way,
so justify the warning rather than suppressing it.
