// Measures how bright the reference's backdrop is behind the rename panel for
// each candidate task. A task whose dialog sits over a dark, text-free stretch
// of transcript keeps the panel's translucent surface closest to the native
// render, which is what the pixel comparison needs.
import fs from 'node:fs';
import { connect, delay, option, setTheme } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const output = option('output', 'artifacts/thread-rename/probe');
fs.mkdirSync(output, { recursive: true });

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;
const theme = option('theme', 'dark');
const cdp = await connect(endpoint);
await setTheme(cdp, theme);
await delay(400);

async function rows() {
  return JSON.parse(await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
    const rect = row.getBoundingClientRect();
    return { title: row.getAttribute('data-app-action-sidebar-thread-title'), rect: [rect.x, rect.y, rect.width, rect.height] };
  })))()`));
}

async function capturePanel(title) {
  const row = (await rows()).find((entry) => entry.title === title);
  if (!row) return null;
  const x = Math.round(row.rect[0] + 60);
  const y = Math.round(row.rect[1] + row.rect[3] / 2);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1 });
  await delay(60);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 2 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 2 });
  for (let attempt = 0; attempt < 40; attempt += 1) {
    if (await cdp.evaluate(`!!${DIALOG_QUERY}`)) break;
    await delay(100);
  }
  await delay(1200);
  const file = `${output}/backdrop-${title.replace(/[^\w\u4e00-\u9fa5-]/g, '_')}.png`;
  await cdp.screenshot(file);
  await cdp.key('Escape');
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (!(await cdp.evaluate(`!!${DIALOG_QUERY}`))) break;
    await delay(100);
  }
  await delay(400);
  return file;
}

const titles = option('threads', '94,74,59,49,44').split(',');
const report = {};
for (const title of titles) {
  const file = await capturePanel(title);
  report[title] = file;
  console.log(title, file);
}
fs.writeFileSync(`${output}/backdrop-report.json`, JSON.stringify(report, null, 1));
cdp.socket.close();
