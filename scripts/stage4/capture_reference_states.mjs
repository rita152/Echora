#!/usr/bin/env node
// Captures the ChatGPT reference states used for stage-4 comparison:
// MCP list, skills list, and the MCP detail panel, all in one CDP session.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/states");

const cdp = await Cdp.connect(endpoint);
const steps = [];

async function clickRect(expression, description) {
  const raw = await cdp.evaluate(
    `JSON.stringify((() => { const node = ${expression}; if (!node) return null; const rect = node.getBoundingClientRect(); return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, text: (node.textContent || '').trim().slice(0, 30) }; })())`,
  );
  const target = JSON.parse(raw ?? "null");
  if (!target) throw new Error(`missing target: ${description}`);
  await cdp.click(target.x, target.y);
  await delay(1800);
  steps.push({ step: description, target: target.text });
  return target;
}

const pageText = async () => cdp.evaluate("(document.body.innerText || '').slice(0, 400)");

// Settings → 插件 (plugins page hosting the plugins/apps/MCP/skills segments).
await clickRect(
  "[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '').includes('个人资料菜单'))",
  "profile-menu",
);
await clickRect(
  "[...document.querySelectorAll('[role=\"menuitem\"], [role=\"menu\"] button, [role=\"menu\"] a')].find((n) => (n.textContent || '').trim().startsWith('设置'))",
  "settings",
);
await clickRect(
  "[...document.querySelectorAll('nav button, nav a, [role=\"tab\"]')].find((n) => (n.textContent || '').trim() === '插件')",
  "plugins-page",
);

const segment = async (label) => {
  await clickRect(
    `[...document.querySelectorAll('button')].find((n) => (n.textContent || '').trim().startsWith(${JSON.stringify(label)}))`,
    `segment-${label}`,
  );
  await delay(1200);
};

await segment("MCP");
await cdp.screenshot(`${outputDir}/mcp-list.png`);
steps.push({ step: "capture-mcp-list", text: await pageText() });

await segment("技能");
await cdp.screenshot(`${outputDir}/skills-list.png`);
steps.push({ step: "capture-skills-list", text: await pageText() });

// The row gear opens the server detail form.
await segment("MCP");
await clickRect(
  "[...document.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === '设置')",
  "mcp-row-gear",
);
await cdp.screenshot(`${outputDir}/mcp-detail.png`);
steps.push({ step: "capture-mcp-detail", text: await pageText() });

await writeJson(`${outputDir}/steps.json`, steps);
console.log(JSON.stringify({ outputDir, steps: steps.map((entry) => entry.step) }));
cdp.close();
