//! `weft lsp`: a language server for template authoring — live `weft check`
//! diagnostics, answer-id completion, hover with question/patch metadata,
//! and go-to-definition between answer references and their questions.
//!
//! Position features are line-heuristic (find the identifier under the
//! cursor, decide from the surrounding line whether it's an answer or a
//! dependency context); precise span tracking is a later refinement.

use std::collections::HashMap;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use weft_core::render::ExprEval;
use weft_engine::template::Template;

pub struct Backend {
    client: Client,
    /// Open-document contents (uri -> text), so features work on unsaved
    /// edits.
    docs: Arc<Mutex<HashMap<Url, String>>>,
}

fn url_to_path(url: &Url) -> Option<Utf8PathBuf> {
    url.to_file_path()
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
}

fn path_to_url(path: &Utf8Path) -> Option<Url> {
    Url::from_file_path(path.as_std_path()).ok()
}

/// Walk up from a file until a directory containing weft.toml.
fn template_root(file: &Utf8Path) -> Option<Utf8PathBuf> {
    let mut dir = file.parent()?;
    loop {
        if dir.join("weft.toml").is_file() {
            return Some(dir.to_owned());
        }
        dir = dir.parent()?;
    }
}

/// The identifier (alnum + `_` + `-`) around a position in a line.
fn identifier_at(line: &str, character: usize) -> Option<(String, usize, usize)> {
    let bytes = line.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    let pos = character.min(bytes.len());
    let mut start = pos;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = pos;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some((line[start..end].to_owned(), start, end))
}

/// First line in `text` containing `needle`, as an LSP range over the match.
fn find_range(text: &str, needle: &str) -> Option<Range> {
    for (i, line) in text.lines().enumerate() {
        if let Some(col) = line.find(needle) {
            return Some(Range {
                start: Position::new(i as u32, col as u32),
                end: Position::new(i as u32, (col + needle.len()) as u32),
            });
        }
    }
    None
}

fn read_file(docs: &HashMap<Url, String>, path: &Utf8Path) -> Option<String> {
    if let Some(url) = path_to_url(path) {
        if let Some(text) = docs.get(&url) {
            return Some(text.clone());
        }
    }
    std::fs::read_to_string(path).ok()
}

/// Backtick-quoted tokens in a check issue ("patch `x` references `y`").
fn quoted_tokens(issue: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = issue;
    while let Some(start) = rest.find('`') {
        let tail = &rest[start + 1..];
        let Some(end) = tail.find('`') else { break };
        out.push(tail[..end].to_owned());
        rest = &tail[end + 1..];
    }
    out
}

impl Backend {
    /// Run `weft check` on the file's template and publish diagnostics to
    /// the files each issue points at.
    async fn refresh_diagnostics(&self, changed: &Url) {
        let Some(file) = url_to_path(changed) else {
            return;
        };
        let Some(root) = template_root(&file) else {
            return;
        };
        let docs = self.docs.lock().await;

        // Collect issues: template load failure is a single issue on the
        // changed file; otherwise run the full structural check.
        let issues: Vec<String> = match Template::load(&root) {
            Err(e) => vec![format!("{e:#}")],
            Ok(_) => match weft_engine::check::run(&weft_engine::check::CheckOptions {
                template: root.clone(),
                presets: vec![],
                answers: vec![],
                answers_file: None,
            }) {
                Ok(report) => report.issues,
                Err(e) => vec![format!("{e:#}")],
            },
        };

        // Group diagnostics per file. Every previously-known template file
        // gets an (possibly empty) publish so stale diagnostics clear.
        let mut per_file: HashMap<Utf8PathBuf, Vec<Diagnostic>> = HashMap::new();
        per_file.entry(file.clone()).or_default();
        per_file.entry(root.join("weft.toml")).or_default();
        if let Ok(entries) = root.join("patches").read_dir_utf8() {
            for entry in entries.flatten() {
                if entry.path().extension() == Some("json") {
                    per_file.entry(entry.path().to_owned()).or_default();
                }
            }
        }

        for issue in &issues {
            let tokens = quoted_tokens(issue);
            // Locate the issue: a mentioned patch name places it in that
            // patch file; otherwise weft.toml; the range anchors on the
            // first mentioned token found in the file.
            let mut target = root.join("weft.toml");
            for token in &tokens {
                let candidate = root.join("patches").join(format!("{token}.json"));
                if candidate.is_file() && issue.contains(&format!("patch `{token}`")) {
                    target = candidate;
                    break;
                }
            }
            let text = read_file(&docs, &target).unwrap_or_default();
            let range = tokens
                .iter()
                .find_map(|t| find_range(&text, t))
                .unwrap_or_default();
            per_file.entry(target).or_default().push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("weft".into()),
                message: issue.clone(),
                ..Default::default()
            });
        }
        drop(docs);

        for (path, diagnostics) in per_file {
            if let Some(url) = path_to_url(&path) {
                self.client
                    .publish_diagnostics(url, diagnostics, None)
                    .await;
            }
        }
    }

    async fn line_at(&self, url: &Url, position: Position) -> Option<(String, Utf8PathBuf)> {
        let path = url_to_path(url)?;
        let docs = self.docs.lock().await;
        let text = read_file(&docs, &path)?;
        let line = text.lines().nth(position.line as usize)?.to_owned();
        Some((line, path))
    }
}

