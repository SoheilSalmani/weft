use anyhow::{bail, Context, Result};
use weft_core::{Question, SecretSpec, SecretValue};

use crate::interact::Interaction;

/// Resolve a secret reference to its value, at render time only. The value
/// never touches disk: `weft_core::Value::Secret` refuses serialization.
pub fn resolve(
    question: &Question,
    spec: &SecretSpec,
    interaction: &mut dyn Interaction,
) -> Result<SecretValue> {
    match spec {
        SecretSpec::Env(var) => {
            let value = std::env::var(var).with_context(|| {
                format!(
                    "secret `{}` reads environment variable `{var}`, which is not set",
                    question.id
                )
            })?;
            Ok(SecretValue::new(value))
        }
        SecretSpec::Cmd(cmd) => {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .with_context(|| format!("running secret command for `{}`", question.id))?;
            if !output.status.success() {
                bail!(
                    "secret command for `{}` exited with {}: {}",
                    question.id,
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
            let value = String::from_utf8(output.stdout)
                .with_context(|| {
                    format!("secret command for `{}` produced non-UTF-8", question.id)
                })?
                .trim_end_matches('\n')
                .to_owned();
            Ok(SecretValue::new(value))
        }
        SecretSpec::Prompt => interaction.ask_secret(question),
    }
}
