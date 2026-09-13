#!/usr/bin/env node
// Drives a real `codex app-server --stdio` through the stage-4 skills and MCP
// methods and records every JSON-RPC frame. Used to validate the client
// implementation against observed server behavior.
import { spawn } from "node:child_process";
import { appendFile, mkdir, writeFile } from "node:fs/promises";
import { setTimeout as delay } from "node:timers/promises";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";

const option = (name, fallback = null) => {
  const prefix = `--${name}=`;
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) ?? fallback;
};

const codexHome = path.resolve(option("codex-home", path.join(process.cwd(), "artifacts", "skills-mcp-stage4", "codex-home")));
const cwd = path.resolve(option("cwd", process.cwd()));
const outputDir = path.resolve(option("output", path.join(process.cwd(), "artifacts", "skills-mcp-stage4", "protocol")));
const codexBin = option("codex", "codex");
const plan = option("plan", "default");

await mkdir(outputDir, { recursive: true });

const transcript = [];
let sequence = 0;

const child = spawn(codexBin, ["app-server", "--stdio"], {
  cwd,
  stdio: ["pipe", "pipe", "pipe"],
  env: { ...process.env, CODEX_HOME: codexHome },
});

function classify(message) {
  if (message?.method && message?.id !== undefined) return "server_request";
  if (message?.method) return "notification";
  if (message?.id !== undefined && message.error !== undefined) return "error_response";
  if (message?.id !== undefined) return "response";
  return "unknown";
}

function record(direction, message, note = null) {
  transcript.push({
    sequence: ++sequence,
    at: new Date().toISOString(),
    direction,
    kind: classify(message),
    method: typeof message?.method === "string" ? message.method : null,
    id: message?.id ?? null,
    note,
    message,
  });
}

function send(message, note = null) {
  record("client_to_server", message, note);
  child.stdin.write(`${JSON.stringify(message)}\n`);
}

const waiters = new Map();
const notifications = [];
let fatal = null;

const stdout = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
stdout.on("line", (line) => {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch (error) {
    fatal = new Error(`invalid JSON from app-server: ${error.message}`);
    return;
  }
  record("server_to_client", message);
  if (message.method && message.id !== undefined) {
    send({ id: message.id, error: { code: -32601, message: `capture client does not implement ${message.method}` } }, "probe answer to server request");
    return;
  }
  if (message.method) {
    notifications.push(message);
    return;
  }
  const waiter = waiters.get(message.id);
  if (waiter) {
    waiters.delete(message.id);
    waiter(message);
  }
});

const stderrLines = [];
readline.createInterface({ input: child.stderr, crlfDelay: Infinity }).on("line", (line) => {
  stderrLines.push({ at: new Date().toISOString(), line });
});

child.on("error", (error) => {
  fatal = error;
});

async function request(method, params, note = null) {
  const id = sequence + 1;
  const outcome = new Promise((resolve, reject) => {
    waiters.set(id, resolve);
    setTimeout(() => {
      if (waiters.delete(id)) reject(new Error(`timeout waiting for ${method} response`));
    }, 30000);
  });
  send({ id, method, ...(params === undefined ? {} : { params }) }, note);
  const message = await outcome;
  return message;
}

async function notify(method, params) {
  send({ method, ...(params === undefined ? {} : { params }) });
}

async function waitForNotification(method, predicate = () => true, timeoutMs = 8000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const found = notifications.find((message) => message.method === method && predicate(message));
    if (found) return found;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  return null;
}

const results = {};

async function startLocalOauthServer(mode, port) {
  const script = path.resolve(option("oauth-server", path.join(process.cwd(), "scripts", "stage4", "mcp_oauth_server.py")));
  const child = spawn(option("python", "python3"), [script, "--port", String(port), "--mode", mode], {
    stdio: ["ignore", "pipe", "pipe"],
  });
  const ready = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("oauth server did not announce itself")), 15000);
    readline.createInterface({ input: child.stdout }).on("line", (line) => {
      if (!line.includes("listening")) return;
      clearTimeout(timer);
      resolve(line);
    });
    child.on("error", reject);
  });
  await ready;
  return child;
}

async function runDefaultPlan() {
  results.initialize = await request("initialize", {
    clientInfo: { name: "echora_stage4_probe", title: "Echora stage 4 probe", version: "1.0.0" },
    capabilities: { experimentalApi: true },
  });
  await notify("initialized", {});

  results.skillsListEmpty = await request("skills/list", { cwds: [cwd] });
  results.skillsListForce = await request("skills/list", { cwds: [cwd], forceReload: true });

  const firstSkill = results.skillsListForce?.result?.data?.flatMap((entry) => entry.skills ?? [])?.[0];
  if (firstSkill) {
    results.skillsConfigWriteDisable = await request(
      "skills/config/write",
      firstSkill.name.includes(":") ? { name: firstSkill.name, enabled: false } : { path: firstSkill.path, enabled: false },
    );
    results.skillsChangedAfterWrite = await waitForNotification("skills/changed", () => true, 5000);
    results.skillsListAfterDisable = await request("skills/list", { cwds: [cwd], forceReload: true });
    results.skillsConfigWriteEnable = await request(
      "skills/config/write",
      firstSkill.name.includes(":") ? { name: firstSkill.name, enabled: true } : { path: firstSkill.path, enabled: true },
    );
    results.skillsConfigWriteUnknown = await request("skills/config/write", { name: "definitely-missing-skill", enabled: true });
  }

  results.mcpStatusFirstPage = await request("mcpServerStatus/list", { limit: 2, detail: "full" });
  const nextCursor = results.mcpStatusFirstPage?.result?.nextCursor ?? null;
  if (nextCursor) {
    results.mcpStatusSecondPage = await request("mcpServerStatus/list", { limit: 2, cursor: nextCursor, detail: "full" });
  }
  results.mcpStatusToolsOnly = await request("mcpServerStatus/list", { detail: "toolsAndAuthOnly" });
  results.mcpReload = await request("config/mcpServer/reload", undefined);
  results.mcpStartupAfterReload = await waitForNotification("mcpServer/startupStatus/updated", () => true, 8000);
  results.mcpStatusAfterReload = await request("mcpServerStatus/list", { detail: "full" });
}

