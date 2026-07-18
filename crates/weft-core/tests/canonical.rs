//! M1 acceptance: canonical serialization snapshots and serde round-trip
//! properties.

use proptest::prelude::*;
use weft_core::{
    Content, Hunk, Line, Op, Patch, Segment, StarlarkExpr, TemplatePath, DEFAULT_FILE_MODE,
};

fn fixture_base_patch() -> Patch {
    Patch::new(
        vec![],
        None,
        vec![
            Op::CreateFile {
                path: TemplatePath::literal("README.md"),
                content: Content(vec![
                    Line(vec![
                        Segment::Literal("# ".into()),
                        Segment::Answer("project_name".into()),
                    ]),
                    Line::literal(""),
                    Line::literal("Scaffolded by weft."),
                ]),
                mode: DEFAULT_FILE_MODE,
            },
            Op::CreateFile {
                path: TemplatePath(vec![
                    Segment::Answer("project_name".into()),
                    Segment::Literal("/__init__.py".into()),
                ]),
                content: Content(vec![]),
                mode: DEFAULT_FILE_MODE,
            },
        ],
    )
}

fn fixture_docker_patch(base: &Patch) -> Patch {
    Patch::new(
        vec![base.id],
        Some(StarlarkExpr::from("use_docker")),
        vec![
            Op::CreateFile {
                path: TemplatePath::literal("Dockerfile"),
                content: Content(vec![Line::literal("FROM python:3.12-slim")]),
                mode: DEFAULT_FILE_MODE,
            },
            Op::ModifyFile {
                path: TemplatePath::literal("README.md"),
                hunks: vec![Hunk {
                    context_before: vec![Line::literal("Scaffolded by weft.")],
                    removed: vec![],
                    added: vec![Line::literal(""), Line::literal("Ships with Docker.")],
                    context_after: vec![],
                }],
            },
        ],
    )
}

#[test]
fn canonical_form_snapshots() {
    let base = fixture_base_patch();
    let docker = fixture_docker_patch(&base);
    insta::assert_snapshot!("base_patch_id", base.id.to_hex());
    insta::assert_snapshot!("base_patch_canonical", base.canonical_json());
    insta::assert_snapshot!("docker_patch_canonical", docker.canonical_json());
}

#[test]
fn canonical_json_parses_back_to_same_id() {
    let base = fixture_base_patch();
    let v: serde_json::Value = serde_json::from_str(&base.canonical_json()).unwrap();
    let ops: Vec<Op> = serde_json::from_value(v["ops"].clone()).unwrap();
    let rebuilt = Patch::new(vec![], None, ops);
    assert_eq!(rebuilt.id, base.id);
}

// ---- property tests: serialize -> deserialize -> serialize is byte-identical

fn arb_segment() -> impl Strategy<Value = Segment> {
    prop_oneof![
        "[a-zA-Z0-9 _./#-]{0,20}".prop_map(Segment::Literal),
        "[a-z_][a-z0-9_]{0,10}".prop_map(|s| Segment::Answer(s.as_str().into())),
        "[a-z_][a-z0-9_]{0,10}".prop_map(|s| Segment::Expr(StarlarkExpr(s))),
    ]
}

fn arb_line() -> impl Strategy<Value = Line> {
    prop::collection::vec(arb_segment(), 0..4).prop_map(Line)
}

fn arb_content() -> impl Strategy<Value = Content> {
    prop::collection::vec(arb_line(), 0..6).prop_map(Content)
}

fn arb_path() -> impl Strategy<Value = TemplatePath> {
    prop_oneof![
        "[a-z0-9_/.]{1,20}".prop_map(|s| TemplatePath::literal(&s)),
        prop::collection::vec(arb_segment(), 1..3).prop_map(TemplatePath),
    ]
}

fn arb_hunk() -> impl Strategy<Value = Hunk> {
    (
        prop::collection::vec(arb_line(), 0..3),
        prop::collection::vec(arb_line(), 0..3),
        prop::collection::vec(arb_line(), 0..3),
        prop::collection::vec(arb_line(), 0..3),
    )
        .prop_map(|(context_before, removed, added, context_after)| Hunk {
            context_before,
            removed,
            added,
            context_after,
        })
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (arb_path(), arb_content()).prop_map(|(path, content)| Op::CreateFile {
            path,
            content,
            mode: DEFAULT_FILE_MODE
        }),
        (arb_path(), prop::collection::vec(arb_hunk(), 0..3))
            .prop_map(|(path, hunks)| Op::ModifyFile { path, hunks }),
        arb_path().prop_map(|path| Op::DeleteFile { path }),
        (arb_path(), arb_path()).prop_map(|(from, to)| Op::RenamePath { from, to }),
        (arb_path(), prop_oneof![Just(0o644u32), Just(0o755u32)])
            .prop_map(|(path, mode)| Op::SetMode { path, mode }),
        (arb_path(), prop::collection::vec(prop::num::u8::ANY, 0..64)).prop_map(|(path, bytes)| {
            use base64::Engine as _;
            Op::CreateBinaryFile {
                path,
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
                mode: DEFAULT_FILE_MODE,
            }
        }),
    ]
}

proptest! {
    #[test]
    fn op_serde_round_trip_is_byte_identical(op in arb_op()) {
        let s1 = serde_json::to_string(&op).unwrap();
        let back: Op = serde_json::from_str(&s1).unwrap();
        let s2 = serde_json::to_string(&back).unwrap();
        prop_assert_eq!(s1, s2);
    }

    #[test]
    fn patch_canonical_round_trip_is_byte_identical(
        ops in prop::collection::vec(arb_op(), 0..5),
        when in prop::option::of("[a-z_]{1,10}"),
    ) {
        let p = Patch::new(vec![], when.map(StarlarkExpr), ops);
        let s1 = p.canonical_json();
        let v: serde_json::Value = serde_json::from_str(&s1).unwrap();
        let ops2: Vec<Op> = serde_json::from_value(v["ops"].clone()).unwrap();
        let when2: Option<StarlarkExpr> = serde_json::from_value(v["when"].clone()).unwrap();
        let p2 = Patch::new(vec![], when2, ops2);
        prop_assert_eq!(p.canonical_json(), p2.canonical_json());
        prop_assert_eq!(p.id, p2.id);
    }
}
