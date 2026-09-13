#!/usr/bin/env node
// Reports the interactive surface of the reference client so onboarding can be
// driven deterministically before capturing skills/MCP pages.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/recon");
const advance = option("advance") === "1";
const limit = Number(option("steps", "12"));

const cdp = await Cdp.connect(endpoint);

const SURFACE = `
JSON.stringify({
  text: (document.body.innerText || '').slice(0, 1200),
  controls: [...document.querySelectorAll('button, [role="button"], [role="radio"], input, [role="checkbox"]')]
    .filter((node) => node.offsetParent !== null)
    .map((node) => {
      const rect = node.getBoundingClientRect();
      return {
        tag: node.tagName.toLowerCase(),
        role: node.getAttribute('role'),
        label: (node.getAttribute('aria-label') || '').slice(0, 40),
        text: (node.textContent || '').trim().slice(0, 40),
        disabled: node.disabled === true || node.getAttribute('aria-disabled') === 'true',
        checked: node.getAttribute('aria-checked'),
        x: Math.round(rect.x + rect.width / 2),
        y: Math.round(rect.y + rect.height / 2),
        w: Math.round(rect.width),
        h: Math.round(rect.height),
      };
    }),
})`;

const steps = [];
const primaryPattern = /继续|下一步|完成|开始使用|Continue|Next|Get started|Done|跳过|Skip/i;

for (let index = 0; index < (advance ? limit : 1); index += 1) {
  let state = JSON.parse(await cdp.evaluate(SURFACE));
  steps.push({ index, phase: "before", state });

  // Onboarding steps gate their primary button on a selection or free text
  // field; make one deterministic choice before advancing.
  if (
    advance &&
    state.controls.some((control) => control.disabled) &&
    !state.controls.some((control) => primaryPattern.test(control.text || control.label) && !control.disabled)
  ) {
    const choice = state.controls.find(
      (control) =>
        !control.disabled &&
        (control.role === "radio" || /工程|Engineering|跳过|Skip/i.test(control.text)),
    );
    if (choice) {
      await cdp.click(choice.x, choice.y);
      await delay(600);
      state = JSON.parse(await cdp.evaluate(SURFACE));
      steps.push({ index, phase: "after-choice", state });
    }
  }

  const primary = state.controls.find(
    (control) => !control.disabled && primaryPattern.test(control.text || control.label),
  );
  if (!primary) break;
  await cdp.click(primary.x, primary.y);
  await delay(1500);
}

const final = JSON.parse(await cdp.evaluate(SURFACE));
await writeJson(`${outputDir}/onboarding.json`, { steps, final });
await cdp.screenshot(`${outputDir}/onboarding-final.png`);
console.log(
  JSON.stringify(
    { steps: steps.length, finalText: final.text.slice(0, 400), controls: final.controls.slice(0, 25) },
    null,
    2,
  ),
);
cdp.close();
