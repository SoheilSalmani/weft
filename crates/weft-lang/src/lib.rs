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
use starlark::values::Value as SlValue;
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
        for (id, value) in answers.iter() {
            let sl: SlValue = match value {
                Value::String(s) => module.heap().alloc(s.as_str()),
                Value::Bool(b) => SlValue::new_bool(*b),
                Value::Int(i) => module.heap().alloc(*i),
                // Secrets are opaque to the expression language.
                Value::Secret(_) => continue,
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

fn to_core_value(v: SlValue) -> Result<Value, EvalError> {
    if let Some(b) = v.unpack_bool() {
        Ok(Value::Bool(b))
    } else if let Some(i) = v.unpack_i32() {
        Ok(Value::Int(i64::from(i)))
    } else if let Some(s) = v.unpack_str() {
        Ok(Value::String(s.to_owned()))
    } else {
        Err(EvalError {
            message: format!(
                "expression produced a {} value; expected string, bool, or int",
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
    fn secrets_are_not_visible() {
        let mut a = AnswerSet::new();
        a.insert(
            "token".into(),
            Value::Secret(weft_core::SecretValue::new("hunter2".into())),
        );
        assert!(eval_expr("token", &a).is_err());
    }
}
