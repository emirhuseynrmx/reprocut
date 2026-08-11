"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs/promises");
const path = require("node:path");

const vscode = require("vscode");
const { PROTOCOL_VERSION } = require("./protocol");
const { runProtocol } = require("./runner");

let lastResult;

function activate(context) {
  const run = (action, uri) => runFromEditor(context, action, uri);
  context.subscriptions.push(
    vscode.commands.registerCommand("reprocut.minimize", (uri) => run("minimize", uri)),
    vscode.commands.registerCommand("reprocut.resume", (uri) => run("resume", uri)),
    vscode.commands.registerCommand("reprocut.openReport", () => openLast("report")),
    vscode.commands.registerCommand("reprocut.openIssue", () => openLast("issue")),
    vscode.commands.registerCommand("reprocut.openReducedProject", () => openProject()),
  );
}

async function runFromEditor(context, action, uri) {
  const root = await resolveRoot(uri);
  if (!root) return;
  const configuration = vscode.workspace.getConfiguration("reprocut", vscode.Uri.file(root));
  const binary = configuration.get("binary", "reprocut");
  const output = path.join(path.dirname(root), `${path.basename(root)}-reprocut`);
  await fs.mkdir(context.globalStorageUri.fsPath, { recursive: true });
  const stateName = crypto.createHash("sha256").update(root).digest("hex");
  const state = path.join(context.globalStorageUri.fsPath, `${stateName}.sqlite3`);
  const request = {
    protocol_version: PROTOCOL_VERSION,
    action,
    root,
    output,
    ecosystem: configuration.get("ecosystem", "auto"),
    preparation: configuration.get("preparation", "offline"),
    command: configuration.get("command", []),
    jobs: configuration.get("jobs", 0),
    state,
  };

  try {
    lastResult = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: action === "resume" ? "Resuming ReproCut" : "Minimizing failure with ReproCut",
        cancellable: true,
      },
      (progress, cancellation) =>
        runProtocol({
          binary,
          request,
          cancellation,
          onEvent: (event) => updateProgress(progress, event),
        }),
    );
    const selection = await vscode.window.showInformationMessage(
      "ReproCut preserved the stabilized failure.",
      "Open report",
      "Open issue",
      "Open project",
    );
    if (selection === "Open report") await openLast("report");
    if (selection === "Open issue") await openLast("issue");
    if (selection === "Open project") await openProject();
  } catch (error) {
    const suffix = error?.code === "ENOENT" ? " Install it with: cargo install reprocut" : "";
    await vscode.window.showErrorMessage(`ReproCut failed: ${error.message}${suffix}`);
  }
}

async function resolveRoot(uri) {
  if (uri?.scheme === "file") {
    const metadata = await fs.stat(uri.fsPath);
    return metadata.isDirectory() ? uri.fsPath : path.dirname(uri.fsPath);
  }
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (folder) return folder.uri.fsPath;
  await vscode.window.showErrorMessage("Open a local project folder before running ReproCut.");
  return undefined;
}

function updateProgress(progress, event) {
  if (event.type === "started") progress.report({ message: "Stabilizing the failure…" });
  if (event.type === "baseline_stable") {
    progress.report({ message: `Same-failure fingerprint ${event.fingerprint_sha256.slice(0, 12)}…` });
  }
}

async function openLast(field) {
  if (!lastResult) {
    await vscode.window.showInformationMessage("Run ReproCut first in this editor session.");
    return;
  }
  await vscode.env.openExternal(vscode.Uri.file(lastResult[field]));
}

async function openProject() {
  if (!lastResult) {
    await vscode.window.showInformationMessage("Run ReproCut first in this editor session.");
    return;
  }
  await vscode.commands.executeCommand(
    "vscode.openFolder",
    vscode.Uri.file(path.join(lastResult.output, "project")),
    { forceNewWindow: true },
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
