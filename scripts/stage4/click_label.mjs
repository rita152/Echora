#!/usr/bin/env node
// Clicks the first visible control matching a label pattern and reports the
// resulting page text. Used to walk the reference client deterministically.
import { Cdp, delay, option } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const pattern = option("pattern", "");
const index = Number(option("index", "0"));
const settle = Number(option("settle", "1200"));

const cdp = await Cdp.connect(endpoint);
const target = await cdp.evaluate(`
JSON.stringify((() => {
  const expression = new RegExp(${JSON.stringify(pattern)}, 'i');
  const nodes = [...document.querySelectorAll('button, [role="button"], [role="menuitem"], a')]
    .filter((node) => node.offsetParent !== null)
    .filter((node) => expression.test(((node.getAttribute('aria-label') || '') + ' ' + (node.textContent || '')).trim()));
  const node = nodes[${index}];
  if (!node) return null;
  const rect = node.getBoundingClientRect();
  return {
    text: (node.textContent || '').trim().slice(0, 60),
    x: Math.round(rect.x + rect.width / 2),
    y: Math.round(rect.y + rect.height / 2),
    matches: nodes.length,
  };
})())`);

const point = JSON.parse(target ?? "null");
if (!point) {
  console.log(JSON.stringify({ clicked: null, pattern }));
  cdp.close();
  process.exit(0);
}
await cdp.click(point.x, point.y);
await delay(settle);
const text = await cdp.evaluate("(document.body.innerText || '').slice(0, 600)");
console.log(JSON.stringify({ clicked: point, text }, null, 2));
cdp.close();
