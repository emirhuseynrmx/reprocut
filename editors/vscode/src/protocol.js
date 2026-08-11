"use strict";

const path = require("node:path");

const PROTOCOL_VERSION = 1;
const SHA256 = /^[0-9a-f]{64}$/;

class ProtocolError extends Error {
  constructor(message) {
    super(message);
    this.name = "ProtocolError";
  }
}

function createEventValidator(request) {
  let stage = "initial";

  return (event) => {
    if (stage === "terminal") {
      throw new ProtocolError("received an event after terminal state");
    }
    requireObject(event);
    if (event.protocol_version !== PROTOCOL_VERSION) {
      throw new ProtocolError(
        `unsupported protocol version ${String(event.protocol_version)}; extension supports ${PROTOCOL_VERSION}`,
      );
    }

    if (event.type === "failed") {
      requireString(event.message, "failed.message");
      stage = "terminal";
      return Object.freeze({ ...event });
    }
    if (event.type === "started") {
      if (stage !== "initial") {
        throw new ProtocolError(`started event arrived during ${stage}`);
      }
      if (event.action !== request.action) {
        throw new ProtocolError("started action does not match request");
      }
      if (!samePath(event.root, request.root)) {
        throw new ProtocolError("started root does not match request");
      }
      stage = "started";
      return Object.freeze({ ...event });
    }
    if (event.type === "baseline_stable") {
      if (stage !== "started") {
        throw new ProtocolError(`baseline_stable event arrived before started (${stage})`);
      }
      if (!SHA256.test(event.fingerprint_sha256)) {
        throw new ProtocolError("baseline fingerprint is not a lowercase SHA-256 digest");
      }
      stage = "baseline";
      return Object.freeze({ ...event });
    }
    if (event.type === "completed") {
      if (stage !== "baseline") {
        throw new ProtocolError(`completed event arrived before started/baseline (${stage})`);
      }
      validateCompleted(event, request.output);
      stage = "terminal";
      return Object.freeze({ ...event });
    }
    throw new ProtocolError(`unknown protocol event type: ${String(event.type)}`);
  };
}

function validateCompleted(event, output) {
  if (!samePath(event.output, output)) {
    throw new ProtocolError("completed output does not match request");
  }
  for (const [field, filename] of [
    ["evidence", "reduction.json"],
    ["report", "report.html"],
    ["issue", "issue.md"],
  ]) {
    requireString(event[field], `completed.${field}`);
    if (!samePath(event[field], path.join(output, filename))) {
      throw new ProtocolError(`completed ${field} path escaped the requested artifact`);
    }
  }
}

function requireObject(value) {
  if (value === null || Array.isArray(value) || typeof value !== "object") {
    throw new ProtocolError("protocol event must be a JSON object");
  }
}

function requireString(value, field) {
  if (typeof value !== "string" || value.length === 0 || value.length > 65_536) {
    throw new ProtocolError(`${field} must be a bounded non-empty string`);
  }
}

function samePath(left, right) {
  if (typeof left !== "string" || typeof right !== "string") {
    return false;
  }
  const normalize = (value) => {
    const resolved = path.resolve(value);
    return process.platform === "win32" ? resolved.toLowerCase() : resolved;
  };
  return normalize(left) === normalize(right);
}

module.exports = { PROTOCOL_VERSION, ProtocolError, createEventValidator };