/// Best-effort preview answers for inlay hints: walk questions in
/// declaration order preferring `example` (parsed by kind) over the
/// evaluated `default` over a kind-appropriate stand-in — so
/// `{"answer": "project_name"}` previews as its example ("My Demo") rather
/// than a generic placeholder.
fn preview_answers(template: &Template) -> weft_core::AnswerSet {
    use weft_core::{AnswerKind, Value};
    let eval = weft_engine::eval();
    let mut resolved = weft_core::AnswerSet::new();
    for q in &template.manifest.questions {
        if let Some(when) = &q.when {
            if !eval.eval_bool(when, &resolved).unwrap_or(false) {
                continue;
            }
        }
        let from_example = q.example.as_ref().and_then(|e| match &q.kind {
            AnswerKind::String => Some(Value::String(e.clone())),
            AnswerKind::Int => e.parse().ok().map(Value::Int),
            AnswerKind::Bool => match e.as_str() {
                "true" | "True" => Some(Value::Bool(true)),
                "false" | "False" => Some(Value::Bool(false)),
                _ => None,
            },
            AnswerKind::Choice { choices } => choices.contains(e).then(|| Value::String(e.clone())),
            AnswerKind::Secret { .. } => None,
        });
        let value = from_example
            .or_else(|| {
                q.default
                    .as_ref()
                    .and_then(|d| eval.eval(d, &resolved).ok())
            })
            .or_else(|| match &q.kind {
                AnswerKind::String => Some(Value::String(format!("<{}>", q.id))),
                AnswerKind::Bool => Some(Value::Bool(true)),
                AnswerKind::Int => Some(Value::Int(1)),
                AnswerKind::Choice { choices } => choices.first().cloned().map(Value::String),
                AnswerKind::Secret { .. } => None,
            });
        if let Some(v) = value {
            resolved.insert(q.id.clone(), v);
        }
    }
    resolved
}

