// Capture the sidebar project hover card from a dedicated ChatGPT reference
// instance.
//
// The script never rewrites React state or CSS: it moves the real pointer onto
// the project row, waits for the tooltip the app itself renders, then records
// geometry, computed styles, the rendered markup, and screenshots.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9352 \
//     node scripts/cdp_capture_project_hover.mjs --output=artifacts/project-hover
//
// Options:
//   --output=DIR      artifact directory (default artifacts/project-hover)
//   --theme=light     capture one theme; omitted captures light then dark
//   --project=NAME    project row to hover (default: the first row)
//   --scale=2         screenshot scale for the crops
import fs from 'node:fs';
import { connect, delay, option, resizeWindow, setTheme, waitFor } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = option('output', 'artifacts/project-hover');
const requestedTheme = option('theme');
const requestedProject = option('project');
const scale = Number(option('scale', '2'));
fs.mkdirSync(output, { recursive: true });

const STYLE_PROBES = [
  'display', 'position', 'width', 'height', 'minWidth', 'maxWidth', 'minHeight', 'maxHeight',
  'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft', 'margin', 'rowGap', 'columnGap',
  'fontFamily', 'fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'textAlign',
  'color', 'backgroundColor', 'backgroundImage', 'borderRadius', 'borderColor', 'borderWidth',
  'borderTopWidth', 'borderTopColor', 'boxShadow', 'opacity', 'overflow', 'flexDirection',
  'alignItems', 'justifyContent', 'flexShrink', 'flexGrow', 'whiteSpace', 'textOverflow',
  'backdropFilter', 'zIndex', 'transform', 'pointerEvents', 'gridTemplateColumns',
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
      cls: (element.className || '').toString(),
      dataSide: element.getAttribute('data-side'),
      inlineStyle: element.getAttribute('style'),
      text: [...element.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent).join('').trim(),
      rect: [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100),
      styles,
    };
    const svg = [...element.children].find((child) => child.tagName === 'svg');
    if (svg) node.svg = svg.outerHTML;
    if (element.children.length) node.children = [...element.children].map(DESCRIBE_SELF);
    return node;
  }
