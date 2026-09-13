#!/usr/bin/env node
// Captures the reference plugins page (plugins / apps / MCP / skills segments):
// structure, geometry, computed styles and screenshots.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/plugins-page");
const segment = option("segment", null);
const label = option("name", segment ?? "all");

const cdp = await Cdp.connect(endpoint);

// Segment chips are flat buttons whose text concatenates the label and its
// count, for example `MCP6`.
const SEGMENTS = `JSON.stringify((() => {
  const pattern = /^(插件|应用|MCP|技能)\\d*$/;
  const nodes = [...document.querySelectorAll('button, [role="tab"], [role="radio"]')]
    .filter((node) => pattern.test((node.textContent || '').trim()));
  return nodes.map((node) => {
    const rect = node.getBoundingClientRect();
    return { text: (node.textContent || '').trim(), tag: node.tagName.toLowerCase(), role: node.getAttribute('role'),
      x: Math.round(rect.x + rect.width / 2), y: Math.round(rect.y + rect.height / 2),
      width: Math.round(rect.width), height: Math.round(rect.height) };
  });
})())`;

if (segment) {
  const segments = JSON.parse(await cdp.evaluate(SEGMENTS));
  const match = segments.find((entry) => entry.text.startsWith(segment));
  if (!match) throw new Error(`segment not found: ${segment} in ${JSON.stringify(segments)}`);
  await cdp.click(match.x, match.y);
  await delay(2000);
}

const structure = JSON.parse(
  await cdp.evaluate(`JSON.stringify((() => {
    const page = [...document.querySelectorAll('div, section, main')]
      .filter((node) => (node.innerText || '').startsWith('插件\\n管理插件、技能和 MCP'))
      [0] || document.body;
    const nodes = [];
    const walk = (node, depth) => {
      if (depth > 18 || nodes.length > 2500) return;
      const rect = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      nodes.push({
        depth,
        tag: node.tagName.toLowerCase(),
        role: node.getAttribute('role'),
        aria: node.getAttribute('aria-label'),
        placeholder: node.getAttribute('placeholder'),
        text: (node.childElementCount === 0 ? (node.textContent || '') : (node.textContent || '').slice(0, 0)).trim().slice(0, 60),
        rect: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
        style: {
          fontSize: style.fontSize, fontWeight: style.fontWeight, lineHeight: style.lineHeight,
          color: style.color, background: style.backgroundColor, radius: style.borderRadius,
          border: style.borderWidth + ' ' + style.borderColor, padding: style.padding, gap: style.gap,
          display: style.display, alignItems: style.alignItems, fontFamily: style.fontFamily.slice(0, 40),
          boxShadow: style.boxShadow.slice(0, 60),
        },
      });
      for (const child of node.children) walk(child, depth + 1);
    };
    walk(page, 0);
    return { text: (page.innerText || '').slice(0, 4000), rect: (() => { const r = page.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) }; })(), nodes };
  })())`),
);

await writeJson(`${outputDir}/${label}.json`, structure);
await cdp.screenshot(`${outputDir}/${label}.png`);
console.log(JSON.stringify({ label, rect: structure.rect, text: structure.text.slice(0, 1200), nodeCount: structure.nodes.length }, null, 2));
cdp.close();
