"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const { buildGallery, validateEntries } = require("../scripts/build");

const root = path.join(__dirname, "..");

test("validates curated entries and builds a dependency-free static page", () => {
  const entries = path.join(root, "entries");
  const output = fs.mkdtempSync(path.join(os.tmpdir(), "reprocut-gallery-test-"));
  try {
    const records = validateEntries(entries);
    assert.equal(records.length, 1);
    buildGallery(entries, output);
    const html = fs.readFileSync(path.join(output, "index.html"), "utf8");
    assert.match(html, /Decimal checkout type mismatch/);
    assert.match(html, /18 → 3/);
    assert.doesNotMatch(html, /https?:\/\/|<script|<iframe/i);
  } finally {
    fs.rmSync(output, { recursive: true, force: true });
  }
});

test("rejects secrets, unlicensed entries, path mismatches, and unknown fields", () => {
  const entries = fs.mkdtempSync(path.join(os.tmpdir(), "reprocut-gallery-invalid-"));
  const entryRoot = path.join(entries, "safe-slug");
  fs.mkdirSync(entryRoot);
  const valid = JSON.parse(
    fs.readFileSync(
      path.join(root, "entries", "decimal-checkout-type-mismatch", "entry.json"),
      "utf8",
    ),
  );
  valid.slug = "different-slug";
  valid.unknown = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
  fs.writeFileSync(path.join(entryRoot, "entry.json"), JSON.stringify(valid));
  try {
    assert.throws(() => validateEntries(entries), /unknown field|slug/i);
  } finally {
    fs.rmSync(entries, { recursive: true, force: true });
  }
});

test("secret scanner rejects a credential even in an otherwise ignored text file", () => {
  const entries = fs.mkdtempSync(path.join(os.tmpdir(), "reprocut-gallery-secret-"));
  const source = path.join(root, "entries", "decimal-checkout-type-mismatch");
  const target = path.join(entries, "decimal-checkout-type-mismatch");
  fs.cpSync(source, target, { recursive: true });
  fs.writeFileSync(path.join(target, "notes.txt"), "AWS key AKIAABCDEFGHIJKLMNOP\n");
  try {
    assert.throws(() => validateEntries(entries), /potential secret/i);
  } finally {
    fs.rmSync(entries, { recursive: true, force: true });
  }
});