/// Occurrences of `"key": "…"` on a line; returns (value, end-column of the
/// closing quote) pairs. Naive quote scan — fine for machine-written JSON.
fn json_string_values(line: &str, key: &str) -> Vec<(String, usize)> {
    let needle = format!("\"{key}\"");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = line[from..].find(&needle) {
        let after_key = from + at + needle.len();
        let rest = &line[after_key..];
        if let Some(colon) = rest.find(':') {
            let rest2 = &rest[colon + 1..];
            if let Some(open) = rest2.find('\"') {
                let value_start = after_key + colon + 1 + open + 1;
                if let Some(close) = line[value_start..].find('\"') {
                    let value_end = value_start + close;
                    out.push((line[value_start..value_end].to_owned(), value_end + 1));
                    from = value_end + 1;
                    continue;
                }
            }
        }
        from = after_key;
    }
    out
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "weft-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["\"".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        self.docs
            .lock()
            .await
            .insert(uri.clone(), params.text_document.text);
        self.refresh_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.docs.lock().await.insert(uri.clone(), change.text);
        }
        self.refresh_diagnostics(&uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.refresh_diagnostics(&params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.lock().await.remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;
        let Some((line, path)) = self.line_at(&uri, position).await else {
            return Ok(None);
        };
        let Some(root) = template_root(&path) else {
            return Ok(None);
        };
        let Ok(template) = Template::load(&root) else {
            return Ok(None);
        };

        let before = &line[..(position.character as usize).min(line.len())];
        let is_patch_json = path.extension() == Some("json");

        // Dependency names inside `"depends_on": [...` (patch files).
        if is_patch_json && line.contains("depends_on") {
            let items = template
                .name_to_id
                .keys()
                .map(|name| CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some("patch".into()),
                    ..Default::default()
                })
                .collect();
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // Answer ids: `"answer": "…`, `"when": "…`, `when = "…`, `default = "…`.
        let answer_context = before.contains("\"answer\"")
            || before.contains("\"when\"")
            || before.contains("when = \"")
            || before.contains("default = \"")
            || before.contains("\"expr\"");
        if answer_context {
            let items = template
                .manifest
                .questions
                .iter()
                .map(|q| CompletionItem {
                    label: q.id.0.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some(q.kind.name().to_owned()),
                    documentation: q.description.clone().map(Documentation::String),
                    ..Default::default()
                })
                .collect();
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // Question kinds in weft.toml.
        if !is_patch_json && before.contains("kind = \"") {
            let items = ["string", "bool", "int", "choice", "secret"]
                .iter()
                .map(|k| CompletionItem {
                    label: (*k).into(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    ..Default::default()
                })
                .collect();
            return Ok(Some(CompletionResponse::Array(items)));
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let Some((line, path)) = self.line_at(&uri, position).await else {
            return Ok(None);
        };
        let Some((word, _, _)) = identifier_at(&line, position.character as usize) else {
            return Ok(None);
        };
        let Some(root) = template_root(&path) else {
            return Ok(None);
        };
        let Ok(template) = Template::load(&root) else {
            return Ok(None);
        };

        // Question hover.
        if let Some(q) = template.manifest.questions.iter().find(|q| q.id.0 == word) {
            let mut md = format!("**{}** · `{}`", q.id, q.kind.name());
            if let Some(desc) = &q.description {
                md.push_str(&format!("\n\n{desc}"));
            }
            if let Some(default) = &q.default {
                md.push_str(&format!("\n\ndefault: `{}`", default.as_str()));
            }
            if let Some(when) = &q.when {
                md.push_str(&format!("\n\nasked when: `{}`", when.as_str()));
            }
            if let Some(example) = &q.example {
                md.push_str(&format!("\n\ne.g. `{example}`"));
            }
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            }));
        }

        // Patch hover (dependency references, or anywhere the name appears).
        if let Some(id) = template.name_to_id.get(&word) {
            let patch = template.patches.iter().find(|p| p.id == *id).unwrap();
            let mut md = format!("**{}** · patch `{}`", word, id.short());
            if let Some(desc) = &patch.meta.description {
                md.push_str(&format!("\n\n{desc}"));
            }
            if let Some(when) = &patch.when {
                md.push_str(&format!("\n\napplies when: `{}`", when.as_str()));
            }
            md.push_str(&format!("\n\n{} op(s)", patch.ops.len()));
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            }));
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let Some((line, path)) = self.line_at(&uri, position).await else {
            return Ok(None);
        };
        let Some((word, _, _)) = identifier_at(&line, position.character as usize) else {
            return Ok(None);
        };
        let Some(root) = template_root(&path) else {
            return Ok(None);
        };
        let Ok(template) = Template::load(&root) else {
            return Ok(None);
        };
        let docs = self.docs.lock().await;

        // Answer id -> its [[question]] in weft.toml.
        if template.manifest.questions.iter().any(|q| q.id.0 == word) {
            let manifest_path = root.join("weft.toml");
            let text = read_file(&docs, &manifest_path).unwrap_or_default();
            let range = find_range(&text, &format!("id = \"{word}\"")).unwrap_or_default();
            if let Some(url) = path_to_url(&manifest_path) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: url,
                    range,
                })));
            }
        }

        // Patch name -> its file.
        if template.name_to_id.contains_key(&word) {
            let patch_path = root.join("patches").join(format!("{word}.json"));
            if let Some(url) = path_to_url(&patch_path) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: url,
                    range: Range::default(),
                })));
            }
        }

        Ok(None)
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let Some(path) = url_to_path(&uri) else {
            return Ok(None);
        };
        let Some(root) = template_root(&path) else {
            return Ok(None);
        };
        let Ok(template) = Template::load(&root) else {
            return Ok(None);
        };
        let docs = self.docs.lock().await;
        let Some(text) = read_file(&docs, &path) else {
            return Ok(None);
        };
        drop(docs);

        let answers = preview_answers(&template);
        let eval = weft_engine::eval();
        let is_json = path.extension() == Some("json");
        let mut hints = Vec::new();

        for (line_no, line) in text.lines().enumerate() {
            let line_no = line_no as u32;
            if line_no < params.range.start.line || line_no > params.range.end.line {
                continue;
            }
            if is_json {
                // {"answer": "x"} -> the value it renders to
                for (id, end_col) in json_string_values(line, "answer") {
                    if let Some(value) = answers.get(&weft_core::AnswerId(id)) {
                        hints.push(InlayHint {
                            position: Position::new(line_no, end_col as u32),
                            label: InlayHintLabel::String(format!("⇒ {}", value.render_text())),
                            kind: None,
                            text_edits: None,
                            tooltip: Some(InlayHintTooltip::String(
                                "preview under example/default answers".into(),
                            )),
                            padding_left: Some(true),
                            padding_right: None,
                            data: None,
                        });
                    }
                }
                // {"expr": "..."} -> evaluated preview
                for (expr, end_col) in json_string_values(line, "expr") {
                    let expr = expr.replace("\\\"", "\"");
                    if let Ok(value) = eval.eval(&weft_core::StarlarkExpr(expr), &answers) {
                        hints.push(InlayHint {
                            position: Position::new(line_no, end_col as u32),
                            label: InlayHintLabel::String(format!("⇒ {}", value.render_text())),
                            kind: None,
                            text_edits: None,
                            tooltip: Some(InlayHintTooltip::String(
                                "preview under example/default answers".into(),
                            )),
                            padding_left: Some(true),
                            padding_right: None,
                            data: None,
                        });
                    }
                }
            } else {
                // weft.toml: default = "expr" -> evaluated preview
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("default = \"") {
                    if let Some(end) = rest.rfind('\"') {
                        let expr = &rest[..end];
                        if let Ok(value) =
                            eval.eval(&weft_core::StarlarkExpr(expr.to_owned()), &answers)
                        {
                            hints.push(InlayHint {
                                position: Position::new(line_no, line.len() as u32),
                                label: InlayHintLabel::String(format!("⇒ {}", value.render_text())),
                                kind: None,
                                text_edits: None,
                                tooltip: Some(InlayHintTooltip::String(
                                    "evaluated with example/default answers".into(),
                                )),
                                padding_left: Some(true),
                                padding_right: None,
                                data: None,
                            });
                        }
                    }
                }
            }
        }
        Ok(Some(hints))
    }
}

/// Run the stdio language server (blocks until the client disconnects).
pub fn serve() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
        let (service, socket) = LspService::new(|client| Backend {
            client,
            docs: Arc::new(Mutex::new(HashMap::new())),
        });
        Server::new(stdin, stdout, socket).serve(service).await;
        Ok(())
    })
}
