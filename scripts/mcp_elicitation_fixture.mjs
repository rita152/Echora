#!/usr/bin/env node
// Minimal MCP stdio server that raises real elicitation/create requests.
//
// It exists only for CDP reference collection and manual verification of
// mcpServer/elicitation/request: no fixture data is compiled into the product
// UI. Tools:
//   elicit-form       one standard form elicitation covering every supported primitive
//   elicit-url        one url elicitation
//   elicit-concurrent two simultaneous form elicitations
//   elicit-long       one form with enough fields to require scrolling
//   echo              control tool that never elicits
import readline from "node:readline";
import fs from "node:fs";

// Every elicitation request/response pair is appended here so the reference
// capture can prove the exact action the client sent.
const logPath =
  process.env.ELICITATION_FIXTURE_LOG ??
  (process.env.CODEX_HOME ? process.env.CODEX_HOME + "/elicitation-fixture.log" : null);

function log(entry) {
  if (!logPath) return;
  try {
    fs.appendFileSync(logPath, JSON.stringify({ at: new Date().toISOString(), ...entry }) + "\n");
  } catch {
    // logging must never break the fixture
  }
}

const serverInfo = { name: "echora-elicitation-fixture", version: "1.0.0" };

const formSchema = {
  type: "object",
  properties: {
    projectName: {
      type: "string",
      title: "项目名称",
      description: "用于生成部署清单的短名称",
      minLength: 2,
      maxLength: 32,
    },
    contactEmail: {
      type: "string",
      title: "联系邮箱",
      format: "email",
    },
    replicas: {
      type: "integer",
      title: "副本数",
      description: "1 到 8 之间",
      minimum: 1,
      maximum: 8,
      default: 2,
    },
    enabled: {
      type: "boolean",
      title: "立即启用",
      default: false,
    },
    region: {
      type: "string",
      title: "部署区域",
      enum: ["us-east", "eu-west", "ap-northeast"],
      enumNames: ["美东", "西欧", "东京"],
      default: "eu-west",
    },
    features: {
      type: "array",
      title: "附加功能",
      description: "最多选择两项",
      minItems: 1,
      maxItems: 2,
      items: { type: "string", enum: ["logs", "metrics", "traces"] },
    },
  },
  required: ["projectName", "contactEmail", "region"],
};

const longSchema = {
  type: "object",
  properties: {
    summary: { type: "string", title: "变更说明", description: "一句话描述本次变更" },
    owner: { type: "string", title: "负责人" },
    channel: {
      type: "string",
      title: "通知渠道",
      oneOf: [
        { const: "slack", title: "Slack" },
        { const: "email", title: "邮件" },
        { const: "none", title: "不通知" },
      ],
    },
    window: {
      type: "string",
      title: "发布时间窗",
      enum: ["now", "tonight", "next-week"],
    },
    dryRun: { type: "boolean", title: "先做演练", default: true },
    rollback: { type: "boolean", title: "准备回滚", default: true },
    approvalNote: { type: "string", title: "审批备注", description: "可留空" },
  },
  required: ["summary"],
};

const tools = [
  {
    name: "elicit-form",
    description: "Ask the user for deployment details before continuing",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "elicit-url",
    description: "Ask the user to finish an external sign-in step",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "elicit-concurrent",
    description: "Ask two independent questions at the same time",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "elicit-long",
    description: "Ask for a longer set of change-request details",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "echo",
    description: "Return the provided text unchanged",
    inputSchema: {
      type: "object",
      properties: { text: { type: "string" } },
      required: ["text"],
    },
  },
];

let nextRequestId = 1000;
const pending = new Map();

function write(message) {
  process.stdout.write(JSON.stringify(message) + "\n");
}

function reply(id, result) {
  write({ jsonrpc: "2.0", id, result });
}

function fail(id, code, message) {
  write({ jsonrpc: "2.0", id, error: { code, message } });
}

function elicit(params) {
  const id = nextRequestId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    write({ jsonrpc: "2.0", id, method: "elicitation/create", params });
    log({ direction: "fixture_to_client", id, params });
  });
}

function textResult(value) {
  return { content: [{ type: "text", text: JSON.stringify(value) }] };
}

async function callTool(id, name) {
  switch (name) {
    case "echo":
      reply(id, textResult({ echoed: true }));
      return;
    case "elicit-form": {
      const result = await elicit({
        mode: "form",
        message: "部署前需要确认以下信息",
        requestedSchema: formSchema,
      });
      reply(id, textResult(result));
      return;
    }
    case "elicit-url": {
      const result = await elicit({
        mode: "url",
        elicitationId: "fixture-url-1",
        message: "请在浏览器中完成登录，然后返回这里继续",
        url: "https://example.com/device?code=echora-fixture",
      });
      reply(id, textResult(result));
      return;
    }
    case "elicit-concurrent": {
      const first = elicit({
        mode: "form",
        message: "并发问题 A：服务名称",
        requestedSchema: {
          type: "object",
          properties: { serviceName: { type: "string", title: "服务名称" } },
          required: ["serviceName"],
        },
      });
      const second = elicit({
        mode: "form",
        message: "并发问题 B：发布窗口",
        requestedSchema: {
          type: "object",
          properties: { window: { type: "string", title: "窗口", enum: ["now", "later"] } },
          required: ["window"],
        },
      });
      const [firstResult, secondResult] = await Promise.all([first, second]);
      reply(id, textResult({ first: firstResult, second: secondResult }));
      return;
    }
    case "elicit-long": {
      const result = await elicit({
        mode: "form",
        message: "请补充变更申请信息",
        requestedSchema: longSchema,
      });
      reply(id, textResult(result));
      return;
    }
    default:
      fail(id, -32602, "unknown tool " + name);
  }
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
  if (message.id !== undefined && message.method === undefined) {
    const waiter = pending.get(message.id);
    if (waiter) {
      pending.delete(message.id);
      log({
        direction: "client_to_fixture",
        id: message.id,
        result: message.result ?? null,
        error: message.error ?? null,
      });
      if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
      else waiter.resolve(message.result);
    }
    return;
  }
  if (message.id === undefined) return;
  switch (message.method) {
    case "initialize":
      reply(message.id, {
        protocolVersion: message.params?.protocolVersion ?? "2025-06-18",
        capabilities: { tools: {} },
        serverInfo,
      });
      break;
    case "tools/list":
      reply(message.id, { tools });
      break;
    case "tools/call":
      callTool(message.id, message.params?.name).catch((error) =>
        fail(message.id, -32603, String(error)),
      );
      break;
    case "resources/list":
      reply(message.id, { resources: [] });
      break;
    case "prompts/list":
      reply(message.id, { prompts: [] });
      break;
    default:
      fail(message.id, -32601, "unsupported method " + message.method);
  }
});
