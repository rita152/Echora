// Read-only probe of the plugin / app / external-agent methods against a real
// app-server. It records the exact request payloads this client intends to send
// and every response verbatim, so the UI can be driven by captured real data.
//
//   node scripts/probe_manage_protocol.mjs <output-dir> [cwd]
//
// Nothing here mutates state: no install, uninstall, share or marketplace call
// is issued.
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";

const outputDir = path.resolve(process.argv[2] ?? "artifacts/p2-manage/protocol");
const cwd = process.argv[3] ? path.resolve(process.argv[3]) : null;
await mkdir(outputDir, { recursive: true });

const child = spawn("codex", ["app-server", "--stdio"], {
  cwd: cwd ?? process.cwd(),
  stdio: ["pipe", "pipe", "pipe"],
  env: process.env,
});

const transport = [];
const notes = [];
let nextId = 1;
const pending = new Map();

const reader = readline.createInterface({ input: child.stdout });
reader.on("line", line => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    transport.push({ direction: "server", raw: line });
    return;
  }
  transport.push({ direction: "server", message });
  if (message.id != null && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
child.stderr.on("data", chunk => notes.push(chunk.toString()));

const send = (method, params) => {
  const id = nextId++;
  const request = { id, method, params: params ?? {} };
  transport.push({ direction: "client", message: request });
  child.stdin.write(JSON.stringify(request) + "\n");
  return new Promise(resolve => pending.set(id, resolve));
};
const notify = (method, params) => {
  const request = { method, params: params ?? {} };
  transport.push({ direction: "client", message: request });
  child.stdin.write(JSON.stringify(request) + "\n");
};
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

// The same initialize payload the application sends, so the answered capability
// set matches a real run.
const initialize = await send("initialize", {
  clientInfo: { name: "gpui_chat_clone", title: "GPUI Chat Clone", version: "0.1.0" },
  capabilities: {
    experimentalApi: true,
    requestAttestation: false,
    optOutNotificationMethods: [
      "thread/goal/updated",
      "thread/goal/cleared",
      "thread/queue/changed",
      "turn/moderationMetadata",
      "thread/compacted",
    ],
  },
});
notify("initialized", {});

const calls = [
  ["app/list", { limit: 100 }],
  ["app/installed", {}],
  ["plugin/list", {}],
  ["plugin/installed", {}],
  ["plugin/share/list", {}],
];

const responses = {};
for (const [method, params] of calls) {
  const key = method;
  const response = await send(method, params);
  responses[key] = response;
  console.log(key, response.error ? "ERROR" : "ok");
}

// Read the first discovered app and plugin so the detail shapes are captured too.
const appIds = responses["app/list"]?.result?.data?.slice(0, 3).map(app => app.id) ?? [];
if (appIds.length > 0) {
  responses["app/read"] = await send("app/read", { appIds, includeTools: true });
}
const firstMarketplace = responses["plugin/list"]?.result?.marketplaces?.[0];
const firstPlugin = firstMarketplace?.plugins?.[0];
if (firstMarketplace && firstPlugin) {
  responses["plugin/read"] = await send("plugin/read", {
    marketplacePath: firstMarketplace.path ?? null,
    pluginName: firstPlugin.name,
    remoteMarketplaceName: null,
  });
  responses["plugin/search"] = await send("plugin/search", {
    searchTerm: firstPlugin.name,
    limit: 10,
  });
}

await wait(300);
child.stdin.end();
child.kill();

const transcriptPath = path.join(outputDir, "protocol-transcript.jsonl");
await writeFile(
  transcriptPath,
  transport.map(entry => JSON.stringify(entry)).join("\n") + "\n",
);
await writeFile(
  path.join(outputDir, "responses.json"),
  JSON.stringify({ initialize, responses }, null, 2),
);
await writeFile(path.join(outputDir, "stderr.log"), notes.join(""));
for (const [method, response] of Object.entries(responses)) {
  if (!response?.result) continue;
  await writeFile(
    path.join(outputDir, method.replaceAll("/", "_") + ".json"),
    JSON.stringify(response.result, null, 2),
  );
}
console.log("saved", outputDir);
