"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const root = path.join(__dirname, "..");

test("extension is a protocol-only VS Code and Cursor surface", () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
  const commands = manifest.contributes.commands.map((entry) => entry.command);
  assert.deepEqual(commands, [
    "reprocut.minimize",
    "reprocut.resume",
    "reprocut.openReport",
    "reprocut.openIssue",
    "reprocut.openReducedProject",
  ]);
  assert.equal(manifest.version, "0.1.0");
  assert.equal(manifest.private, true);

  const sources = fs
    .readdirSync(path.join(root, "src"), { recursive: true })
    .filter((entry) => entry.endsWith(".js"))
    .map((entry) => fs.readFileSync(path.join(root, "src", entry), "utf8"))
    .join("\n");
  assert.match(sources, /protocol run/);
  assert.doesNotMatch(sources, /\bddmin\b|delta.?debug|reduce_hierarchical/i);
});
