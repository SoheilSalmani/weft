//! Weft lang: Starlark evaluation for conditions, defaults, and derived
//! values. This is the only crate that touches the Starlark interpreter.
//!
//! Answers are injected as module-level variables named by their `AnswerId`;
//! secret values are deliberately *not* injected, so expressions can never
//! observe or copy a secret.

#![forbid(unsafe_code)]

use starlark::environment::{Globals, Module};
use starlark::eval::Evaluator;
use starlark::syntax::{AstModule, Dialect};
use starlark::values::list::{AllocList, ListRef};
use starlark::values::{Heap, Value as SlValue};
use weft_core::render::{EvalError, ExprEval};
use weft_core::{AnswerSet, StarlarkExpr, Value};

/// Real Starlark-backed implementation of `weft_core::render::ExprEval`.
#[derive(Default, Clone, Copy)]
pub struct StarlarkEval;

impl ExprEval for StarlarkEval {
    fn eval(&self, expr: &StarlarkExpr, answers: &AnswerSet) -> Result<Value, EvalError> {
        eval_expr(expr.as_str(), answers)
    }
}

/// Parse-check an expression without evaluating it (used by `weft check`).
pub fn parse_expr(expr: &str) -> Result<(), EvalError> {
    AstModule::parse("<expr>", expr.to_owned(), &Dialect::Standard)
        .map(|_| ())
        .map_err(|e| EvalError {
            message: e.to_string(),
        })
}

pub fn eval_expr(expr: &str, answers: &AnswerSet) -> Result<Value, EvalError> {
    let ast =
        AstModule::parse("<expr>", expr.to_owned(), &Dialect::Standard).map_err(|e| EvalError {
            message: format!("parse error: {e}"),
        })?;
    let globals = Globals::standard();
    Module::with_temp_heap(|module| {
        let heap = module.heap();
        for (id, value) in answers.iter() {
            let Some(sl) = to_sl_value(value, heap) else {
                // Secrets (and lists containing them) are opaque to the
                // expression language.
                continue;
            };
            module.set(&id.0, sl);
        }
        let mut evaluator = Evaluator::new(&module);
        let result = evaluator
            .eval_module(ast, &globals)
            .map_err(|e| EvalError {
                message: e.to_string(),
            })?;
        to_core_value(result)
    })
}

/// Marshal a core `Value` into the heap. Returns `None` for secrets (and any
/// list transitively containing one), which must never enter the interpreter.
fn to_sl_value<'v>(value: &Value, heap: Heap<'v>) -> Option<SlValue<'v>> {
    match value {
        Value::String(s) => Some(heap.alloc(s.as_str())),
        Value::Bool(b) => Some(SlValue::new_bool(*b)),
        Value::Int(i) => Some(heap.alloc(*i)),
        Value::List(items) => {
            let elems: Option<Vec<SlValue>> =
                items.iter().map(|it| to_sl_value(it, heap)).collect();
            Some(heap.alloc(AllocList(elems?)))
        }
        Value::Secret(_) => None,
    }
}

