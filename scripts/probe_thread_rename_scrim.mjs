// Measures whether the rename panel dims what is behind it: the same frame is
// captured with and without the panel, and the pixels outside the panel's own
// rectangle are compared.
import fs from 'node:fs';
import { connect, delay, option } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const output = option('output', 'artifacts/thread-rename-20260922/probe');
fs.mkdirSync(output, { recursive: true });

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;

const cdp = await connect(endpoint);
await delay(500);

const rows = JSON.parse(await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
  const rect = row.getBoundingClientRect();
  return { title: row.getAttribute('data-app-action-sidebar-thread-title'), threadId: row.getAttribute('data-app-action-sidebar-thread-id'), selected: row.getAttribute('data-app-action-sidebar-thread-selected'), rect: [rect.x, rect.y, rect.width, rect.height] };
})))()`));
const row = rows.find((entry) => entry.selected !== 'true') ?? rows.at(-1);
const x = Math.round(row.rect[0] + 60);
const y = Math.round(row.rect[1] + row.rect[3] / 2);

await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 2 });
await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 2 });
for (let attempt = 0; attempt < 40; attempt += 1) {
  if (await cdp.evaluate(`!!${DIALOG_QUERY}`)) break;
  await delay(100);
}
await delay(700);
const rect = JSON.parse(await cdp.evaluate(`(() => { const r = ${DIALOG_QUERY}.getBoundingClientRect(); return JSON.stringify([r.x, r.y, r.width, r.height]); })()`));
const withPanel = await cdp.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
fs.writeFileSync(`${output}/scrim-with-panel.png`, Buffer.from(withPanel.data, 'base64'));

await cdp.key('Escape');
for (let attempt = 0; attempt < 30; attempt += 1) {
  if (!(await cdp.evaluate(`!!${DIALOG_QUERY}`))) break;
  await delay(100);
}
await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1200, y: 860 });
await delay(700);
const withoutPanel = await cdp.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
fs.writeFileSync(`${output}/scrim-without-panel.png`, Buffer.from(withoutPanel.data, 'base64'));

fs.writeFileSync(`${output}/scrim-rect.json`, JSON.stringify({ rect, row }, null, 1));
console.log('panel rect', JSON.stringify(rect), 'row', row.title);
cdp.socket.close();
