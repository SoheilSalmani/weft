//! M2 acceptance: a fixture template renders byte-identically across runs
//! and regardless of patch input order; golden-tree snapshots.

use weft_core::render::render;
use weft_core::{
    AnswerId, AnswerSet, Content, Hunk, Line, Op, Patch, Segment, StarlarkExpr, TemplatePath, Tree,
    Value, DEFAULT_FILE_MODE,
};
use weft_lang::StarlarkEval;

fn answers(pairs: &[(&str, Value)]) -> AnswerSet {
    pairs
        .iter()
        .map(|(k, v)| (AnswerId::from(*k), v.clone()))
        .collect()
}

/// A small python-service-flavored fixture: base scaffold + docker patch
/// gated on `use_docker` + a patch deriving content from an expression.
fn fixture_patches() -> Vec<Patch> {
    let base = Patch::new(
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
                path: TemplatePath::literal("pyproject.toml"),
                content: Content(vec![
                    Line::literal("[project]"),
                    Line(vec![
                        Segment::Literal("name = \"".into()),
                        Segment::Expr(StarlarkExpr::from("project_name.lower().replace(' ', '-')")),
                        Segment::Literal("\"".into()),
                    ]),
                ]),
                mode: DEFAULT_FILE_MODE,
            },
        ],
    );
    let docker = Patch::new(
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
    );
    let script = Patch::new(
        vec![base.id],
        None,
        vec![
            Op::CreateFile {
                path: TemplatePath::literal("scripts/run.sh"),
                content: Content(vec![Line::literal("#!/bin/sh"), Line::literal("exec app")]),
                mode: DEFAULT_FILE_MODE,
            },
            Op::SetMode {
                path: TemplatePath::literal("scripts/run.sh"),
                mode: 0o755,
            },
        ],
    );
    vec![base, docker, script]
}

fn show_tree(tree: &Tree) -> String {
    let mut out = String::new();
    for (path, entry) in tree.iter() {
        out.push_str(&format!("=== {} (mode {:o})\n", path, entry.mode));
        out.push_str(entry.content.text().unwrap_or("<binary>"));
    }
    out
}

#[test]
fn golden_tree_with_docker() {
    let tree = render(
        &fixture_patches(),
        &answers(&[
            ("project_name", Value::String("My App".into())),
            ("use_docker", Value::Bool(true)),
        ]),
        &StarlarkEval,
    )
    .unwrap();
    insta::assert_snapshot!("tree_with_docker", show_tree(&tree));
}

#[test]
fn golden_tree_without_docker() {
    let tree = render(
        &fixture_patches(),
        &answers(&[
            ("project_name", Value::String("My App".into())),
            ("use_docker", Value::Bool(false)),
        ]),
        &StarlarkEval,
    )
    .unwrap();
    insta::assert_snapshot!("tree_without_docker", show_tree(&tree));
}

#[test]
fn render_is_deterministic_and_order_independent() {
    let a = answers(&[
        ("project_name", Value::String("My App".into())),
        ("use_docker", Value::Bool(true)),
    ]);
    let patches = fixture_patches();
    let first = render(&patches, &a, &StarlarkEval).unwrap();
    let second = render(&patches, &a, &StarlarkEval).unwrap();
    assert_eq!(
        first.hash(),
        second.hash(),
        "two runs must be byte-identical"
    );

    let mut reversed = patches.clone();
    reversed.reverse();
    let third = render(&reversed, &a, &StarlarkEval).unwrap();
    assert_eq!(
        first.hash(),
        third.hash(),
        "patch input order must not matter"
    );
}
