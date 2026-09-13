#!/usr/bin/env node
// Minimal MCP stdio server used as a deterministic reference endpoint for
// skills/MCP verification. It implements just enough of the MCP JSON-RPC
// surface for `mcpServerStatus/list` to report tools and resources.
import readline from "node:readline";

const serverInfo = { name: "echora-echo", version: "1.4.0", title: "Echora Echo", websiteUrl: "https://example.invalid/echo" };

const tools = [
  {
    name: "echo",
    description: "Return the provided text unchanged",
    inputSchema: {
      type: "object",
      properties: { text: { type: "string", description: "Text to echo" } },
      required: ["text"],
    },
  },
  {
    name: "sum",
    description: "Add two integers",
    inputSchema: {
      type: "object",
      properties: { left: { type: "integer" }, right: { type: "integer" } },
      required: ["left", "right"],
    },
  },
  {
    name: "slow-report",
    description: "Produce a short status report after a wait",
    inputSchema: { type: "object", properties: {} },
  },
];

const resources = [
  { uri: "echo://notes/readme", name: "readme", title: "Echo notes", mimeType: "text/markdown", description: "Deterministic sample resource" },
];

const resourceTemplates = [
  { uriTemplate: "echo://notes/{topic}", name: "topic-note", title: "Topic note", mimeType: "text/markdown" },
];

function respond(id, result) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`);
}

function fail(id, code, message) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id, error: { code, message } })}\n`);
}

const reader = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
reader.on("line", (line) => {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  if (message.id === undefined) return;
  switch (message.method) {
    case "initialize":
      respond(message.id, {
        protocolVersion: message.params?.protocolVersion ?? "2025-06-18",
        capabilities: { tools: { listChanged: false }, resources: { listChanged: false, subscribe: false } },
        serverInfo,
      });
      break;
    case "tools/list":
      respond(message.id, { tools });
      break;
    case "resources/list":
      respond(message.id, { resources });
      break;
    case "resources/templates/list":
      respond(message.id, { resourceTemplates });
      break;
    case "tools/call": {
      const name = message.params?.name;
      const args = message.params?.arguments ?? {};
      if (name === "echo") {
        respond(message.id, { content: [{ type: "text", text: String(args.text ?? "") }] });
      } else if (name === "sum") {
        respond(message.id, { content: [{ type: "text", text: String(Number(args.left) + Number(args.right)) }] });
      } else if (name === "slow-report") {
        setTimeout(() => respond(message.id, { content: [{ type: "text", text: "ok" }] }), 1200);
      } else {
        fail(message.id, -32602, `unknown tool: ${name}`);
      }
      break;
    }
    case "ping":
      respond(message.id, {});
      break;
    case "prompts/list":
      respond(message.id, { prompts: [] });
      break;
    default:
      fail(message.id, -32601, `unsupported method: ${message.method}`);
  }
});

reader.on("close", () => process.exit(0));
