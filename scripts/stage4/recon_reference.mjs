#!/usr/bin/env node
// Reconnaissance of the ChatGPT reference client: settings navigation, the
// skills and MCP pages, their DOM shape, and their computed styles.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/recon");

// Helpers evaluated in the page. Kept flat so the expressions stay readable.
const PAGE_HELPERS = `
window.__stage4 = {
  rect(node) {
    const rect = node.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  },
  center(node) {
    const rect = node.getBoundingClientRect();
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  },
  summary() {
    return {
      url: location.href,
      title: document.title,
      panels: [...document.querySelectorAll('[data-settings-panel-slug]')].map((node) => ({
        slug: node.getAttribute('data-settings-panel-slug'),
        label: (node.textContent || '').trim().slice(0, 40),
      })),
      navText: [...document.querySelectorAll('nav a, nav button')]
        .map((node) => (node.textContent || '').trim())
        .filter(Boolean),
      dialogs: document.querySelectorAll('[role="dialog"]').length,
    };
  },
  findButton(pattern) {
    const expression = new RegExp(pattern, 'i');
    return [...document.querySelectorAll('button, a, [role="button"], [role="menuitem"]')]
      .find((node) => expression.test((node.getAttribute('aria-label') || '') + ' ' + (node.textContent || '')));
  },
};
true`;

const cdp = await Cdp.connect(endpoint);
const steps = [];

const summary = async () => cdp.evaluate("JSON.stringify(window.__stage4.summary())");

await cdp.evaluate(PAGE_HELPERS);
steps.push({ step: "initial", state: JSON.parse(await summary()) });

// Command-comma opens settings in the desktop client.
await cdp.key(",", 4, "Comma");
await delay(1500);
let state = JSON.parse(await summary());
steps.push({ step: "after-cmd-comma", state });

if (state.panels.length === 0) {
  const target = await cdp.evaluate(
    "JSON.stringify((() => { const node = window.__stage4.findButton('设置|Settings'); return node ? window.__stage4.center(node) : null; })())",
  );
  const point = JSON.parse(target ?? "null");
  steps.push({ step: "settings-entry", target: point });
  if (point) {
    await cdp.click(point.x, point.y);
    await delay(1200);
    const afterClick = JSON.parse(await summary());
    steps.push({ step: "after-settings-click", state: afterClick });
    if (afterClick.panels.length === 0) {
      const item = await cdp.evaluate(
        "JSON.stringify((() => { const node = window.__stage4.findButton('^设置$|^Settings$'); return node ? window.__stage4.center(node) : null; })())",
      );
      const itemPoint = JSON.parse(item ?? "null");
      if (itemPoint) {
        await cdp.click(itemPoint.x, itemPoint.y);
        await delay(1500);
      }
    }
  }
}

state = JSON.parse(await summary());
steps.push({ step: "settings-open", state });

try {
  steps.push({ step: "window", windowInfo: await cdp.windowBounds() });
} catch (error) {
  steps.push({ step: "window", error: String(error) });
}

await writeJson(`${outputDir}/recon.json`, steps);
await cdp.screenshot(`${outputDir}/settings-open.png`);

console.log(
  JSON.stringify(
    { outputDir, steps: steps.map((entry) => entry.step), panels: state.panels, nav: state.navText },
    null,
    2,
  ),
);
cdp.close();
