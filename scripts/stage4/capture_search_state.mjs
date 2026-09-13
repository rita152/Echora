#!/usr/bin/env node
// Types into the reference plugin/skill search field and captures the result,
// including the empty state.
import { Cdp, delay, option } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/search");
const query = option("query", "nnn-no-such-server");
const name = option("name", "search");

const cdp = await Cdp.connect(endpoint);

const field = JSON.parse(
  (await cdp.evaluate(`JSON.stringify((() => {
    const node = document.querySelector('input[type="search"], input[type="text"]');
    if (!node) return null;
    const rect = node.getBoundingClientRect();
    return { placeholder: node.getAttribute('placeholder'), x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  })())`)) ?? "null",
);
if (!field) throw new Error("search field not found");

await cdp.click(field.x, field.y);
await cdp.send("Input.insertText", { text: query });
await delay(1200);

const state = JSON.parse(
  await cdp.evaluate(`JSON.stringify({
    text: (document.body.innerText || '').slice(0, 1200),
    empty: [...document.querySelectorAll('div, p, span')].filter((node) => node.childElementCount === 0 && /没有|无匹配|未找到|No results|Nothing/.test(node.textContent || '')).map((node) => (node.textContent || '').trim().slice(0, 80)).slice(-6),
  })`),
);

await cdp.screenshot(`${outputDir}/${name}.png`);
// Clear the field so the fixture returns to its default listing.
await cdp.evaluate("(() => { const node = document.querySelector('input[type=\"search\"], input[type=\"text\"]'); if (node) node.focus(); })()");
for (let index = 0; index < query.length; index += 1) {
  await cdp.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8, nativeVirtualKeyCode: 51 });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Backspace", code: "Backspace", windowsVirtualKeyCode: 8, nativeVirtualKeyCode: 51 });
}
await delay(800);
console.log(JSON.stringify(state, null, 2).slice(0, 1800));
cdp.close();
