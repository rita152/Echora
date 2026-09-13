#!/usr/bin/env node
// Advances the reference client's onboarding by clicking real labels and
// primary buttons, so the skills/MCP pages can be reached for capture.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/recon");
const limit = Number(option("steps", "16"));

const cdp = await Cdp.connect(endpoint);

const OPTION_RECTS = `
JSON.stringify([...document.querySelectorAll('input[type="radio"]')].map((input, index) => {
  const label = input.closest('label') || input.parentElement;
  const rect = label.getBoundingClientRect();
  return {
    index,
    checked: input.checked,
    text: (label.textContent || '').trim().slice(0, 24),
    x: Math.round(rect.x + rect.width / 2),
    y: Math.round(rect.y + rect.height / 2),
    width: Math.round(rect.width),
    height: Math.round(rect.height),
  };
}))`;

const PRIMARY = `
JSON.stringify((() => {
  const pattern = /继续|下一步|完成|开始使用|Continue|Next|Get started|Done/i;
  const node = [...document.querySelectorAll('button')].find((button) =>
    pattern.test((button.textContent || '').trim()) && !button.disabled,
  );
  if (!node) return null;
  const rect = node.getBoundingClientRect();
  return {
    label: (node.textContent || '').trim(),
    x: Math.round(rect.x + rect.width / 2),
    y: Math.round(rect.y + rect.height / 2),
  };
})())`;

const TEXT_FIELD = `
JSON.stringify((() => {
  const node = document.querySelector('input[aria-label], textarea');
  if (!node) return null;
  const rect = node.getBoundingClientRect();
  return { label: node.getAttribute('aria-label'), x: Math.round(rect.x), y: Math.round(rect.y + rect.height / 2) };
})())`;

const steps = [];
for (let index = 0; index < limit; index += 1) {
  const options = JSON.parse(await cdp.evaluate(OPTION_RECTS));
  const textField = JSON.parse((await cdp.evaluate(TEXT_FIELD)) ?? "null");
  let primary = JSON.parse((await cdp.evaluate(PRIMARY)) ?? "null");
  steps.push({
    index,
    primary,
    options: options.map(({ index: i, text, checked, width }) => ({ i, text, checked, width })),
    textField,
  });
  if (options.length > 0 && options.every((option) => !option.checked)) {
    const first = options.find((option) => option.width > 4) ?? options[0];
    await cdp.click(first.x, first.y);
    await delay(600);
    primary = JSON.parse((await cdp.evaluate(PRIMARY)) ?? "null");
  } else if (textField && /描述其他内容/.test(textField.label ?? "") && !primary) {
    const filled = JSON.parse(await cdp.evaluate(TEXT_FIELD));
    if (filled) {
      await cdp.click(filled.x + 4, filled.y);
      await cdp.send("Input.insertText", { text: "工程" });
      await delay(300);
      primary = JSON.parse((await cdp.evaluate(PRIMARY)) ?? "null");
    }
  }
  if (!primary) break;
  await cdp.click(primary.x, primary.y);
  await delay(1600);
}

await writeJson(`${outputDir}/onboarding-advance.json`, steps);
await cdp.screenshot(`${outputDir}/onboarding-advanced.png`);
console.log(JSON.stringify({ steps: steps.length, last: steps.at(-1) }, null, 2));
cdp.close();
