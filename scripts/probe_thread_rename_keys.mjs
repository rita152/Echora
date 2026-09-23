// What the rename panel does with the keyboard: Enter, Escape, Tab, and what
// the saved title looks like after trimming.
import fs from 'node:fs';
import { connect, delay, option, setTheme } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const output = option('output', 'artifacts/thread-rename-20260922/probe');
fs.mkdirSync(output, { recursive: true });

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;

const cdp = await connect(endpoint);
await setTheme(cdp, 'dark');
await delay(400);

async function rows() {
  return JSON.parse(await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
    const rect = row.getBoundingClientRect();
    return { title: row.getAttribute('data-app-action-sidebar-thread-title'), threadId: row.getAttribute('data-app-action-sidebar-thread-id'), selected: row.getAttribute('data-app-action-sidebar-thread-selected'), rect: [rect.x, rect.y, rect.width, rect.height] };
  })))()`));
}

async function open(threadId) {
  const list = await rows();
  const row = list.find((entry) => entry.threadId === threadId);
  const x = Math.round(row.rect[0] + 60);
  const y = Math.round(row.rect[1] + row.rect[3] / 2);
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

async function close() {
  await cdp.key('Escape');
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (!(await cdp.evaluate(`!!${DIALOG_QUERY}`))) return true;
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

async function realKey(key, code, keyCode) {
  const base = { windowsVirtualKeyCode: keyCode, nativeVirtualKeyCode: keyCode, key, code };
  await cdp.send('Input.dispatchKeyEvent', { ...base, type: 'rawKeyDown' });
  await cdp.send('Input.dispatchKeyEvent', { ...base, type: 'char', text: '\r' });
  await cdp.send('Input.dispatchKeyEvent', { ...base, type: 'keyUp' });
}

async function state() {
  return JSON.parse(await cdp.evaluate(`(() => {
    const input = ${DIALOG_QUERY}?.querySelector('input');
    return JSON.stringify({
      open: !!${DIALOG_QUERY},
      value: input ? input.value : null,
      selection: input ? [input.selectionStart, input.selectionEnd] : null,
      active: document.activeElement ? document.activeElement.tagName.toLowerCase() + ':' + (document.activeElement.getAttribute('aria-label') || document.activeElement.textContent || '').trim().slice(0, 20) : null,
    });
  })()`));
}

const list = await rows();
const current = list.find((row) => row.selected === 'true') ?? list.at(-1);
const scratch = list.find((row) => row.threadId !== current.threadId) ?? current;
const original = scratch.title;
const probe = original + ' probe';
const report = { threadId: scratch.threadId, original, steps: [] };

await open(scratch.threadId);
report.steps.push({ step: 'open', state: await state() });

// Typing replaces the selected title, exactly like the reference selects all
// when the panel opens.
await cdp.send('Input.insertText', { text: probe });
await delay(200);
report.steps.push({ step: 'typed', state: await state(), rows: (await rows()).map((r) => r.title) });

await realKey('Enter', 'Enter', 13);
await delay(1200);
report.steps.push({ step: 'enter', state: await state(), rows: (await rows()).map((r) => r.title) });

if (await cdp.evaluate(`!!${DIALOG_QUERY}`)) {
  // Trim behaviour: does the app strip surrounding whitespace?
  await setValue('   spaced title   ');
  await realKey('Enter', 'Enter', 13);
  await delay(1200);
  report.steps.push({ step: 'enter-spaced', state: await state(), rows: (await rows()).map((r) => r.title) });
}

if (await cdp.evaluate(`!!${DIALOG_QUERY}`)) {
  await setValue(original);
  const save = await cdp.evaluate(`(() => {
    const dialog = ${DIALOG_QUERY};
    const button = [...dialog.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Save');
    const rect = button.getBoundingClientRect();
    return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]);
  })()`);
  const [x, y] = JSON.parse(save);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: Math.round(x), y: Math.round(y), button: 'left', buttons: 1, clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: Math.round(x), y: Math.round(y), button: 'left', buttons: 0, clickCount: 1 });
  await delay(1200);
  report.steps.push({ step: 'save-restore', state: await state(), rows: (await rows()).map((r) => r.title) });
}

await close();
report.final = (await rows()).map((r) => r.title);
fs.writeFileSync(`${output}/keys-report.json`, JSON.stringify(report, null, 1));
console.log(JSON.stringify(report, null, 1));
cdp.socket.close();
