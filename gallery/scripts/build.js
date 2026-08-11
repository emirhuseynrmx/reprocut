"use strict";

const fs = require("node:fs");
const path = require("node:path");

const ALLOWED_FIELDS = new Set([
  "schema_version",
  "slug",
  "title",
  "license",
  "ecosystem",
  "fingerprint_sha256",
  "termination",
  "original_files",
  "retained_files",
  "original_bytes",
  "retained_bytes",
  "source_included",
  "featured",
]);
const SLUG = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const SHA256 = /^[0-9a-f]{64}$/;
const SPDX = /^[A-Za-z0-9+.()/: -]+$/;
const ECOSYSTEMS = new Set(["cargo", "python", "npm", "none"]);
const SECRET_PATTERNS = [
  /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/,
  /\bAKIA[0-9A-Z]{16}\b/,
  /\bgh[oprsu]_[A-Za-z0-9_]{20,}\b/,
  /\bgithub_pat_[A-Za-z0-9_]{20,}\b/,
  /\b(?:password|passwd|api[_-]?key|secret|token)\s*[:=]\s*["']?[^\s"']{12,}/i,
];
const MAX_ENTRIES = 200;
const MAX_FILE_BYTES = 1024 * 1024;
const MAX_ENTRY_BYTES = 4 * 1024 * 1024;

function validateEntries(entriesRoot) {
  const root = path.resolve(entriesRoot);
  const rootMetadata = fs.lstatSync(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) {
    throw new Error("gallery entries root must be a real directory");
  }
  const names = fs
    .readdirSync(root)
    .filter((name) => !name.startsWith("."))
    .sort((left, right) => left.localeCompare(right, "en"));
  if (names.length > MAX_ENTRIES) throw new Error(`gallery exceeds ${MAX_ENTRIES} entries`);

  const seen = new Set();
  return names.map((name) => {
    const directory = path.join(root, name);
    const metadata = fs.lstatSync(directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error(`entry ${name} is not a real directory`);
    }
    scanSubmission(directory);
    const entryPath = path.join(directory, "entry.json");
    const licensePath = path.join(directory, "LICENSE_DECLARATION.md");
    if (!fs.existsSync(entryPath) || !fs.existsSync(licensePath)) {
      throw new Error(`entry ${name} requires entry.json and LICENSE_DECLARATION.md`);
    }
    const entry = JSON.parse(fs.readFileSync(entryPath, "utf8"));
    validateEntry(entry, name);
    if (seen.has(entry.slug)) throw new Error(`duplicate gallery slug: ${entry.slug}`);
    seen.add(entry.slug);
    const license = fs.readFileSync(licensePath, "utf8");
    if (!license.includes(`\`${entry.license}\``)) {
      throw new Error(`license declaration does not match entry ${name}`);
    }
    const hasSource = fs.existsSync(path.join(directory, "source"));
    if (hasSource !== entry.source_included) {
      throw new Error(`source_included does not match files for entry ${name}`);
    }
    return Object.freeze({ ...entry });
  });
}

function validateEntry(entry, directoryName) {
  if (entry === null || Array.isArray(entry) || typeof entry !== "object") {
    throw new Error(`entry ${directoryName} must be an object`);
  }
  for (const field of Object.keys(entry)) {
    if (!ALLOWED_FIELDS.has(field)) throw new Error(`unknown field ${field} in ${directoryName}`);
  }
  for (const field of ALLOWED_FIELDS) {
    if (!(field in entry)) throw new Error(`missing field ${field} in ${directoryName}`);
  }
  if (entry.schema_version !== 1) throw new Error("unsupported gallery schema");
  if (!SLUG.test(entry.slug) || entry.slug.length > 64 || entry.slug !== directoryName) {
    throw new Error(`entry slug must match directory ${directoryName}`);
  }
  boundedText(entry.title, "title", 100);
  boundedText(entry.termination, "termination", 80);
  boundedText(entry.license, "license", 100);
  if (!SPDX.test(entry.license)) throw new Error("license is not a bounded SPDX expression");
  if (!ECOSYSTEMS.has(entry.ecosystem)) throw new Error("unsupported ecosystem");
  if (!SHA256.test(entry.fingerprint_sha256)) throw new Error("invalid fingerprint SHA-256");
  for (const field of ["original_files", "retained_files", "original_bytes", "retained_bytes"]) {
    if (!Number.isSafeInteger(entry[field]) || entry[field] < 1) {
      throw new Error(`${field} must be a positive safe integer`);
    }
  }
  if (entry.retained_files > entry.original_files || entry.retained_bytes > entry.original_bytes) {
    throw new Error("retained measurements cannot exceed original measurements");
  }
  if (typeof entry.source_included !== "boolean" || typeof entry.featured !== "boolean") {
    throw new Error("source_included and featured must be booleans");
  }
}

function boundedText(value, field, maxLength) {
  if (
    typeof value !== "string" ||
    value.trim() !== value ||
    value.length === 0 ||
    [...value].length > maxLength ||
    /[\u0000-\u001f\u007f]/.test(value)
  ) {
    throw new Error(`${field} must be a bounded printable string`);
  }
}

function scanSubmission(directory) {
  let total = 0;
  const pending = [directory];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const name of fs.readdirSync(current).sort()) {
      const candidate = path.join(current, name);
      const metadata = fs.lstatSync(candidate);
      if (metadata.isSymbolicLink()) throw new Error(`symbolic link is forbidden: ${candidate}`);
      if (metadata.isDirectory()) {
        pending.push(candidate);
        continue;
      }
      if (!metadata.isFile()) throw new Error(`non-regular gallery member: ${candidate}`);
      if (metadata.size > MAX_FILE_BYTES) throw new Error(`gallery file exceeds 1 MiB: ${candidate}`);
      total += metadata.size;
      if (total > MAX_ENTRY_BYTES) throw new Error("gallery entry exceeds 4 MiB");
      const contents = fs.readFileSync(candidate).toString("utf8");
      if (SECRET_PATTERNS.some((pattern) => pattern.test(contents))) {
        throw new Error(`potential secret detected in ${candidate}`);
      }
    }
  }
}

