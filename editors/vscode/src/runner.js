"use strict";

const fs = require("node:fs/promises");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");

const { ProtocolError, createEventValidator } = require("./protocol");

const MAX_EVENT_BYTES = 1024 * 1024;
const MAX_STDERR_BYTES = 64 * 1024;

async function runProtocol({
  binary,
  request,
  onEvent = () => {},
  cancellation,
  spawnProcess = spawn,
}) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "reprocut-vscode-"));
  const requestPath = path.join(directory, "request.json");
  await fs.writeFile(requestPath, `${JSON.stringify(request)}\n`, {
    encoding: "utf8",
    mode: 0o600,
    flag: "wx",
  });

  try {
    // The extension is only a client for `reprocut protocol run`.
    return await execute(
      binary,
      requestPath,
      request,
      onEvent,
      cancellation,
      spawnProcess,
    );
  } finally {
    await fs.rm(directory, { recursive: true, force: true });
  }
}

function execute(binary, requestPath, request, onEvent, cancellation, spawnProcess) {
  return new Promise((resolve, reject) => {
    const child = spawnProcess(binary, ["protocol", "run", "--request", requestPath], {
      shell: false,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const validate = createEventValidator(request);
    let stdout = Buffer.alloc(0);
    let stderr = Buffer.alloc(0);
    let terminal = null;
    let settled = false;

    const cancelSubscription = cancellation?.onCancellationRequested(() => {
      child.kill();
    });

    const fail = (error) => {
      if (settled) return;
      settled = true;
      child.kill();
      reject(error);
    };

    child.once("error", (error) => fail(error));
    child.stdout.on("data", (chunk) => {
      if (settled) return;
      stdout = Buffer.concat([stdout, chunk]);
      if (stdout.length > MAX_EVENT_BYTES) {
        fail(new ProtocolError("protocol event exceeded the 1 MiB bound"));
        return;
      }
      let newline = stdout.indexOf(0x0a);
      while (newline >= 0) {
        const line = stdout.subarray(0, newline).toString("utf8").trimEnd();
        stdout = stdout.subarray(newline + 1);
        if (line.length > 0) {
          try {
            const event = validate(JSON.parse(line));
            terminal = event.type === "completed" || event.type === "failed" ? event : terminal;
            onEvent(event);
          } catch (error) {
            fail(error);
            return;
          }
        }
        newline = stdout.indexOf(0x0a);
      }
    });
    child.stderr.on("data", (chunk) => {
      const remaining = MAX_STDERR_BYTES - stderr.length;
      if (remaining > 0) stderr = Buffer.concat([stderr, chunk.subarray(0, remaining)]);
    });
    child.once("close", (code, signal) => {
      cancelSubscription?.dispose();
      if (settled) return;
      settled = true;
      if (stdout.length !== 0) {
        reject(new ProtocolError("protocol stdout ended with an incomplete JSONL event"));
        return;
      }
      if (terminal?.type === "completed" && code === 0) {
        resolve(terminal);
        return;
      }
      const diagnostic = terminal?.message ?? stderr.toString("utf8").trim();
      reject(
        new ProtocolError(
          diagnostic || `ReproCut exited without completion (code=${String(code)}, signal=${String(signal)})`,
        ),
      );
    });
  });
}

module.exports = { MAX_EVENT_BYTES, runProtocol };
