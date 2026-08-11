"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const { EventEmitter } = require("node:events");
const { PassThrough } = require("node:stream");
const test = require("node:test");

const { runProtocol } = require("../src/runner");

test("spawns the exact machine protocol and removes its private request", async () => {
  const request = {
    protocol_version: 1,
    action: "minimize",
    root: "/work/project",
    output: "/work/minimal",
  };
  let requestPath;
  let observedRequest;
  const events = [];

  const completed = await runProtocol({
    binary: "reprocut-custom",
    request,
    onEvent: (event) => events.push(event.type),
    spawnProcess(binary, arguments_, options) {
      assert.equal(binary, "reprocut-custom");
      assert.deepEqual(arguments_.slice(0, 3), ["protocol", "run", "--request"]);
      assert.equal(options.shell, false);
      requestPath = arguments_[3];
      observedRequest = JSON.parse(fs.readFileSync(requestPath, "utf8"));

      const child = new EventEmitter();
      child.stdout = new PassThrough();
      child.stderr = new PassThrough();
      child.kill = () => true;
      setImmediate(() => {
        for (const event of [
          { type: "started", protocol_version: 1, action: "minimize", root: request.root },
          {
            type: "baseline_stable",
            protocol_version: 1,
            fingerprint_sha256: "b".repeat(64),
          },
          {
            type: "completed",
            protocol_version: 1,
            output: request.output,
            evidence: `${request.output}/reduction.json`,
            report: `${request.output}/report.html`,
            issue: `${request.output}/issue.md`,
          },
        ]) {
          child.stdout.write(`${JSON.stringify(event)}\n`);
        }
        child.stdout.end();
        child.stderr.end();
        child.emit("close", 0, null);
      });
      return child;
    },
  });

  assert.deepEqual(observedRequest, request);
  assert.deepEqual(events, ["started", "baseline_stable", "completed"]);
  assert.equal(completed.type, "completed");
  assert.equal(fs.existsSync(requestPath), false);
});
