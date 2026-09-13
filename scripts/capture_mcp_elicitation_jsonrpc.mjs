#!/usr/bin/env node
// Real app-server JSON-RPC capture for mcpServer/elicitation/request.
//
// Runs the codex app-server against a temporary CODEX_HOME whose config
// registers the elicitation fixture MCP server, then triggers tool calls with
// mcpServer/tool/call and records every elicitation request, response, and
// serverRequest/resolved notification.
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";

const projectCwd = path.resolve(process.argv[2] ?? process.cwd());
const codexHome = path.resolve(process.argv[3] ?? path.join(projectCwd, "artifacts", "mcp-elicitation-codex-home"));
const outputDir = path.resolve(process.argv[4] ?? path.join(projectCwd, "artifacts", "mcp-elicitation-cdp-20260913", "protocol"));
const codexBin = process.argv[5] ?? "codex";

await mkdir(outputDir, { recursive: true });

const transcript = [];
let sequence = 0;

function record(entry) {
  transcript.push({ sequence: ++sequence, at: new Date().toISOString(), ...entry });
}

const child = spawn(codexBin, ["app-server", "--stdio"], {
  cwd: projectCwd,
  stdio: ["pipe", "pipe", "pipe"],
  env: { ...process.env, CODEX_HOME: codexHome },
});

const stderr = [];
child.stderr.on("data", (chunk) => stderr.push(chunk.toString()));

const pendingResponses = new Map();
const elicitationRequests = [];
let nextId = 1;

function send(message, note) {
  record({ direction: "client_to_server", note, message });
  child.stdin.write(JSON.stringify(message) + "\n");
}

function request(method, params, note) {
  const id = nextId++;
  const promise = new Promise((resolve) => pendingResponses.set(id, resolve));
  send({ method, id, params }, note);
  return promise;
}

let elicitationHandler = null;

readline.createInterface({ input: child.stdout, crlfDelay: Infinity }).on("line", (line) => {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    record({ direction: "server_to_client", raw: line, note: "unparsable" });
    return;
  }
  record({ direction: "server_to_client", message });
  if (message.method === "mcpServer/elicitation/request") {
    elicitationRequests.push(message);
    const handler = elicitationHandler;
    elicitationHandler = null;
    if (handler) handler(message);
    return;
  }
  if (message.id !== undefined && message.method === undefined) {
    const resolve = pendingResponses.get(message.id);
    if (resolve) {
      pendingResponses.delete(message.id);
      resolve(message);
    }
  }
});

function waitForElicitation(timeoutMs = 20000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("timed out waiting for mcpServer/elicitation/request")),
      timeoutMs,
    );
    elicitationHandler = (message) => {
      clearTimeout(timer);
      resolve(message);
    };
  });
}

function answer(message, result, note) {
  send({ id: message.id, result }, note);
}

await new Promise((resolve) => child.once("spawn", resolve));

const initialize = await request("initialize", {
  clientInfo: { name: "echora_elicitation_capture", title: "Echora capture", version: "0.1.0" },
  capabilities: { experimentalApi: true, requestAttestation: false },
});
send({ method: "initialized", params: {} });
record({ direction: "note", note: "initialize result", message: initialize });

const threadA = await request("thread/start", {
  cwd: projectCwd,
  ephemeral: false,
  historyMode: "paginated",
  serviceName: "echora-elicitation-capture",
});
const threadAId = threadA?.result?.thread?.id;
const threadB = await request("thread/start", {
  cwd: projectCwd,
  ephemeral: true,
  historyMode: "paginated",
  serviceName: "echora-elicitation-capture",
});
const threadBId = threadB?.result?.thread?.id;

const scenarios = [];

async function toolCall(tool, threadId, note) {
  return request(
    "mcpServer/tool/call",
    { server: "echora-elicitation-fixture", threadId, tool, arguments: {} },
    note,
  );
}

