#!/usr/bin/env node
// Captures the geometry, computed styles and screenshot of the settings
// content region, so GPUI can be rebuilt against measured values.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/regions");
const name = option("name", "region");
const needle = option("anchor", "插件");

const cdp = await Cdp.connect(endpoint);

const region = JSON.parse(
  await cdp.evaluate(`JSON.stringify((() => {
    const anchorText = ${JSON.stringify(needle)};
    const candidates = [...document.querySelectorAll('div, section, main')]
      .filter((node) => (node.innerText || '').includes(anchorText));
    if (candidates.length === 0) return null;
    // The innermost container that still spans the settings content column.
    const column = candidates
      .map((node) => ({ node, rect: node.getBoundingClientRect() }))
      .filter((entry) => entry.rect.width >= 700 && entry.rect.width <= 1200 && entry.rect.height > 400)
      .sort((left, right) => left.rect.width - right.rect.width)[0];
    if (!column) return null;
    const rect = column.rect;
    const nodes = [];
    const walk = (node, depth) => {
      if (depth > 14 || nodes.length > 900) return;
      const nodeRect = node.getBoundingClientRect();
      if (nodeRect.width === 0 || nodeRect.height === 0) return;
      const style = getComputedStyle(node);
      nodes.push({
        depth,
        tag: node.tagName.toLowerCase(),
        role: node.getAttribute('role'),
        aria: node.getAttribute('aria-label'),
        placeholder: node.getAttribute('placeholder'),
        text: (node.childElementCount === 0 ? (node.textContent || '').trim() : '').slice(0, 70),
        rect: {
          x: Math.round(nodeRect.x - rect.x),
          y: Math.round(nodeRect.y - rect.y),
          width: Math.round(nodeRect.width),
          height: Math.round(nodeRect.height),
        },
        style: {
          fontSize: style.fontSize,
          fontWeight: style.fontWeight,
          lineHeight: style.lineHeight,
          color: style.color,
          background: style.backgroundColor,
          radius: style.borderRadius,
          border: style.borderWidth + ' ' + style.borderColor,
          padding: style.padding,
          gap: style.gap,
          display: style.display,
        },
      });
      for (const child of node.children) walk(child, depth + 1);
    };
    walk(column.node, 0);
    return {
      rect: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) },
      text: (column.node.innerText || '').slice(0, 3000),
      nodes,
    };
  })())`),
);

if (!region) throw new Error(`content region not found for anchor ${needle}`);

await writeJson(`${outputDir}/${name}.json`, region);
await cdp.screenshot(`${outputDir}/${name}.png`, {
  clip: { x: region.rect.x, y: region.rect.y, width: region.rect.width, height: region.rect.height },
});
await delay(50);
console.log(
  JSON.stringify({ name, rect: region.rect, nodeCount: region.nodes.length, text: region.text.slice(0, 600) }, null, 2),
);
cdp.close();
