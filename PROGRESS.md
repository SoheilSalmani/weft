# Progress log

Running notes per milestone, as required by PLAN.md. Records decisions and
deviations from the plan.

## M2 — Deterministic render

- `weft_core::render` implements `resolve_answers` (when-gating in question
  declaration order, provided-value type checks, Starlark defaults) and
  `render(patches, answers, eval) -> Tree`.
- **Core stays Starlark-free:** core defines an `ExprEval` trait; `weft-lang`
  implements it (`StarlarkEval`). Core tests use a stub evaluator; the
  golden-tree tests live in `weft-lang` where the real evaluator is available.
- **Determinism:** patches apply in topological order with ties broken by
  patch id, so the input ordering of the patch list never matters (covered by
  a test that reverses the input and compares tree hashes).
- **`when`-skipped patches skip their dependents transitively** rather than
  erroring; a dependent's ops usually target files the skipped patch created.
- Hunk application requires the rendered pattern (`context_before + removed +
  context_after`) to match at exactly one position: zero matches and multiple
  matches are distinct, clean errors. A fully-empty pattern means "append at
  end of file".
- Rendered paths are validated: relative, `/`-separated, no `..`/`.`/empty
  components — answer values cannot escape the destination tree.
- `starlark` 0.14 changed module construction to a scoped
  `Module::with_temp_heap` API; evaluation happens inside that scope.
  Secrets are never injected into the Starlark environment.

## M1 — Data model + serialization

- All plan types implemented in `weft-core` with serde round-trips.
- **Canonical form:** compact JSON with fixed struct key order; a patch's id
  is blake3 of the canonical body (`depends_on` sorted dep *ids*, `when`,
  `ops`) — the id itself is never inside the hashed bytes, so ids form a
  Merkle DAG.
- **Hand-writable patch files:** `Segment`/`Line`/`TemplatePath` serialize
  fully-literal values as plain JSON strings and abstracted values as
  `{"answer": id}` / `{"expr": src}` objects. Fixture patch files can be
  written by hand; ids are recomputed on load (files don't embed their id).
- **Content model decision:** text-only (UTF-8) for MVP; rendered files are
  normalized to end with exactly one trailing newline (empty files stay
  empty). Binary files and exact-trailing-newline preservation are post-MVP.
- Secrets: `Value::Secret` fails serialization with an explicit error and is
  redacted in `Debug`; only `SecretSpec` references are serializable.
- `deny_unknown_fields` can't be combined with the flattened `kind` on
  `Question` (serde limitation) — stray-key validation deferred to
  `weft check`.
- Property tests (proptest): serialize → deserialize → serialize is
  byte-identical for ops and canonical patch bodies. Insta snapshots pin the
  canonical forms and a fixture patch id.

## M0 — Skeleton

- Cargo workspace with `weft-core`, `weft-engine`, `weft-lang`, `weft-cli`,
  plus `tests/e2e` as a workspace member package (integration tests via
  `assert_cmd`; run with `cargo test --workspace` so the `weft` binary is
  built first).
- **Deviation:** `starlark = "0.13"` (the version current when the plan was
  written) no longer compiles — its `allocative` dependency drifted against
  newer `hashbrown`. Pinned `starlark = "0.14"` instead; same API surface for
  our use.
- The reference design docs mentioned in PLAN.md (`weft-mvp-design.md`,
  `weft-record-mvp.md`) are not present in the repo; PLAN.md is the sole
  source of truth.
- CI gate is `cargo fmt --check && cargo clippy --workspace --all-targets
  -- -D warnings && cargo test --workspace`. All green.
