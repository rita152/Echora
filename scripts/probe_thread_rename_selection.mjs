// Reads the selection colours the reference paints inside the rename input,
// plus the input's own border/shadow in its focused and unfocused states.
import fs from 'node:fs';
import { connect, delay, option, setTheme } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const output = option('output', 'artifacts/thread-rename-20260922/probe');
fs.mkdirSync(output, { recursive: true });

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;
const cdp = await connect(endpoint);

const report = {};
for (const theme of ['dark', 'light']) {
  await setTheme(cdp, theme);
  await delay(400);
  const rows = JSON.parse(await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
    const rect = row.getBoundingClientRect();
    return { title: row.getAttribute('data-app-action-sidebar-thread-title'), threadId: row.getAttribute('data-app-action-sidebar-thread-id'), selected: row.getAttribute('data-app-action-sidebar-thread-selected'), rect: [rect.x, rect.y, rect.width, rect.height] };
  })))()`));
  const row = rows.find((entry) => entry.selected === 'true') ?? rows.at(-1);
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
  await delay(500);
  report[theme] = JSON.parse(await cdp.evaluate(`(() => {
    const dialog = ${DIALOG_QUERY};
    const input = dialog.querySelector('input');
    const selection = getComputedStyle(input, '::selection');
    const root = getComputedStyle(document.documentElement);
    const style = getComputedStyle(input);
    return JSON.stringify({
      selectionBackground: selection.backgroundColor,
      selectionColor: selection.color,
      borderColor: style.borderColor,
      boxShadow: style.boxShadow,
      caretColor: style.caretColor,
      tokens: {
        ring: root.getPropertyValue('--color-border-ring'),
        selection: root.getPropertyValue('--color-background-selection'),
        soft: root.getPropertyValue('--color-background-primary-soft'),
        solid: root.getPropertyValue('--color-background-primary-solid'),
        solidText: root.getPropertyValue('--color-text-primary-solid'),
        outline: root.getPropertyValue('--color-border-primary-outline'),
        border: root.getPropertyValue('--color-border'),
        elevated: root.getPropertyValue('--color-surface-elevated-secondary'),
        dialogHeading: root.getPropertyValue('--color-text-codex-description'),
      },
    });
  })()`));
  // Blur the input to read the resting border: clicking the dialog's own
  // padding moves focus away without dismissing it.
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: 520, y: 380, button: 'left', buttons: 1, clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: 520, y: 380, button: 'left', buttons: 0, clickCount: 1 });
  await delay(300);
  report[theme].blurred = JSON.parse(await cdp.evaluate(`(() => {
    const dialog = ${DIALOG_QUERY};
    if (!dialog) return JSON.stringify({ closed: true });
    const input = dialog.querySelector('input');
    const style = getComputedStyle(input);
    return JSON.stringify({ closed: false, borderColor: style.borderColor, focused: document.activeElement === input });
  })()`));
  await cdp.key('Escape');
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (!(await cdp.evaluate(`!!${DIALOG_QUERY}`))) break;
    await delay(100);
  }
}

fs.writeFileSync(`${output}/selection.json`, JSON.stringify(report, null, 1));
console.log(JSON.stringify(report, null, 1));
cdp.socket.close();
