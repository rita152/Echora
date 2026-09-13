#!/usr/bin/env node
// Opens one settings page by label and dumps its visible structure.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/recon");
const label = option("label", "插件");

const cdp = await Cdp.connect(endpoint);

const target = JSON.parse(
  (await cdp.evaluate(`JSON.stringify((() => {
    const nodes = [...document.querySelectorAll('nav button, nav a, [role="tab"], button')]
      .filter((node) => (node.textContent || '').trim() === ${JSON.stringify(label)});
    const node = nodes[0];
    if (!node) return null;
    const rect = node.getBoundingClientRect();
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, count: nodes.length };
  })())`)) ?? "null",
);

if (!target) throw new Error(`settings nav entry not found: ${label}`);
await cdp.click(target.x, target.y);
await delay(2000);

const dump = JSON.parse(
  await cdp.evaluate(`JSON.stringify({
    text: (document.body.innerText || '').slice(0, 2500),
    tabs: [...document.querySelectorAll('[role="tab"], [role="tablist"] button')].map((node) => (node.textContent || '').trim()).filter(Boolean),
    toggles: [...document.querySelectorAll('button[role="switch"], input[type="checkbox"], [role="checkbox"]')].map((node) => ({ role: node.getAttribute('role'), checked: node.getAttribute('aria-checked'), label: node.getAttribute('aria-label'), })),
    scrollHeight: document.documentElement.scrollHeight,
  })`),
);

await writeJson(`${outputDir}/page-${label}.json`, dump);
await cdp.screenshot(`${outputDir}/page-${label}.png`);
console.log(JSON.stringify(dump, null, 2).slice(0, 4000));
cdp.close();