function buildGallery(entriesRoot, outputRoot) {
  const entries = validateEntries(entriesRoot);
  const output = path.resolve(outputRoot);
  if (fs.existsSync(output) && fs.lstatSync(output).isSymbolicLink()) {
    throw new Error("gallery output cannot be a symbolic link");
  }
  fs.mkdirSync(output, { recursive: true });
  const cards = entries.map(renderCard).join("\n");
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ReproCut Gallery</title><style>
:root{color-scheme:dark}*{box-sizing:border-box}body{margin:0;background:#090c10;color:#e8edf3;font:16px/1.5 system-ui,sans-serif}main{max-width:1080px;margin:auto;padding:72px 24px}h1{font-size:clamp(42px,8vw,88px);letter-spacing:-.06em;margin:0 0 12px}.lede{color:#9aa7b5;max-width:62ch}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:18px;margin-top:48px}article{border:1px solid #28313c;border-radius:18px;background:#10151c;padding:22px}small{color:#7ee787;font-weight:700}code{font-size:12px;color:#a9b7c6}.metrics{display:flex;gap:24px}.metrics strong{display:block;font-size:22px}footer{margin-top:52px;color:#718096}
</style></head><body><main><small>CURATED · STATIC · NO TRACKING</small><h1>Failures, cut down.</h1><p class="lede">Opt-in minimal reproductions reviewed through pull requests. Gallery CI validates metadata and scans files; it never executes submitted code.</p><section class="grid">${cards}</section><footer>${entries.length} reviewed reproduction${entries.length === 1 ? "" : "s"} · ReproCut 0.1</footer></main></body></html>\n`;
  const temporary = path.join(output, `.index.html.${process.pid}.tmp`);
  fs.writeFileSync(temporary, html, { encoding: "utf8", flag: "wx" });
  fs.renameSync(temporary, path.join(output, "index.html"));
  return entries;
}

function renderCard(entry) {
  return `<article><small>${escapeHtml(entry.ecosystem.toUpperCase())}${entry.featured ? " · REPRO OF THE WEEK" : ""}</small><h2>${escapeHtml(entry.title)}</h2><code>${escapeHtml(entry.fingerprint_sha256.slice(0, 16))}…</code><div class="metrics"><p><strong>${entry.original_files} → ${entry.retained_files}</strong>files</p><p><strong>${entry.original_bytes} → ${entry.retained_bytes}</strong>bytes</p></div><p>${escapeHtml(entry.termination)} · ${escapeHtml(entry.license)}</p></article>`;
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character]);
}

if (require.main === module) {
  const root = path.join(__dirname, "..");
  const entries = process.argv[2] ?? path.join(root, "entries");
  const output = process.argv[3] ?? path.join(root, "dist");
  const records = buildGallery(entries, output);
  process.stdout.write(`built ${records.length} curated gallery entries at ${output}\n`);
}

module.exports = { buildGallery, validateEntries };