// 1. form + accept with a complete payload
{
  const call = toolCall("elicit-form", threadAId, "form accept");
  const elicitation = await waitForElicitation();
  scenarios.push({
    name: "form-accept",
    request: elicitation,
    turnId: elicitation.params?.turnId === undefined ? "missing" : elicitation.params.turnId,
  });
  answer(
    elicitation,
    {
      action: "accept",
      content: {
        projectName: "echora",
        contactEmail: "dev@example.com",
        replicas: 3,
        enabled: true,
        region: "eu-west",
        features: ["logs", "metrics"],
      },
    },
    "form accept response",
  );
  scenarios.at(-1).toolResult = await call;
}

// 2. form + decline
{
  const call = toolCall("elicit-form", threadAId, "form decline");
  const elicitation = await waitForElicitation();
  scenarios.push({ name: "form-decline", request: elicitation });
  answer(elicitation, { action: "decline" }, "form decline response");
  scenarios.at(-1).toolResult = await call;
}

// 3. form + cancel
{
  const call = toolCall("elicit-form", threadAId, "form cancel");
  const elicitation = await waitForElicitation();
  scenarios.push({ name: "form-cancel", request: elicitation });
  answer(elicitation, { action: "cancel" }, "form cancel response");
  scenarios.at(-1).toolResult = await call;
}

// 4. url + accept without content
{
  const call = toolCall("elicit-url", threadAId, "url accept");
  const elicitation = await waitForElicitation();
  scenarios.push({ name: "url-accept", request: elicitation });
  answer(elicitation, { action: "accept" }, "url accept response");
  scenarios.at(-1).toolResult = await call;
}

// 5. two concurrent elicitations on one thread, answered out of order
{
  const call = toolCall("elicit-concurrent", threadAId, "concurrent");
  const first = await waitForElicitation();
  const second = await waitForElicitation();
  scenarios.push({
    name: "concurrent",
    request: first,
    secondRequest: second,
  });
  answer(second, { action: "accept", content: { window: "later" } }, "concurrent second");
  answer(first, { action: "accept", content: { serviceName: "checkout" } }, "concurrent first");
  scenarios.at(-1).toolResult = await call;
}

// 6. another thread must stay independent
{
  const call = toolCall("elicit-long", threadBId, "second thread");
  const elicitation = await waitForElicitation();
  scenarios.push({
    name: "second-thread",
    request: elicitation,
    turnId: elicitation.params?.turnId === undefined ? "missing" : elicitation.params.turnId,
  });
  answer(
    elicitation,
    {
      action: "accept",
      content: {
        summary: "second thread answer",
        channel: "slack",
        window: "tonight",
        dryRun: false,
        rollback: true,
      },
    },
    "second thread response",
  );
  scenarios.at(-1).toolResult = await call;
}

await new Promise((resolve) => setTimeout(resolve, 1500));
child.stdin.end();
child.kill();

const summary = scenarios.map((scenario) => ({
  name: scenario.name,
  requestId: scenario.request?.id,
  requestIdType: typeof scenario.request?.id,
  serverName: scenario.request?.params?.serverName,
  threadId: scenario.request?.params?.threadId,
  turnId: scenario.turnId ?? scenario.request?.params?.turnId,
  mode: scenario.request?.params?.mode,
  hasTurnIdKey: scenario.request?.params
    ? Object.prototype.hasOwnProperty.call(scenario.request.params, "turnId")
    : null,
  secondRequestId: scenario.secondRequest?.id,
  toolResultIsError: scenario.toolResult?.error != null,
  toolResultText: scenario.toolResult?.result?.content?.[0]?.text ?? null,
}));

await writeFile(
  path.join(outputDir, "elicitation-jsonrpc-transcript.json"),
  JSON.stringify({ codexBin, codexHome, scenarios: summary, transcript }, null, 2),
);
await writeFile(path.join(outputDir, "app-server-stderr.txt"), stderr.join(""));
console.log(JSON.stringify(summary, null, 2));
