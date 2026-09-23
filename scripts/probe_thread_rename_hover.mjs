// Hover states of the rename panel's three controls, in both themes.
import fs from 'node:fs';
import { connect, delay, option, setTheme } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const output = option('output', 'artifacts/thread-rename-20260922/probe');
fs.mkdirSync(output, { recursive: true });

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;
const cdp = await connect(endpoint);

async function centerOf(label) {
  return JSON.parse(await cdp.evaluate(`(() => {
    const dialog = ${DIALOG_QUERY};
    const target = [...dialog.querySelectorAll('button')].find((el) => ((el.getAttribute('aria-label') || el.textContent || '').trim()) === ${JSON.stringify(label)});
    if (!target) return null;
    const rect = target.getBoundingClientRect();
    return JSON.stringify({ point: [rect.x + rect.width / 2, rect.y + rect.height / 2], rect: [rect.x, rect.y, rect.width, rect.height] });
  })()`));
}

async function styleOf(label) {
  return JSON.parse(await cdp.evaluate(`(() => {
    const dialog = ${DIALOG_QUERY};
    const target = [...dialog.querySelectorAll('button')].find((el) => ((el.getAttribute('aria-label') || el.textContent || '').trim()) === ${JSON.stringify(label)});
    if (!target) return null;
    const style = getComputedStyle(target);
    return JSON.stringify({
      backgroundColor: style.backgroundColor,
      borderColor: style.borderColor,
      color: style.color,
      boxShadow: style.boxShadow,
      opacity: style.opacity,
      outline: style.outline,
      padding: style.padding,
      borderRadius: style.borderRadius,
      transition: style.transitionProperty + ' ' + style.transitionDuration,
    });
  })()`));
}

async function move(x, y) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: Math.round(x), y: Math.round(y) });
  await delay(350);
}

const report = {};
for (const theme of ['dark', 'light']) {
  await setTheme(cdp, theme);
  await delay(400);
  const rows = JSON.parse(await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
    const rect = row.getBoundingClientRect();
    return { threadId: row.getAttribute('data-app-action-sidebar-thread-id'), selected: row.getAttribute('data-app-action-sidebar-thread-selected'), rect: [rect.x, rect.y, rect.width, rect.height] };
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
  await delay(400);

  const themeReport = { resting: {}, hovered: {}, geometry: {} };
  for (const label of ['Cancel', 'Save', 'Close dialog']) {
    themeReport.geometry[label] = await centerOf(label);
    themeReport.resting[label] = await styleOf(label);
  }
  // Park the pointer inside the dialog but away from every control first, so
  // each hover measurement starts from the resting state.
  await move(600, 470);
  for (const label of ['Cancel', 'Save', 'Close dialog']) {
    const target = themeReport.geometry[label];
    await move(target.point[0], target.point[1]);
    themeReport.hovered[label] = await styleOf(label);
    await move(600, 470);
  }
  // Keyboard focus rings: Tab reaches the controls in DOM order.
  await cdp.evaluate(`(() => { const input = ${DIALOG_QUERY}.querySelector('input'); input.focus(); })()`);
  await delay(200);
  await cdp.key('Tab');
  await delay(300);
  themeReport.afterTab = JSON.parse(await cdp.evaluate(`(() => {
    const active = document.activeElement;
    const style = getComputedStyle(active);
    return JSON.stringify({ tag: active.tagName.toLowerCase(), text: (active.textContent || '').trim(), boxShadow: style.boxShadow, outline: style.outline, backgroundColor: style.backgroundColor });
  })()`));
  report[theme] = themeReport;

  await cdp.key('Escape');
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (!(await cdp.evaluate(`!!${DIALOG_QUERY}`))) break;
    await delay(100);
  }
}

fs.writeFileSync(`${output}/hover.json`, JSON.stringify(report, null, 1));
console.log(JSON.stringify(report, null, 1));
cdp.socket.close();