fn to_core_value(v: SlValue) -> Result<Value, EvalError> {
    if let Some(b) = v.unpack_bool() {
        Ok(Value::Bool(b))
    } else if let Some(i) = v.unpack_i32() {
        Ok(Value::Int(i64::from(i)))
    } else if let Some(s) = v.unpack_str() {
        Ok(Value::String(s.to_owned()))
    } else if let Some(list) = ListRef::from_value(v) {
        let items: Result<Vec<Value>, EvalError> = list.iter().map(to_core_value).collect();
        Ok(Value::List(items?))
    } else {
        Err(EvalError {
            message: format!(
                "expression produced a {} value; expected string, bool, int, or list",
                v.get_type()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use weft_core::render::ExprEval;
    use weft_core::AnswerId;

    fn answers(pairs: &[(&str, Value)]) -> AnswerSet {
        pairs
            .iter()
            .map(|(k, v)| (AnswerId::from(*k), v.clone()))
            .collect()
    }

    #[test]
    fn evaluates_literals_and_references() {
        let a = answers(&[
            ("name", Value::String("api".into())),
            ("workers", Value::Int(4)),
        ]);
        assert_eq!(eval_expr("True", &a).unwrap(), Value::Bool(true));
        assert_eq!(eval_expr("workers * 2", &a).unwrap(), Value::Int(8));
        assert_eq!(
            eval_expr("name + '-service'", &a).unwrap(),
            Value::String("api-service".into())
        );
    }

    #[test]
    fn condition_truthiness() {
        let a = answers(&[("use_docker", Value::Bool(true))]);
        assert!(StarlarkEval
            .eval_bool(&StarlarkExpr::from("use_docker"), &a)
            .unwrap());
        assert!(!StarlarkEval
            .eval_bool(&StarlarkExpr::from("not use_docker"), &a)
            .unwrap());
    }

    #[test]
    fn derived_default_referencing_other_answer() {
        let a = answers(&[("project_name", Value::String("My App".into()))]);
        assert_eq!(
            eval_expr("project_name.lower().replace(' ', '-')", &a).unwrap(),
            Value::String("my-app".into())
        );
    }

    #[test]
    fn unknown_name_is_an_error() {
        assert!(eval_expr("nope", &AnswerSet::new()).is_err());
    }

    #[test]
    fn list_membership_and_projection() {
        let fonts = Value::List(vec![
            Value::String("besley".into()),
            Value::String("nunito".into()),
        ]);
        let a = answers(&[("selected_fonts", fonts)]);
        assert!(StarlarkEval
            .eval_bool(&StarlarkExpr::from("'besley' in selected_fonts"), &a)
            .unwrap());
        assert!(!StarlarkEval
            .eval_bool(&StarlarkExpr::from("'plein' in selected_fonts"), &a)
            .unwrap());
        assert_eq!(
            eval_expr("' '.join(selected_fonts)", &a).unwrap(),
            Value::String("besley nunito".into())
        );
        assert_eq!(
            eval_expr("[f for f in selected_fonts if f != 'nunito']", &a).unwrap(),
            Value::List(vec![Value::String("besley".into())])
        );
    }

    #[test]
    fn list_default_built_from_scalars() {
        let a = answers(&[
            ("font_ui", Value::String("figtree".into())),
            ("font_heading", Value::String("besley".into())),
        ]);
        assert_eq!(
            eval_expr("[font_ui, font_heading]", &a).unwrap(),
            Value::List(vec![
                Value::String("figtree".into()),
                Value::String("besley".into()),
            ])
        );
    }

    #[test]
    fn task_command_interpolates_a_joined_list() {
        use weft_core::render::render_segments;
        use weft_core::segment::Segment;
        let a = answers(&[(
            "components",
            Value::List(vec![
                Value::String("button".into()),
                Value::String("card".into()),
            ]),
        )]);
        let segs = vec![
            Segment::Literal("shadcn add ".into()),
            Segment::Expr(StarlarkExpr::from("' '.join(components)")),
        ];
        assert_eq!(
            render_segments(&segs, &a, &StarlarkEval).unwrap(),
            "shadcn add button card"
        );
    }

    #[test]
    fn gated_off_question_default_keeps_name_defined() {
        use weft_core::render::resolve_answers;
        use weft_core::{AnswerKind, Question, StarlarkExpr};
        let q = |id: &str, default: &str, when: Option<&str>| Question {
            id: id.into(),
            kind: AnswerKind::String,
            prompt: None,
            description: None,
            example: None,
            default: Some(StarlarkExpr::from(default)),
            when: when.map(StarlarkExpr::from),
            computed: false,
            section: None,
        };
        let questions = vec![
            Question {
                id: "use_ui".into(),
                kind: AnswerKind::Bool,
                prompt: None,
                description: None,
                example: None,
                default: Some(StarlarkExpr::from("False")),
                when: None,
                computed: false,
                section: None,
            },
            // gated off (use_ui is False) but still defaults to 'none'
            q("registries", "'none'", Some("use_ui")),
            // references `registries` unconditionally — Starlark binds names
            // eagerly, so this only works because `registries` is defined.
            q("label", "registries + '-x'", None),
        ];
        let resolved = resolve_answers(&questions, &AnswerSet::new(), &StarlarkEval).unwrap();
        assert_eq!(
            resolved.get(&AnswerId::from("registries")),
            Some(&Value::String("none".into()))
        );
        assert_eq!(
            resolved.get(&AnswerId::from("label")),
            Some(&Value::String("none-x".into()))
        );
    }

    #[test]
    fn secrets_are_not_visible() {
        let mut a = AnswerSet::new();
        a.insert(
            "token".into(),
            Value::Secret(weft_core::SecretValue::new("hunter2".into())),
        );
        assert!(eval_expr("token", &a).is_err());
    }
}
