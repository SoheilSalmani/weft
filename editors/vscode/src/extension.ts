// Weft VSCode extension: LSP client for `weft lsp` plus a "New Project"
// command driven entirely by the `weft describe --json` contract.

import { execFile } from "child_process";
import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function weftPath(): string {
  return vscode.workspace.getConfiguration("weft").get<string>("path") ?? "weft";
}

function run(args: string[], cwd?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(weftPath(), args, { cwd, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) reject(new Error(stderr.trim() || err.message));
      else resolve(stdout);
    });
  });
}

interface Question {
  id: string;
  kind: "string" | "bool" | "int" | "choice" | "secret";
  choices?: string[];
  prompt?: string;
  description?: string;
  example?: string;
  default_preview?: string;
  when?: string;
  required: boolean;
}

interface DescribeDoc {
  template: { name: string; description?: string };
  questions: Question[];
  presets: { name: string }[];
}

async function askQuestion(q: Question): Promise<string | number | boolean | undefined> {
  const title = q.prompt ?? q.id;
  const detail = [q.description, q.when ? `(only relevant when ${q.when})` : ""]
    .filter(Boolean)
    .join(" ");
  if (q.kind === "choice") {
    const pick = await vscode.window.showQuickPick(q.choices ?? [], {
      title: `${title} — ${q.id}`,
      placeHolder: detail,
    });
    return pick;
  }
  if (q.kind === "bool") {
    const pick = await vscode.window.showQuickPick(["true", "false"], {
      title: `${title} — ${q.id}`,
      placeHolder: detail,
    });
    return pick === undefined ? undefined : pick === "true";
  }
  const raw = await vscode.window.showInputBox({
    title: `${title} — ${q.id}`,
    prompt: detail,
    placeHolder: q.example ?? q.default_preview ?? "",
    validateInput: (value) =>
      q.kind === "int" && value !== "" && !/^-?\d+$/.test(value)
        ? "must be an integer"
        : undefined,
  });
  if (raw === undefined) return undefined;
  return q.kind === "int" ? Number(raw) : raw;
}

async function newProject(): Promise<void> {
  const templatePick = await vscode.window.showOpenDialog({
    canSelectFolders: true,
    canSelectFiles: false,
    title: "Select a weft template directory (contains weft.toml)",
  });
  const template = templatePick?.[0]?.fsPath;
  if (!template) return;

  let doc: DescribeDoc;
  try {
    doc = JSON.parse(await run(["describe", template, "--json"])) as DescribeDoc;
  } catch (e) {
    void vscode.window.showErrorMessage(`weft describe failed: ${e}`);
    return;
  }

  const answers: Record<string, string | number | boolean> = {};
  for (const q of doc.questions) {
    if (q.kind === "secret" || !q.required) continue;
    const value = await askQuestion(q);
    if (value === undefined) return; // cancelled
    answers[q.id] = value;
  }

  const preset =
    doc.presets.length > 0
      ? await vscode.window.showQuickPick(["(none)", ...doc.presets.map((p) => p.name)], {
          title: "Apply a preset?",
        })
      : "(none)";
  if (preset === undefined) return;

  const parentPick = await vscode.window.showOpenDialog({
    canSelectFolders: true,
    canSelectFiles: false,
    title: "Select the parent directory for the new project",
  });
  const parent = parentPick?.[0]?.fsPath;
  if (!parent) return;
  const name = await vscode.window.showInputBox({
    title: "New project directory name",
    value: String(answers["project_name"] ?? doc.template.name)
      .toLowerCase()
      .replace(/[^a-z0-9-_]+/g, "-"),
  });
  if (!name) return;
  const dest = path.join(parent, name);

  const args = ["new", template, dest, "--answers-json", JSON.stringify(answers), "--non-interactive"];
  if (preset !== "(none)") args.push("--preset", preset!);

  await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Notification, title: `weft: scaffolding ${name}…` },
    async () => {
      try {
        await run(args);
        const open = await vscode.window.showInformationMessage(
          `Scaffolded ${name}`,
          "Open Folder",
        );
        if (open === "Open Folder") {
          await vscode.commands.executeCommand(
            "vscode.openFolder",
            vscode.Uri.file(dest),
            { forceNewWindow: true },
          );
        }
      } catch (e) {
        void vscode.window.showErrorMessage(`weft new failed: ${e}`);
      }
    },
  );
}

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("weft.newProject", newProject),
  );

  const serverOptions: ServerOptions = {
    command: weftPath(),
    args: ["lsp"],
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { pattern: "**/weft.toml" },
      { pattern: "**/patches/*.json" },
    ],
  };
  client = new LanguageClient("weft", "Weft Language Server", serverOptions, clientOptions);
  client.start().catch((e) => {
    void vscode.window.showWarningMessage(
      `weft lsp not started (${e}); install weft or set weft.path`,
    );
  });
  context.subscriptions.push({ dispose: () => void client?.stop() });
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