`;

// The recursive helper is injected as a named function so nested calls keep
// describing children instead of stopping at the first level.
const DESCRIBE_SELF = '__describe';
const describeFunction = `const ${DESCRIBE_SELF} = ${DESCRIBE.replace(/DESCRIBE_SELF/g, DESCRIBE_SELF)};`;

const PANEL_QUERY = `[...document.body.children].find((el) => /w-fit text-sm whitespace-normal/.test((el.className || '').toString()))`;

const ROW_QUERY = `[...document.querySelectorAll('[data-app-action-sidebar-project-row]')]`;

async function projectRows(cdp) {
  return JSON.parse(
    await cdp.evaluate(`(() => {
      return JSON.stringify(${ROW_QUERY}.map((row) => {
        const rect = row.getBoundingClientRect();
        return {
          label: (row.getAttribute('data-app-action-sidebar-project-label') || row.textContent || '').trim(),
          projectId: row.getAttribute('data-app-action-sidebar-project-id'),
          expanded: row.getAttribute('aria-expanded'),
          rect: [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100),
        };
      }));
    })()`),
  );
}

async function hoverRow(cdp, rect, { timeoutMs = 4000 } = {}) {
  const x = Math.round(rect[0] + rect[2] * 0.35);
  const y = Math.round(rect[1] + rect[3] / 2);
  // Leave the row first so the app always sees a fresh pointer enter.
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: Math.round(rect[0] + rect[2] * 0.35), y: Math.round(rect[1] + rect[3] * 4) });
  await delay(150);
  const startedAt = Date.now();
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  let appearedAfterMs = null;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const present = await cdp.evaluate(`!!${PANEL_QUERY}`);
    if (present) {
      appearedAfterMs = Date.now() - startedAt;
      break;
    }
    await delay(50);
  }
  if (appearedAfterMs === null) throw new Error('project hover card never appeared');
  // The card fades in; give the transition time to finish before measuring.
  await delay(600);
  return { pointer: [x, y], appearedAfterMs };
}

async function dump(cdp) {
  return JSON.parse(
    await cdp.evaluate(`(() => {
      ${describeFunction}
      const panel = ${PANEL_QUERY};
      if (!panel) return JSON.stringify({ found: false });
      const bodyIndex = [...document.body.children].indexOf(panel);
      return JSON.stringify({
        found: true,
        bodyIndex,
        viewport: [innerWidth, innerHeight, devicePixelRatio],
        theme: document.documentElement.getAttribute('data-theme'),
        root: ${DESCRIBE_SELF}(panel),
        innerText: panel.innerText,
        innerHTML: panel.outerHTML,
      });
    })()`),
  );
}

async function hoverInnerRow(cdp, label) {
  const target = await cdp.evaluate(`(() => {
    const node = [...document.querySelectorAll('[aria-label]')].find((el) => el.getAttribute('aria-label') === ${JSON.stringify(label)});
    if (!node) return JSON.stringify(null);
    const rect = node.getBoundingClientRect();
    return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]);
  })()`);
  if (!target || target === 'null') return null;
  const [x, y] = JSON.parse(target);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: Math.round(x), y: Math.round(y) });
  await delay(500);
  return [Math.round(x), Math.round(y)];
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

function union(a, b) {
  const x = Math.min(a[0], b[0]);
  const y = Math.min(a[1], b[1]);
  return [x, y, Math.max(a[0] + a[2], b[0] + b[2]) - x, Math.max(a[1] + a[3], b[1] + b[3]) - y];
}

const cdp = await connect(endpoint);
await waitFor(cdp, `document.querySelectorAll('[data-app-action-sidebar-project-row]').length > 0`, { timeoutMs: 60000 });
const size = await resizeWindow(cdp, 1440, 900).catch((error) => `resize skipped: ${error.message}`);
console.log('window', size);

const rows = await projectRows(cdp);
const row = requestedProject ? rows.find((candidate) => candidate.label === requestedProject) : rows[0];
if (!row) throw new Error(`no project row named ${requestedProject}`);

const report = {
  endpoint,
  capturedAt: new Date().toISOString(),
  window: size,
  project: { label: row.label, projectId: row.projectId },
  themes: {},
};

for (const theme of requestedTheme ? [requestedTheme] : ['light', 'dark']) {
  await setTheme(cdp, theme);
  await delay(400);
  const hover = await hoverRow(cdp, row.rect);
  const snapshot = await dump(cdp);
  if (!snapshot.found) throw new Error('hover card missing after hover');
  const panelRect = snapshot.root.rect;
  const geometry = {
    row: row.rect,
    panel: panelRect,
    offsetLeft: Math.round((panelRect[0] - (row.rect[0] + row.rect[2])) * 100) / 100,
    offsetTop: Math.round((panelRect[1] - row.rect[1]) * 100) / 100,
    side: snapshot.root.dataSide,
    inlineStyle: snapshot.root.inlineStyle,
    bodyIndex: snapshot.bodyIndex,
    appearedAfterMs: hover.appearedAfterMs,
    pointer: hover.pointer,
  };
  const full = await captureRegion(cdp, `${output}/${theme}-window.png`, [0, 0, 1440, 900], { padding: 0 });
  const crop = await captureRegion(cdp, `${output}/${theme}-hover-card.png`, union(row.rect, panelRect));
  const innerHover = await hoverInnerRow(cdp, `Open ${snapshot.innerText.split('\n').find((line) => line.startsWith('/'))}`);
  const innerSnapshot = innerHover ? await dump(cdp) : null;
  if (innerSnapshot) fs.writeFileSync(`${output}/${theme}-hover-card-path-row.json`, JSON.stringify(innerSnapshot, null, 1));

  report.themes[theme] = { geometry, full, crop, innerRowHover: innerHover ? { pointer: innerHover } : null, snapshot };
  fs.writeFileSync(`${output}/${theme}-hover-card.json`, JSON.stringify(snapshot, null, 1));
  console.log(theme, JSON.stringify(geometry));
  // Move the pointer away and let the tooltip unmount before the next theme.
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 900, y: 700 });
  await delay(500);
}

fs.writeFileSync(`${output}/report.json`, JSON.stringify(report, null, 1));
console.log('wrote', `${output}/report.json`);
cdp.socket.close();
