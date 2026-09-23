// Looks for a reference state where the rename panel opens over a flat pane:
// the Scheduled/Plugins pages and the New-chat home, which leave the sidebar
// task rows in place.
import fs from 'node:fs';
import { connect, delay, option, setTheme, clickLabel } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const output = option('output', 'artifacts/thread-rename-20260922/probe');
fs.mkdirSync(output, { recursive: true });

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;
const cdp = await connect(endpoint);
await setTheme(cdp, 'dark');
await delay(400);

async function selectedRow() {
  return cdp.evaluate(`(() => {
    const row = ${ROW_QUERY}.find((el) => el.getAttribute('data-app-action-sidebar-thread-selected') === 'true');
    return row ? row.getAttribute('data-app-action-sidebar-thread-title') : null;
  })()`);
}

async function clickRow(title) {
  const point = JSON.parse(await cdp.evaluate(`(() => {
    const row = ${ROW_QUERY}.find((el) => el.getAttribute('data-app-action-sidebar-thread-title') === ${JSON.stringify(title)});
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return JSON.stringify([rect.x + 60, rect.y + rect.height / 2]);
  })()`));
  if (!point) return null;
  await cdp.click(Math.round(point[0]), Math.round(point[1]));
  await delay(1200);
  return point;
}

async function doubleClickRow(title) {
  const point = JSON.parse(await cdp.evaluate(`(() => {
    const row = ${ROW_QUERY}.find((el) => el.getAttribute('data-app-action-sidebar-thread-title') === ${JSON.stringify(title)});
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return JSON.stringify([rect.x + 60, rect.y + rect.height / 2]);
  })()`));
  if (!point) return false;
  const x = Math.round(point[0]);
  const y = Math.round(point[1]);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1 });
  await delay(60);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 2 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 2 });
  for (let attempt = 0; attempt < 40; attempt += 1) {
    if (await cdp.evaluate(`!!${DIALOG_QUERY}`)) return true;
    await delay(100);
  }
  return false;
}

async function dismiss() {
  await cdp.key('Escape');
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (!(await cdp.evaluate(`!!${DIALOG_QUERY}`))) return;
    await delay(100);
  }
}

const target = option('thread', '94');
const report = {};

// 1. Open the task, then leave the task view for another sidebar page.
await clickRow(target);
report.selectedAfterRowClick = await selectedRow();
for (const page of ['Scheduled', 'Plugins']) {
  try {
    await clickLabel(cdp, page);
  } catch (error) {
    report[page] = { error: error.message };
    continue;
  }
  await delay(1200);
  const selected = await selectedRow();
  const opened = await doubleClickRow(target);
  await delay(500);
  await cdp.screenshot(`${output}/flat-${page.toLowerCase()}.png`);
  report[page] = { selectedBeforeDoubleClick: selected, opened, selectedAfter: await selectedRow() };
  if (opened) await dismiss();
}

// 2. New chat, then double click the task row that is still highlighted.
await clickLabel(cdp, 'New chat');
await delay(1200);
const selectedAfterNewChat = await selectedRow();
const openedFromHome = await doubleClickRow(target);
await delay(500);
await cdp.screenshot(`${output}/flat-new-chat.png`);
report.newChat = { selectedAfterNewChat, openedFromHome };
if (openedFromHome) await dismiss();

fs.writeFileSync(`${output}/flat-backdrop.json`, JSON.stringify(report, null, 1));
console.log(JSON.stringify(report, null, 1));
cdp.socket.close();
