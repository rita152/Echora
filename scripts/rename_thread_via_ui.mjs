// Renames one task through the reference app's own rename panel, then confirms
// the sidebar shows the new title. Used to repair probe data.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9361 \
//     node scripts/rename_thread_via_ui.mjs --from="old title" --to="new title"
import { connect, delay, option, setTheme } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const from = option('from');
const fromId = option('from-id');
const to = option('to');
if ((!from && !fromId) || !to) throw Error('pass --from=TITLE or --from-id=ID, and --to=TITLE');
const theme = option('theme', 'dark');

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;
const cdp = await connect(endpoint);
await setTheme(cdp, theme);
await delay(400);

async function rows() {
  return JSON.parse(await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
    const rect = row.getBoundingClientRect();
    return { title: row.getAttribute('data-app-action-sidebar-thread-title'), threadId: row.getAttribute('data-app-action-sidebar-thread-id'), rect: [rect.x, rect.y, rect.width, rect.height] };
  })))()`));
}

const list = await rows();
const row = fromId
  ? list.find((entry) => entry.threadId === fromId)
  : list.find((entry) => entry.title === from);
if (!row) throw new Error('no task named ' + (from ?? fromId));
const x = Math.round(row.rect[0] + 60);
const y = Math.round(row.rect[1] + row.rect[3] / 2);
await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1 });
await delay(60);
await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 2 });
await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 2 });
let opened = false;
for (let attempt = 0; attempt < 40; attempt += 1) {
  if (await cdp.evaluate(`!!${DIALOG_QUERY}`)) { opened = true; break; }
  await delay(100);
}
if (!opened) throw new Error('rename panel never appeared');
await cdp.evaluate(`(() => {
  const input = ${DIALOG_QUERY}.querySelector('input');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(input, ${JSON.stringify(to)});
  input.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
await delay(200);
const save = JSON.parse(await cdp.evaluate(`(() => {
  const button = [...${DIALOG_QUERY}.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Save');
  const rect = button.getBoundingClientRect();
  return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]);
})()`));
await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: Math.round(save[0]), y: Math.round(save[1]), button: 'left', buttons: 1, clickCount: 1 });
await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: Math.round(save[0]), y: Math.round(save[1]), button: 'left', buttons: 0, clickCount: 1 });
await delay(1500);
const after = await rows();
console.log('titles:', JSON.stringify(after.map((entry) => entry.title)));
console.log(after.some((entry) => entry.title === to) ? 'renamed' : 'RENAME FAILED');
cdp.socket.close();
