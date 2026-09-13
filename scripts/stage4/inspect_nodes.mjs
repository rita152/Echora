#!/usr/bin/env node
// Dumps geometry and computed styles for every element whose text matches a
// pattern, anywhere in the reference document.
import { Cdp, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const pattern = option("pattern", ".*");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/recon");
const name = option("name", "inspect");

const cdp = await Cdp.connect(endpoint);
const result = JSON.parse(
  await cdp.evaluate(`JSON.stringify((() => {
    const expression = new RegExp(${JSON.stringify(pattern)});
    const matches = [...document.querySelectorAll('*')].filter((node) => {
      const text = (node.textContent || '').trim();
      if (!expression.test(text)) return false;
      const rect = node.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    });
    return matches.slice(-24).map((node) => {
      const rect = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      return {
        tag: node.tagName.toLowerCase(),
        role: node.getAttribute('role'),
        aria: node.getAttribute('aria-label'),
        text: (node.childElementCount === 0 ? (node.textContent || '').trim() : '').slice(0, 60),
        fullText: (node.textContent || '').trim().slice(0, 80),
        rect: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
        style: {
          fontSize: style.fontSize, fontWeight: style.fontWeight, lineHeight: style.lineHeight,
          color: style.color, background: style.backgroundColor, radius: style.borderRadius,
          border: style.borderWidth + ' ' + style.borderColor, padding: style.padding, gap: style.gap,
          display: style.display, alignItems: style.alignItems, boxShadow: style.boxShadow.slice(0, 50),
        },
      };
    });
  })())`),
);

await writeJson(`${outputDir}/${name}.json`, result);
for (const node of result) {
  console.log(
    `${node.rect.x},${node.rect.y} ${node.rect.width}x${node.rect.height} ${node.tag}${
      node.role ? `[${node.role}]` : ""
    } ${node.style.fontSize}/${node.style.fontWeight} ${node.style.color} bg=${node.style.background} r=${node.style.radius} :: ${node.fullText}`,
  );
}
cdp.close();
