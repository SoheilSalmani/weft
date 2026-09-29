# Archetypes

The archetype changes the answer more than any other input. Establish it before generating.

| Archetype | Weighted heavily | Weighted lightly | Distinctive constraint |
| --- | --- | --- | --- |
| Company, umbrella | Extensibility, trademark strength, domain | Typing ergonomics | Must contain products that do not exist yet |
| Platform | Broad resonance, extensibility, trademark | CLI ergonomics | Cannot be named after its first feature |
| Product, app | Memorability, pronunciation, emotional fit | Registry namespaces | May be use-case specific |
| Developer tool | Search ownership, registry, speech in docs | Consumer resonance | Must survive GitHub search |
| **CLI** | Typing effort, shell collision, lowercase | Emotional tone, `.com` | Must not shadow a real binary |
| Library, package | Registry availability and normalisation, imports | Domain, social handles | Normalisation collisions |
| SDK | Relationship to the parent brand | Independent identity | Usually `Parent SDK`, not a new brand |
| **Protocol, standard** | Descriptive credibility, stability | Brand distinctiveness | Descriptive is often correct here |
| Open-source project | GitHub searchability, registry | Trademark depth | Community pronounceability |
| Internal codename | Memorability | Everything else | Deliberately disposable |

Two rows are exceptions worth remembering. **Protocols** are frequently better off boring and descriptive, where brand distinctiveness is a liability. **SDKs** rarely deserve their own brand; inventing one fragments the parent.

## CLI specifics

A beautiful platform name can be a terrible command. Test the actual typing:

```
name init
name run
name deploy
name inspect
```

Check: length and typing effort, collision with shell built-ins and common Unix commands, whether the lowercase form reads well, whether hyphens are needed (they usually hurt), and whether the name survives tab completion ambiguity.

**Brand and executable need not be identical.** `Brand` with executable `br` is legitimate when coherence survives. Separating them is better than forcing an eight-syllable brand into a command, and better than picking a weak brand to get a good command.

Reject any candidate that shadows a real binary. That is a correctness problem, not an aesthetic one.

## Package specifics

Import ergonomics matter: read the import line and a dependency list entry aloud. Names that are fine spoken can be awkward in `from x import y`.

Scoped or namespaced fallbacks are legitimate when the bare name is taken, but a scope does not fix a name that is confusable with a popular package.

## Expansion horizon

Ask directly: **will this name be wrong if the thing succeeds?**

A tool that strips AI provenance metadata today may handle content provenance broadly later; a name encoding "watermark removal" traps it. The opposite failure is real too: an abstract evocative name on a single-purpose fifty-line CLI obscures rather than elevates.

Match abstraction to ambition, and ask about ambition rather than assuming it.

## Brand architecture

When naming into an existing family, inspect the parent first. Options: branded house (`Parent CLI`, `Parent Cloud`), endorsed (`Child by Parent`), or an independent brand.

Do not invent a standalone brand where `Parent + descriptive product` is stronger, and do not force everything under the parent when a product has its own audience and lifespan.

## Brand plus descriptor

The finalist output is a brand **and** a factual descriptor:

```
Rivet: release orchestration for monorepos
```

The separator is a convention, not a rule. A colon, a dash or a line break all
work; what matters is that the brand stands alone and the descriptor is a plain
factual sentence beside it, never fused into the name.

The brand carries identity, the descriptor carries clarity. Solving clarity inside the name pushes it toward the weak end of the distinctiveness spectrum.

Descriptors are factual and specific. Not "revolutionary AI platform for effortless workflows". The descriptor later feeds `repository-presentation`, which owns discoverability.
