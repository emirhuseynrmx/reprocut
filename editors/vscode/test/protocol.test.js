"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

const { PROTOCOL_VERSION, ProtocolError, createEventValidator } = require("../src/protocol");

const request = Object.freeze({
  protocol_version: PROTOCOL_VERSION,
  action: "minimize",
  root: "/work/project",
  output: "/work/minimal",
});

test("accepts exactly one ordered protocol lifecycle", () => {
  const validate = createEventValidator(request);
  assert.equal(
    validate({
      type: "started",
      protocol_version: 1,
      action: "minimize",
      root: "/work/project",
    }).type,
    "started",
  );
  assert.equal(
    validate({
      type: "baseline_stable",
      protocol_version: 1,
      fingerprint_sha256: "a".repeat(64),
    }).type,
    "baseline_stable",
  );
  assert.equal(
    validate({
      type: "completed",
      protocol_version: 1,
      output: "/work/minimal",
      evidence: "/work/minimal/reduction.json",
      report: "/work/minimal/report.html",
      issue: "/work/minimal/issue.md",
    }).type,
    "completed",
  );
});

test("rejects incompatible, reordered, and mismatched events", () => {
  assert.throws(
    () =>
      createEventValidator(request)({
        type: "started",
        protocol_version: 2,
        action: "minimize",
        root: "/work/project",
      }),
    ProtocolError,
  );
  assert.throws(
    () =>
      createEventValidator(request)({
        type: "completed",
        protocol_version: 1,
        output: "/work/minimal",
        evidence: "x",
        report: "x",
        issue: "x",
      }),
    /before started/,
  );
  assert.throws(
    () =>
      createEventValidator(request)({
        type: "started",
        protocol_version: 1,
        action: "resume",
        root: "/work/project",
      }),
    /action does not match/,
  );
});

test("a failed event is terminal and keeps its safe message", () => {
  const validate = createEventValidator(request);
  const failed = validate({
    type: "failed",
    protocol_version: 1,
    message: "baseline command succeeded",
  });
  assert.equal(failed.message, "baseline command succeeded");
  assert.throws(() => validate(failed), /after terminal/);
});
