//! `weft mcp`: a Model Context Protocol server over stdio, exposing weft to
//! AI agents as typed tools.
//!
//! Guardrails, by construction:
//! - `scaffold` never runs template tasks (an agent can't trigger a
//!   template's shell actions),
//! - secrets are never accepted as answers; they resolve through their
//!   declared sources exactly like the CLI,
//! - worktree file paths are validated (relative, no escapes).

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use serde::Deserialize;
use weft_engine::interact::NonInteractive;
use weft_engine::template::Template;

fn invalid(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::invalid_params(e.to_string(), None)
}

fn internal(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

pub struct WeftMcp {
    templates_dir: Utf8PathBuf,
}

impl WeftMcp {
    pub fn new(templates_dir: Utf8PathBuf) -> Self {
        Self { templates_dir }
    }

    fn template_dir(&self, name: &str) -> Result<Utf8PathBuf, ErrorData> {
        let ok = !name.is_empty()
            && !name.starts_with('.')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !ok {
            return Err(invalid(format!("invalid template name {name:?}")));
        }
        // The templates dir may itself be a template ("." layout) or contain
        // template subdirectories.
        let dir = self.templates_dir.join(name);
        if dir.join("weft.toml").is_file() {
            Ok(dir)
        } else if name == "." && self.templates_dir.join("weft.toml").is_file() {
            Ok(self.templates_dir.clone())
        } else {
            Err(invalid(format!(
                "no template named `{name}` under {}",
                self.templates_dir
            )))
        }
    }

    fn load(&self, name: &str) -> Result<Template, ErrorData> {
        Template::load(&self.template_dir(name)?).map_err(|e| invalid(format!("{e:#}")))
    }
}

fn validate_rel_path(path: &str) -> Result<Utf8PathBuf, ErrorData> {
    let ok = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.ends_with('/')
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..");
    if !ok {
        return Err(invalid(format!("invalid worktree path {path:?}")));
    }
    Ok(Utf8PathBuf::from(path))
}

fn answers_json(answers: &BTreeMap<String, serde_json::Value>) -> Option<String> {
    if answers.is_empty() {
        None
    } else {
        serde_json::to_string(answers).ok()
    }
}

// ---- tool parameter types --------------------------------------------

#[derive(Deserialize, schemars::JsonSchema, Default)]
pub struct Empty {}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TemplateParam {
    /// Template directory name under the server's templates dir (`.` when
    /// the templates dir itself is the template).
    template: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ScaffoldParams {
    template: String,
    /// Destination directory (created; must not already contain files).
    dest: String,
    /// Answers keyed by question id (see describe_template for the schema).
    #[serde(default)]
    answers: BTreeMap<String, serde_json::Value>,
    /// Preset names to layer under the answers.
    #[serde(default)]
    presets: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RecordStartParams {
    template: String,
    #[serde(default)]
    answers: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    presets: Vec<String>,
    /// Discard an existing session on this template instead of failing.
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct FileParams {
    template: String,
    /// Worktree-relative path.
    path: String,
    /// File content (write only).
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CommitParams {
    template: String,
    /// Patch name (alphanumeric, - and _).
    name: String,
    /// What this patch does — stored as metadata, shown in describe/graph.
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    /// Optional Starlark gate for the new patch.
    #[serde(default)]
    when: Option<String>,
    /// Abstraction decisions per answer id; undecided candidates default to
    /// accepted, secret values are always abstracted.
    #[serde(default)]
    decisions: BTreeMap<String, bool>,
    /// Occurrences to keep literal, as ANSWER@PATH:LINE[:NTH].
    #[serde(default)]
    keep_literal: Vec<String>,
}

// ---- tools -------------------------------------------------------------

#[tool_router]
impl WeftMcp {
    #[tool(
        name = "list_templates",
        description = "List the weft templates this server can operate on."
    )]
    fn list_templates(
        &self,
        Parameters(_): Parameters<Empty>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut out = Vec::new();
        let mut candidates: Vec<(String, Utf8PathBuf)> =
            vec![(".".into(), self.templates_dir.clone())];
        if let Ok(entries) = self.templates_dir.read_dir_utf8() {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    candidates.push((entry.file_name().to_owned(), entry.path().to_owned()));
                }
            }
        }
        for (name, dir) in candidates {
            if !dir.join("weft.toml").is_file() {
                continue;
            }
            if let Ok(template) = Template::load(&dir) {
                out.push(serde_json::json!({
                    "template": name,
                    "name": template.manifest.template.name,
                    "description": template.manifest.template.description,
                    "questions": template.manifest.questions.len(),
                    "patches": template.patches.len(),
                    "presets": template.manifest.presets.len(),
                }));
            }
        }
        Ok(CallToolResult::structured(
            serde_json::json!({ "templates": out }),
        ))
    }

    #[tool(
        name = "describe_template",
        description = "The full contract of a template: every question with type, \
                       default preview, gate and whether it is required; presets; \
                       the patch DAG with descriptions; tasks. Read this before \
                       scaffolding or recording."
    )]
    fn describe_template(
        &self,
        Parameters(p): Parameters<TemplateParam>,
    ) -> Result<CallToolResult, ErrorData> {
        let template = self.load(&p.template)?;
        let doc = weft_engine::describe::describe(&template, &weft_engine::eval())
            .map_err(|e| internal(format!("{e:#}")))?;
        serde_json::to_value(&doc)
            .map(CallToolResult::structured)
            .map_err(internal)
    }

    #[tool(
        name = "scaffold",
        description = "Render a template into a destination directory. Template \
                       tasks are NEVER executed by this tool (run them yourself if \
                       needed). Secrets resolve from their declared sources."
    )]
    fn scaffold(
        &self,
        Parameters(p): Parameters<ScaffoldParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let template_dir = self.template_dir(&p.template)?;
        let opts = weft_engine::new::NewOptions {
            stored_ref: None,
            template: template_dir,
            dest: Utf8PathBuf::from(&p.dest),
            presets: p.presets.clone(),
            answers: vec![],
            answers_file: None,
            answers_json: answers_json(&p.answers),
            instances: vec![],
            skip_tasks: true, // guardrail: agents never run template shell tasks
        };
        weft_engine::new::run(
            &opts,
            &mut weft_engine::template::PathResolver,
            &mut NonInteractive,
        )
        .map_err(|e| invalid(format!("{e:#}")))?;
        let files = weft_engine::fsio::read_tree(&opts.dest)
            .map(|t| t.len())
            .unwrap_or(0);
        Ok(CallToolResult::structured(serde_json::json!({
            "dest": p.dest,
            "files": files,
            "note": "template tasks were not run (agent guardrail)",
        })))
    }

    #[tool(
        name = "check_template",
        description = "Validate a template: manifest, expressions, patch graph, \
                       and patch commutation. Run after any template change."
    )]
    fn check_template(
        &self,
        Parameters(p): Parameters<TemplateParam>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.template_dir(&p.template)?;
        let report = weft_engine::check::run(
            &weft_engine::check::CheckOptions {
                template: dir,
                presets: vec![],
                answers: vec![],
                answers_file: None,
            },
            &mut weft_engine::template::PathResolver,
        )
        .map_err(|e| invalid(format!("{e:#}")))?;
        Ok(CallToolResult::structured(serde_json::json!({
            "ok": report.issues.is_empty(),
            "issues": report.issues,
            "notes": report.notes,
        })))
    }

    #[tool(
        name = "record_start",
        description = "Start authoring a patch: renders the template's base state \
                       (needs a complete answer set) into a scratch worktree. Edit \
                       files with record_write_file, then record_commit. One \
                       session per template."
    )]
    fn record_start(
        &self,
        Parameters(p): Parameters<RecordStartParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let template_dir = self.template_dir(&p.template)?;
        let opts = weft_engine::record::RecordOptions {
            foreach: None,
            exec: None,
            template: template_dir,
            base: "latest".into(),
            presets: p.presets.clone(),
            answers: vec![],
            answers_file: None,
            answers_json: answers_json(&p.answers),
            force: p.force,
        };
        let worktree = weft_engine::record::run(
            &opts,
            &mut weft_engine::template::PathResolver,
            &mut NonInteractive,
        )
        .map_err(|e| invalid(format!("{e:#}")))?;
        let files: Vec<String> = weft_engine::fsio::read_tree(&worktree)
            .map(|t| t.paths().map(ToString::to_string).collect())
            .unwrap_or_default();
        Ok(CallToolResult::structured(serde_json::json!({
            "worktree": worktree.as_str(),
            "files": files,
        })))
    }

    #[tool(
        name = "record_read_file",
        description = "Read a file from the active recording worktree."
    )]
    fn record_read_file(
        &self,
        Parameters(p): Parameters<FileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.template_dir(&p.template)?;
        let rel = validate_rel_path(&p.path)?;
        let target = weft_engine::session::worktree_dir(&dir).join(&rel);
        let content = std::fs::read_to_string(&target)
            .map_err(|_| invalid(format!("no file {rel} in the worktree")))?;
        Ok(CallToolResult::structured(
            serde_json::json!({ "path": rel, "content": content }),
        ))
    }

    #[tool(
        name = "record_write_file",
        description = "Create or overwrite a file in the active recording worktree."
    )]
    fn record_write_file(
        &self,
        Parameters(p): Parameters<FileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.template_dir(&p.template)?;
        let rel = validate_rel_path(&p.path)?;
        let content = p
            .content
            .ok_or_else(|| invalid("record_write_file needs `content`"))?;
        let target = weft_engine::session::worktree_dir(&dir).join(&rel);
        if !weft_engine::session::Session::exists(&dir) {
            return Err(invalid(
                "no active recording session; call record_start first",
            ));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(internal)?;
        }
        std::fs::write(&target, content).map_err(internal)?;
        Ok(CallToolResult::structured(
            serde_json::json!({ "path": rel, "written": true }),
        ))
    }

    #[tool(
        name = "record_delete_file",
        description = "Delete a file from the active recording worktree."
    )]
    fn record_delete_file(
        &self,
        Parameters(p): Parameters<FileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.template_dir(&p.template)?;
        let rel = validate_rel_path(&p.path)?;
        let target = weft_engine::session::worktree_dir(&dir).join(&rel);
        std::fs::remove_file(&target)
            .map_err(|_| invalid(format!("no file {rel} in the worktree")))?;
        Ok(CallToolResult::structured(
            serde_json::json!({ "path": rel, "deleted": true }),
        ))
    }

    #[tool(
        name = "record_commit",
        description = "Diff the recording worktree against its base, abstract \
                       answer values (your decisions per answer id; undecided = \
                       accepted, secrets always abstracted), validate by replay, \
                       and append the patch to the template. Give it a clear \
                       description — that metadata is how future agents understand \
                       the patch."
    )]
    fn record_commit(
        &self,
        Parameters(p): Parameters<CommitParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.template_dir(&p.template)?;
        let opts = weft_engine::commit::CommitOptions {
            template: dir,
            name: Some(p.name.clone()),
            when: p.when.clone(),
            title: None,
            describe: p.description.clone(),
            tags: p.tags.clone(),
            decisions: Some(p.decisions.clone()),
            keep_literal: p.keep_literal.clone(),
            link: None,
        };
        weft_engine::commit::run(
            &opts,
            &mut weft_engine::template::PathResolver,
            &mut NonInteractive,
        )
        .map_err(|e| invalid(format!("{e:#}")))?;
        Ok(CallToolResult::structured(
            serde_json::json!({ "patch": p.name, "committed": true }),
        ))
    }

    #[tool(
        name = "record_discard",
        description = "Discard the active recording session and its worktree."
    )]
    fn record_discard(
        &self,
        Parameters(p): Parameters<TemplateParam>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.template_dir(&p.template)?;
        weft_engine::session::Session::discard(&dir).map_err(|e| internal(format!("{e:#}")))?;
        Ok(CallToolResult::structured(
            serde_json::json!({ "discarded": true }),
        ))
    }
}

#[tool_handler]
impl ServerHandler for WeftMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "weft is a record-based project scaffolding engine. Typical flows: \
             (1) scaffold a project: list_templates → describe_template (read \
             the questions: `required` ones must be supplied, secrets resolve \
             from env/cmd sources) → scaffold. (2) author a template patch: \
             record_start with concrete answers → record_write_file with \
             concrete values (weft abstracts them into answer references at \
             commit) → record_commit with a description → check_template.",
        )
    }
}

/// Run the stdio MCP server (blocks until the client disconnects).
pub fn serve(templates_dir: Utf8PathBuf) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let service = WeftMcp::new(templates_dir)
            .serve(rmcp::transport::stdio())
            .await?;
        service.waiting().await?;
        Ok(())
    })
}
