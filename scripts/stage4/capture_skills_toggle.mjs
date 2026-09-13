#!/usr/bin/env node
// Captures the reference skills segment before/during/after a toggle write.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/skills-toggle");

const cdp = await Cdp.connect(endpoint);

const SWITCH = `JSON.stringify((() => {
  const node = [...document.querySelectorAll('[role="switch"]')].find((candidate) => {
    const rect = candidate.getBoundingClientRect();
    return rect.width > 0 && rect.x > 1000;
  });
  if (!node) return null;
  const rect = node.getBoundingClientRect();
  const style = getComputedStyle(node);
  const thumb = node.querySelector('*');
  const thumbRect = thumb ? thumb.getBoundingClientRect() : null;
  return {
    aria: node.getAttribute('aria-label'),
    checked: node.getAttribute('aria-checked'),
    rect: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
    style: { background: style.backgroundColor, radius: style.borderRadius, transition: style.transition },
    thumb: thumbRect ? { x: Math.round(thumbRect.x), y: Math.round(thumbRect.y), width: Math.round(thumbRect.width), height: Math.round(thumbRect.height), background: getComputedStyle(thumb).backgroundColor } : null,
    html: node.outerHTML.slice(0, 500),
  };
})())`;

const before = JSON.parse((await cdp.evaluate(SWITCH)) ?? "null");
if (!before) throw new Error("skill switch not found; open the skills segment first");

const frames = [];
await cdp.hover(before.rect.x + before.rect.width / 2, before.rect.y + before.rect.height / 2);
await delay(400);
frames.push({ phase: "hover", switch: JSON.parse(await cdp.evaluate(SWITCH)) });

await cdp.click(before.rect.x + before.rect.width / 2, before.rect.y + before.rect.height / 2);
await delay(120);
frames.push({ phase: "clicked", switch: JSON.parse(await cdp.evaluate(SWITCH)) });
await cdp.screenshot(`${outputDir}/saving.png`);

await delay(1800);
frames.push({ phase: "settled", switch: JSON.parse(await cdp.evaluate(SWITCH)) });
const toasts = await cdp.evaluate(
  "JSON.stringify([...document.querySelectorAll('[data-sonner-toast], [role=\"status\"], [role=\"alert\"]')].map((node) => (node.textContent || '').trim().slice(0, 120)))",
);
await cdp.screenshot(`${outputDir}/saved.png`);

// Restore the original state so the reference instance keeps a stable fixture.
const restored = JSON.parse(await cdp.evaluate(SWITCH));
if (restored && restored.checked !== before.checked) {
  await cdp.click(restored.rect.x + restored.rect.width / 2, restored.rect.y + restored.rect.height / 2);
  await delay(1500);
}

await writeJson(`${outputDir}/frames.json`, { before, frames, toasts: JSON.parse(toasts) });
console.log(JSON.stringify({ before, frames: frames.map((frame) => ({ phase: frame.phase, checked: frame.switch?.checked })), toasts: JSON.parse(toasts) }, null, 2));
cdp.close();
