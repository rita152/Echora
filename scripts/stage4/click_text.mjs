#!/usr/bin/env node
// Clicks the innermost element containing the given text (walking up to the
// nearest clickable ancestor) and reports the resulting page state.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const needle = option("text", "");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/recon");
const name = option("name", "click");
const settle = Number(option("settle", "1500"));
const hover = option("hover") === "1";

const cdp = await Cdp.connect(endpoint);

const target = JSON.parse(
  (await cdp.evaluate(`JSON.stringify((() => {
    const match = [...document.querySelectorAll('*')].find((node) =>
      (node.childElementCount === 0 ? (node.textContent || '') : '').trim().includes(${JSON.stringify(needle)}));
    if (!match) return null;
    const clickable = match.closest('[role="button"], button, [role="menuitem"], a, li') || match;
    const rect = clickable.getBoundingClientRect();
    return {
      text: (clickable.textContent || '').trim().slice(0, 60),
      role: clickable.getAttribute('role'),
      x: rect.x + rect.width / 2,
      y: rect.y + rect.height / 2,
      rect: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
    };
  })())`)) ?? "null",
);

if (!target) throw new Error(`text not found: ${needle}`);
if (hover) {
  await cdp.hover(target.x, target.y);
} else {
  await cdp.click(target.x, target.y);
}
await delay(settle);

const state = JSON.parse(
  await cdp.evaluate(`JSON.stringify({
    text: (document.body.innerText || '').slice(0, 1500),
    dialogs: [...document.querySelectorAll('[role="dialog"], [role="menu"], [role="alertdialog"]')].map((node) => (node.textContent || '').trim().slice(0, 200)),
    buttons: [...document.querySelectorAll('button, [role="button"]')].filter((node) => node.offsetParent !== null).map((node) => (node.textContent || '').trim().slice(0, 30)).filter(Boolean).slice(0, 40),
  })`),
);

await writeJson(`${outputDir}/${name}.json`, { target, state });
await cdp.screenshot(`${outputDir}/${name}.png`);
console.log(JSON.stringify({ target, text: state.text.slice(0, 800), dialogs: state.dialogs, buttons: state.buttons }, null, 2).slice(0, 3000));
cdp.close();
