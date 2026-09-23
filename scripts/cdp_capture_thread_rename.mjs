// Capture the sidebar task rename panel from a dedicated ChatGPT reference
// instance.
//
// The script never rewrites React state or CSS: it dispatches the same
// native double click a person makes on a task row, waits for the dialog the
// app itself renders, then records geometry, computed styles, the rendered
// markup, screenshots, and what each control does.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9361 \
//     node scripts/cdp_capture_thread_rename.mjs --output=artifacts/thread-rename/reference
//
// Options:
//   --output=DIR      artifact directory (default artifacts/thread-rename)
//   --theme=light     capture one theme; omitted captures light then dark
//   --thread=TITLE    task row to double click (default: the current task)
//   --dpr=1           force the page device pixel ratio for the capture
//   --scale=2         screenshot scale for the crops
import fs from 'node:fs';
import { connect, delay, option, referencePid, resizeWindow, setTheme, waitFor } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = option('output', 'artifacts/thread-rename');
const requestedTheme = option('theme');
const requestedThread = option('thread');
const requestedDpr = option('dpr');
const scale = Number(option('scale', '2'));
fs.mkdirSync(output, { recursive: true });

const STYLE_PROBES = [
  'display', 'position', 'width', 'height', 'minWidth', 'maxWidth', 'minHeight', 'maxHeight',
  'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft', 'marginTop', 'marginRight',
  'marginBottom', 'marginLeft', 'rowGap', 'columnGap',
  'fontFamily', 'fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'textAlign',
  'color', 'backgroundColor', 'backgroundImage', 'borderRadius', 'borderColor', 'borderWidth',
  'borderTopWidth', 'borderTopColor', 'boxShadow', 'opacity', 'overflow', 'flexDirection',
  'alignItems', 'justifyContent', 'flexShrink', 'flexGrow', 'whiteSpace', 'textOverflow',
  'backdropFilter', 'zIndex', 'transform', 'pointerEvents', 'cursor', 'inset', 'outline',
];

const DESCRIBE = `
  (element) => {
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    const styles = {};
    for (const name of ${JSON.stringify(STYLE_PROBES)}) styles[name] = style[name];
    const node = {
      tag: element.tagName.toLowerCase(),
      role: element.getAttribute('role'),
      label: element.getAttribute('aria-label'),
      placeholder: element.getAttribute('placeholder'),
      type: element.getAttribute('type'),
      cls: (element.className || '').toString(),
      dataState: element.getAttribute('data-state'),
      inlineStyle: element.getAttribute('style'),
      text: [...element.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent).join('').trim(),
      value: typeof element.value === 'string' ? element.value : undefined,
      disabled: element.disabled,
      rect: [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100),
      styles,
    };
    const svg = [...element.children].find((child) => child.tagName === 'svg');
    if (svg) node.svg = svg.outerHTML;
    if (element.children.length) node.children = [...element.children].map(DESCRIBE_SELF);
    return node;
  }
`;