async function runOauthPlan() {
  results.initialize = await request("initialize", {
    clientInfo: { name: "echora_stage4_probe", title: "Echora stage 4 probe", version: "1.0.0" },
    capabilities: { experimentalApi: true },
  });
  await notify("initialized", {});
  const name = option("server", "remote-notes");
  const mode = option("oauth-mode", "accept");
  const port = Number(option("oauth-port", "8791"));
  const serverMode = mode === "never-open" ? "hang" : mode;
  const oauthServer = await startLocalOauthServer(serverMode, port);
  results.oauthServerMode = mode;
  const timeoutSecs = option("timeout-secs");
  const loginParams = { name, ...(timeoutSecs ? { timeoutSecs: Number(timeoutSecs) } : {}) };
  const loginStarted = Date.now();
  const authorization = await request("mcpServer/oauth/login", loginParams);
  results.oauthLoginDurationMs = Date.now() - loginStarted;
  results.oauthLogin = authorization;
  results.oauthLoginAt = new Date().toISOString();
  const authorizationUrl = authorization?.result?.authorizationUrl ?? null;
  if (authorizationUrl && mode !== "never-open") {
    results.oauthBrowserVisit = await visitAuthorizationUrl(authorizationUrl);
  }
  results.oauthCompleted = await waitForNotification("mcpServer/oauthLogin/completed", () => true, 25000);
  results.mcpStatusAfterLogin = await request("mcpServerStatus/list", { detail: "full" });
  results.oauthLoginMissing = await request("mcpServer/oauth/login", { name: "definitely-missing-server" });
  try {
    const response = await fetch(`http://127.0.0.1:${port}/events`);
    results.oauthServerEvents = await response.json();
  } catch (error) {
    results.oauthServerEvents = `unavailable: ${error.message}`;
  }
  oauthServer.kill("SIGTERM");
  await delay(200);
}

async function visitAuthorizationUrl(url) {
  try {
    const response = await fetch(url, { redirect: "follow" });
    return { status: response.status, finalUrl: response.url };
  } catch (error) {
    return { error: error.message };
  }
}

async function runThreadPlan() {
  results.initialize = await request("initialize", {
    clientInfo: { name: "echora_stage4_probe", title: "Echora stage 4 probe", version: "1.0.0" },
    capabilities: { experimentalApi: true },
  });
  await notify("initialized", {});
  results.threadStart = await request("thread/start", {
    cwd,
    approvalPolicy: "never",
    sandbox: "read-only",
    serviceName: "echora-stage4-thread",
  });
  const threadId = results.threadStart?.result?.thread?.id ?? null;
  results.threadId = threadId;
  await new Promise((resolve) => setTimeout(resolve, 4000));
  results.startupNotifications = notifications
    .filter((message) => message.method === "mcpServer/startupStatus/updated")
    .map((message) => message.params);
  results.mcpStatusWithThread = await request("mcpServerStatus/list", { threadId, detail: "full" });
  results.mcpStatusToolsOnlyWithThread = await request("mcpServerStatus/list", { threadId, detail: "toolsAndAuthOnly", limit: 2 });

  const skillPath = path.join(codexHome, "skills", "release-notes", "SKILL.md");
  try {
    await appendFile(skillPath, `\n<!-- stage4 watch trigger ${Date.now()} -->\n`);
    results.skillFileTouched = skillPath;
  } catch (error) {
    results.skillFileTouched = `failed: ${error.message}`;
  }
  results.skillsChanged = await waitForNotification("skills/changed", () => true, 10000);
  results.skillsListAfterChange = await request("skills/list", { cwds: [cwd] });
  results.mcpReloadWithThread = await request("config/mcpServer/reload", undefined);
  await new Promise((resolve) => setTimeout(resolve, 3000));
  results.startupNotificationsAfterReload = notifications
    .filter((message) => message.method === "mcpServer/startupStatus/updated")
    .map((message) => message.params);
  results.mcpStatusAfterReloadWithThread = await request("mcpServerStatus/list", { threadId, detail: "full" });
}

let runError = null;
try {
  if (plan === "oauth") await runOauthPlan();
  else if (plan === "thread") await runThreadPlan();
  else await runDefaultPlan();
} catch (error) {
  runError = error instanceof Error ? error.message : String(error);
} finally {
  child.stdin.end();
  await new Promise((resolve) => {
    if (child.exitCode !== null) return resolve();
    child.once("exit", resolve);
    setTimeout(() => {
      child.kill("SIGTERM");
      resolve();
    }, 2000);
  });
}

const capture = {
  captureFormatVersion: 1,
  metadata: { codexBin, codexHome, cwd, plan, platform: process.platform, runError, finishedAt: new Date().toISOString() },
  transcript,
  stderr: stderrLines,
};

await writeFile(path.join(outputDir, `${plan}-transcript.json`), `${JSON.stringify(capture, null, 2)}\n`);
await writeFile(path.join(outputDir, `${plan}-results.json`), `${JSON.stringify(results, null, 2)}\n`);

console.log(JSON.stringify({ outputDir, plan, runError, notifications: notifications.map((message) => message.method) }));
if (runError) process.exitCode = 1;
