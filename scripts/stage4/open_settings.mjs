#!/usr/bin/env node
// Opens the reference client's Settings surface through the profile menu and
// reports the settings navigation that is actually available.
import { Cdp, delay, option, writeJson } from "./cdp_client.mjs";

const endpoint = option("endpoint", "http://127.0.0.1:9335");
const outputDir = option("output", "artifacts/skills-mcp-stage4/chatgpt/recon");

const cdp = await Cdp.connect(endpoint);

const RECT = (selectorExpression) => `
JSON.stringify((() => {
  const node = ${selectorExpression};
  if (!node) return null;
  const rect = node.getBoundingClientRect();
  return {
    text: (node.textContent || '').trim().slice(0, 40),
    x: rect.x + rect.width / 2,
    y: rect.y + rect.height / 2,
  };
})())`;

const profile = JSON.parse(
  (await cdp.evaluate(
    RECT("[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label')||'').includes('个人资料菜单'))"),
  )) ?? "null",
);
if (!profile) throw new Error("profile menu button not found");
await cdp.click(profile.x, profile.y);
await delay(1000);

const menuItems = JSON.parse(
  await cdp.evaluate(
    "JSON.stringify([...document.querySelectorAll('[role=\"menuitem\"], [role=\"menu\"] button, [role=\"menu\"] a')].map((node) => { const rect = node.getBoundingClientRect(); return { text: (node.textContent||'').trim().slice(0, 30), x: rect.x + rect.width/2, y: rect.y + rect.height/2 }; }))",
  ),
);

const settings = menuItems.find((item) => item.text.startsWith("设置") || item.text.startsWith("Settings"));
if (!settings) throw new Error(`settings entry not found in ${JSON.stringify(menuItems)}`);
await cdp.click(settings.x, settings.y);
await delay(2500);

const summary = JSON.parse(
  await cdp.evaluate(`JSON.stringify({
    text: (document.body.innerText || '').slice(0, 400),
    navItems: [...document.querySelectorAll('nav button, nav a, [role="tab"]')].map((node) => (node.textContent || '').trim()).filter(Boolean),
    headings: [...document.querySelectorAll('h1, h2, h3')].map((node) => (node.textContent || '').trim()).slice(0, 20),
  })`),
);

await writeJson(`${outputDir}/settings.json`, { menuItems, summary });
await cdp.screenshot(`${outputDir}/settings.png`);
console.log(JSON.stringify({ menuItems, summary }, null, 2));
cdp.close();