const DESCRIBE_SELF = '__describe';
const describeFunction = `const ${DESCRIBE_SELF} = ${DESCRIBE.replace(/DESCRIBE_SELF/g, DESCRIBE_SELF)};`;

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`;
const DIALOG_QUERY = `document.querySelector('[role=dialog]')`;

async function threadRows(cdp) {
  return JSON.parse(
    await cdp.evaluate(`(() => JSON.stringify(${ROW_QUERY}.map((row) => {
      const rect = row.getBoundingClientRect();
      return {
        title: row.getAttribute('data-app-action-sidebar-thread-title'),
        threadId: row.getAttribute('data-app-action-sidebar-thread-id'),
        kind: row.getAttribute('data-app-action-sidebar-thread-kind'),
        selected: row.getAttribute('data-app-action-sidebar-thread-selected'),
        active: row.getAttribute('data-app-action-sidebar-thread-active'),
        rect: [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100),
      };
    })))()`),
  );
}

async function nativeDoubleClick(cdp, x, y, gapMs = 60) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await delay(40);
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1,
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1,
  });
  await delay(gapMs);
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 2,
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 2,
  });
}

async function singleClick(cdp, x, y) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await delay(40);
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1,
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1,
  });
}

async function waitForDialog(cdp, { timeoutMs = 4000, present = true } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const open = await cdp.evaluate(`!!${DIALOG_QUERY}`);
    if (open === present) return true;
    await delay(80);
  }
  return false;
}

async function dumpDialog(cdp) {
  return JSON.parse(
    await cdp.evaluate(`(() => {
      ${describeFunction}
      const dialog = ${DIALOG_QUERY};
      if (!dialog) return JSON.stringify({ found: false });
      const input = dialog.querySelector('input');
      const overlay = [...document.body.children].find((el) => {
        const style = getComputedStyle(el);
        return style.position === 'fixed' && !el.contains(dialog);
      });
      const active = document.activeElement;
      return JSON.stringify({
        found: true,
        viewport: [innerWidth, innerHeight, devicePixelRatio],
        theme: document.documentElement.getAttribute('data-theme'),
        rect: (() => { const r = dialog.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map((v) => Math.round(v * 100) / 100); })(),
        activeElement: active ? { tag: active.tagName.toLowerCase(), label: active.getAttribute('aria-label') } : null,
        selection: input ? { start: input.selectionStart, end: input.selectionEnd, length: input.value.length } : null,
        overlay: overlay ? { tag: overlay.tagName.toLowerCase(), cls: (overlay.className || '').toString(), styles: (() => { const s = getComputedStyle(overlay); return { position: s.position, inset: s.inset, backgroundColor: s.backgroundColor, backdropFilter: s.backdropFilter, zIndex: s.zIndex }; })() } : null,
        root: ${DESCRIBE_SELF}(dialog),
        innerText: dialog.innerText,
        innerHTML: dialog.outerHTML,
      });
    })()`),
  );
}

async function captureRegion(cdp, file, rect, { padding = 16 } = {}) {
  const viewport = await cdp.evaluate('JSON.stringify([innerWidth, innerHeight])').then(JSON.parse);
  const x = Math.max(0, Math.floor(rect[0] - padding));
  const y = Math.max(0, Math.floor(rect[1] - padding));
  const width = Math.min(viewport[0] - x, Math.ceil(rect[2] + padding * 2));
  const height = Math.min(viewport[1] - y, Math.ceil(rect[3] + padding * 2));
  const shot = await cdp.send('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: false,
    clip: { x, y, width, height, scale },
  });
  fs.writeFileSync(file, Buffer.from(shot.data, 'base64'));
  return { file, clip: [x, y, width, height], scale };
}

async function controlRect(cdp, label) {
  const raw = await cdp.evaluate(`(() => {
    const dialog = ${DIALOG_QUERY};
    if (!dialog) return null;
    const target = [...dialog.querySelectorAll('button, input')].find((el) => {
      const text = (el.getAttribute('aria-label') || el.textContent || el.getAttribute('placeholder') || '').trim();
      return text === ${JSON.stringify(label)};
    });
    if (!target) return null;
    const rect = target.getBoundingClientRect();
    return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]);
  })()`);
  return raw ? JSON.parse(raw) : null;
}

async function setInputValue(cdp, value) {
  return cdp.evaluate(`(() => {
    const input = ${DIALOG_QUERY}?.querySelector('input');
    if (!input) return false;
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(input, ${JSON.stringify(value)});
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  })()`);
}

async function dialogState(cdp) {
  return JSON.parse(
    await cdp.evaluate(`(() => {
      const dialog = ${DIALOG_QUERY};
      if (!dialog) return JSON.stringify({ open: false });
      const input = dialog.querySelector('input');
      const save = [...dialog.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Save');
      return JSON.stringify({
        open: true,
        value: input ? input.value : null,
        selection: input ? [input.selectionStart, input.selectionEnd] : null,
        saveDisabled: save ? save.disabled : null,
        saveAriaDisabled: save ? save.getAttribute('aria-disabled') : null,
      });
    })()`),
  );
}

/// Double clicks a task row and waits for the panel.
async function openPanelFor(cdp, threadId) {
  const rows = await threadRows(cdp);
  const row = rows.find((entry) => entry.threadId === threadId);
  if (!row) throw new Error('no row for ' + threadId);
  const point = [Math.round(row.rect[0] + 60), Math.round(row.rect[1] + row.rect[3] / 2)];
  await nativeDoubleClick(cdp, point[0], point[1]);
  if (!(await waitForDialog(cdp))) throw new Error('rename panel never appeared for ' + row.title);
  await delay(400);
  return { row, point };
}

async function closePanel(cdp) {
  await cdp.key('Escape');
  const closed = await waitForDialog(cdp, { present: false, timeoutMs: 2500 });
  await delay(300);
  return closed;
}

const cdp = await connect(endpoint);
await waitFor(cdp, `${ROW_QUERY}.length > 0`, { timeoutMs: 90000 });
const size = await resizeWindow(cdp, 1440, 900).catch((error) => `resize skipped: ${error.message}`);
console.log('window', size, 'pid', referencePid());
if (requestedDpr) {
  const dpr = Number(requestedDpr);
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width: 1440,
    height: 900,
    deviceScaleFactor: dpr,
    mobile: false,
  });
  await delay(600);
  console.log('viewport', await cdp.evaluate('innerWidth + "x" + innerHeight + "@" + devicePixelRatio'));
}

// One theme per run must not drop the other theme's recording, so an existing
// report is merged rather than replaced.
const reportPath = `${output}/report.json`;
const report = fs.existsSync(reportPath)
  ? JSON.parse(fs.readFileSync(reportPath, 'utf8'))
  : { endpoint, capturedAt: new Date().toISOString(), window: size, themes: {}, behavior: {} };
report.endpoint = endpoint;
report.window = size;
report.themes ??= {};
report.behavior ??= {};

for (const theme of requestedTheme ? [requestedTheme] : ['light', 'dark']) {
  await setTheme(cdp, theme);
  await delay(500);
  let rows = await threadRows(cdp);
  const currentRow = rows.find((row) => row.selected === 'true') ?? rows.at(-1);
  const target = requestedThread
    ? rows.find((row) => row.title === requestedThread)
    : currentRow;
  if (!target) throw new Error(`no task row named ${requestedThread}`);
  // The row the capture starts from must not be the open conversation, so the
  // recording covers the two-click path the product uses.
  const start = target === currentRow ? rows.find((row) => row !== currentRow) : currentRow;
  if (start && start !== target) {
    const startPoint = [Math.round(start.rect[0] + 60), Math.round(start.rect[1] + start.rect[3] / 2)];
    await singleClick(cdp, ...startPoint);
    await delay(900);
    rows = await threadRows(cdp);
  }
  const row = rows.find((entry) => entry.threadId === target.threadId);
  if (!row) throw new Error('task row disappeared before the capture');

  const point = [Math.round(row.rect[0] + 60), Math.round(row.rect[1] + row.rect[3] / 2)];
  const beforeOpen = await threadRows(cdp);
  const alreadyCurrent = beforeOpen.find((entry) => entry.threadId === row.threadId)?.selected === 'true';

  // Exactly what a person does: one click selects the task, the second opens
  // the panel. The first click is issued separately so the recording can show
  // that it does not open the panel on its own.
  await singleClick(cdp, ...point);
  const singleOpenedDialog = await cdp.evaluate(`!!${DIALOG_QUERY}`);
  await delay(500);
  const rowsAfterSingle = await threadRows(cdp);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: point[0], y: point[1], button: 'left', buttons: 1, clickCount: 2 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: point[0], y: point[1], button: 'left', buttons: 0, clickCount: 2 });
  if (!(await waitForDialog(cdp))) throw new Error('rename panel never appeared');
  await delay(600);

  const snapshot = await dumpDialog(cdp);
  const full = await captureRegion(cdp, `${output}/${theme}-window.png`, [0, 0, 1440, 900], { padding: 0 });
  const crop = await captureRegion(cdp, `${output}/${theme}-rename-panel.png`, snapshot.rect);
  fs.writeFileSync(`${output}/${theme}-rename-panel.json`, JSON.stringify(snapshot, null, 1));

  report.themes[theme] = {
    thread: { title: row.title, threadId: row.threadId, kind: row.kind },
    row: row.rect,
    pointer: point,
    alreadyCurrent,
    singleClickOpenedDialog: singleOpenedDialog,
    rowsAfterSingle,
    geometry: { panel: snapshot.rect, viewport: snapshot.viewport, theme: snapshot.theme },
    activeElement: snapshot.activeElement,
    selection: snapshot.selection,
    overlay: snapshot.overlay,
    full,
    crop,
  };
  console.log(theme, JSON.stringify(report.themes[theme].geometry), 'alreadyCurrent', alreadyCurrent);

  report.behavior[theme] = {
    markerBefore: row.title,
    emptyInputDisablesSave: await (async () => {
      const before = await dialogState(cdp);
      await setInputValue(cdp, '');
      await delay(200);
      const empty = await dialogState(cdp);
      await setInputValue(cdp, row.title);
      await delay(200);
      return { before, empty };
    })(),
    escapeCloses: await (async () => {
      await cdp.key('Escape');
      const closed = await waitForDialog(cdp, { present: false, timeoutMs: 2500 });
      const after = await threadRows(cdp);
      return { closed, titles: after.map((entry) => entry.title) };
    })(),
  };
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1200, y: 820 });
  await delay(400);
}

// Control behaviour: every path is driven with real input on the same task,
// and the task is left exactly as it was found.
const liveRows = await threadRows(cdp);
const currentRow = liveRows.find((row) => row.selected === 'true') ?? liveRows.at(-1);
const scratch = liveRows.find((row) => row.threadId !== currentRow.threadId) ?? currentRow;
const originalTitle = scratch.title;
const renamedTitle = originalTitle + ' (rename probe)';

async function reopen() {
  return openPanelFor(cdp, scratch.threadId);
}

{
  await reopen();
  const initial = await dialogState(cdp);
  const cancel = await controlRect(cdp, 'Cancel');
  await singleClick(cdp, Math.round(cancel[0]), Math.round(cancel[1]));
  const cancelClosed = await waitForDialog(cdp, { present: false, timeoutMs: 2500 });
  const afterCancel = (await threadRows(cdp)).find((row) => row.threadId === scratch.threadId)?.title;

  await reopen();
  const closeButton = await controlRect(cdp, 'Close dialog');
  await singleClick(cdp, Math.round(closeButton[0]), Math.round(closeButton[1]));
  const closeClosed = await waitForDialog(cdp, { present: false, timeoutMs: 2500 });
  const afterClose = (await threadRows(cdp)).find((row) => row.threadId === scratch.threadId)?.title;

  await reopen();
  // Clicking the backdrop is the fifth way out of this dialog.
  await singleClick(cdp, 40, 860);
  const backdropClosed = await waitForDialog(cdp, { present: false, timeoutMs: 2500 });
  if (!backdropClosed) await closePanel(cdp);
  const afterBackdrop = (await threadRows(cdp)).find((row) => row.threadId === scratch.threadId)?.title;

  await reopen();
  await setInputValue(cdp, renamedTitle);
  await delay(200);
  const beforeEnter = await dialogState(cdp);
  await cdp.key('Enter');
  const enterClosed = await waitForDialog(cdp, { present: false, timeoutMs: 2500 });
  await delay(900);
  const afterEnterRow = (await threadRows(cdp)).find((row) => row.threadId === scratch.threadId);

  // Put the task back exactly as it was found.
  await reopen();
  await setInputValue(cdp, originalTitle);
  await delay(200);
  const save = await controlRect(cdp, 'Save');
  await singleClick(cdp, Math.round(save[0]), Math.round(save[1]));
  const saveClosed = await waitForDialog(cdp, { present: false, timeoutMs: 2500 });
  await delay(900);
  const restoredRow = (await threadRows(cdp)).find((row) => row.threadId === scratch.threadId);

  report.behavior.controls = {
    thread: { threadId: scratch.threadId, originalTitle, renamedTitle },
    initial,
    cancel: { closed: cancelClosed, titleAfter: afterCancel },
    closeButton: { closed: closeClosed, titleAfter: afterClose },
    backdrop: { closed: backdropClosed, titleAfter: afterBackdrop },
    enter: { beforeEnter, closed: enterClosed, titleAfter: afterEnterRow?.title ?? null },
    save: { closed: saveClosed, titleAfter: restoredRow?.title ?? null },
    restored: restoredRow?.title === originalTitle,
  };
  console.log('controls', JSON.stringify(report.behavior.controls));
}

fs.writeFileSync(reportPath, JSON.stringify(report, null, 1));
console.log('wrote', reportPath);
if (requestedDpr) {
  await cdp.send('Emulation.clearDeviceMetricsOverride');
}
cdp.socket.close();
