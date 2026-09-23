// How the reference turns typed text into a saved title: whitespace, empty
// input, and long titles. The probe tracks the row by its task id and always
// puts the original title back, so the surrounding data is left as found.
import fs from 'node:fs';
import { connect, delay, option } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const output = option('output', 'artifacts/thread-rename-20260922/probe');
const wanted = option('thread', '94');
fs.mkdirSync(output, { recursive: true });

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;
const cdp = await connect(endpoint);

async function rows() {
  return JSON.parse(await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
    const rect = row.getBoundingClientRect();
    return { title: row.getAttribute('data-app-action-sidebar-thread-title'), threadId: row.getAttribute('data-app-action-sidebar-thread-id'), rect: [rect.x, rect.y, rect.width, rect.height] };
  })))()`));
}

async function open(threadId) {
  const row = (await rows()).find((entry) => entry.threadId === threadId);
  if (!row) throw new Error('no row for ' + threadId);
  const x = Math.round(row.rect[0] + 60);
  const y = Math.round(row.rect[1] + row.rect[3] / 2);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1 });
  await delay(80);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 2 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 2 });
  for (let attempt = 0; attempt < 40; attempt += 1) {
    if (await cdp.evaluate(`!!${DIALOG_QUERY}`)) return true;
    await delay(100);
  }
  return false;
}

async function setValue(value) {
  return cdp.evaluate(`(() => {
    const input = ${DIALOG_QUERY}.querySelector('input');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(input, ${JSON.stringify(value)});
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return input.value;
  })()`);
}

async function save() {
  const point = JSON.parse(await cdp.evaluate(`(() => {
    const button = [...${DIALOG_QUERY}.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Save');
    const rect = button.getBoundingClientRect();
    return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]);
  })()`));
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: Math.round(point[0]), y: Math.round(point[1]), button: 'left', buttons: 1, clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: Math.round(point[0]), y: Math.round(point[1]), button: 'left', buttons: 0, clickCount: 1 });
  await delay(1400);
  const open = await cdp.evaluate(`!!${DIALOG_QUERY}`);
  return open;
}

async function titleOf(threadId) {
  return (await rows()).find((entry) => entry.threadId === threadId)?.title ?? null;
}

async function write(threadId, value) {
  if (!(await open(threadId))) throw new Error('panel never opened');
  await setValue(value);
  await delay(200);
  const stillOpen = await save();
  const title = await titleOf(threadId);
  return { stillOpen, title };
}

const target = (await rows()).find((row) => row.title === wanted);
if (!target) throw new Error('no task named ' + wanted);
const original = target.title;
const report = { threadId: target.threadId, original, steps: [] };

try {
  report.steps.push({ wrote: '  padded  ', ...(await write(target.threadId, '  padded  ')) });
  report.steps.push({ wrote: '   whitespace only', ...(await write(target.threadId, '   ')) });
  report.steps.push({ wrote: 'x200', ...(await write(target.threadId, 'x'.repeat(200))) });
  report.steps.push({ wrote: 'empty', ...(await write(target.threadId, '')) });
  report.steps.push({ wrote: '汉100', ...(await write(target.threadId, '汉'.repeat(100))) });
  report.steps.push({ wrote: 'ascii57+58', ...(await write(target.threadId, 'a'.repeat(57) + 'b'.repeat(3))) });
} finally {
  const current = await titleOf(target.threadId);
  if (current !== original) {
    report.restore = await write(target.threadId, original);
  }
  report.finalTitle = await titleOf(target.threadId);
}

fs.writeFileSync(`${output}/values-report.json`, JSON.stringify(report, null, 1));
console.log(JSON.stringify(report, null, 1));
cdp.socket.close();
