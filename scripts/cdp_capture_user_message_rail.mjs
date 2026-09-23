// Capture the conversation's user-message navigation rail from a dedicated
// ChatGPT reference instance.
//
// The script drives the real app: it opens a task from the sidebar, switches
// the app's own appearance setting, hovers a rail marker with a real pointer
// move, and clicks one to see what jumping does. Nothing is rewritten in the
// page, so the recording describes the shipped build's behaviour.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9355 \
//     node scripts/cdp_capture_user_message_rail.mjs \
//       --output=artifacts/user-message-rail/reference
//
// Options:
//   --output=DIR    artifact directory (default artifacts/user-message-rail/reference)
//   --theme=NAME    capture one theme; omitted captures light then dark
//   --thread=TITLE  sidebar task row to open (default: the current task)
//   --window=WxH    emulated viewport (default 1800x1000)
//   --dpr=N         device scale factor honoured by the screenshots (default 1)
//   --scale=N       screenshot scale for the crops (default 1)
//   --hover=INDEX   marker to hover, 1-based (default: 4)
import fs from 'node:fs';
import path from 'node:path';
import { connect, delay, option, setTheme, waitFor } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = option('output', 'artifacts/user-message-rail/reference');
const requestedTheme = option('theme');
const requestedThread = option('thread');
const [windowWidth, windowHeight] = option('window', '1800x1000').split('x').map(Number);
const dpr = Number(option('dpr', '1'));
const scale = Number(option('scale', '1'));
const hoverIndex = Number(option('hover', '4'));
fs.mkdirSync(output, { recursive: true });

const STYLE_PROBES = [
  'display', 'position', 'top', 'left', 'right', 'bottom', 'transform', 'transformOrigin',
  'width', 'height', 'minWidth', 'minHeight', 'maxWidth', 'maxHeight',
  'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft', 'marginTop', 'marginRight',
  'marginBottom', 'marginLeft', 'rowGap', 'columnGap',
  'fontFamily', 'fontSize', 'fontWeight', 'fontStyle', 'lineHeight', 'letterSpacing',
  'color', 'backgroundColor', 'backgroundImage', 'borderRadius', 'borderWidth', 'borderColor',
  'boxShadow', 'opacity', 'overflowX', 'overflowY', 'flexDirection', 'alignItems',
  'justifyContent', 'flexShrink', 'flexGrow', 'whiteSpace', 'textOverflow', 'textOverflowMode',
  'backdropFilter', 'zIndex', 'pointerEvents', 'cursor', 'outline', 'transition',
  'transitionDuration', 'transitionProperty', 'textAlign', 'wordBreak',
];

const TOKENS = [
  '--color-codex-description', '--color-text', '--color-surface-elevated-secondary',
  '--color-border', '--color-text-tertiary', '--text-sm', '--spacing', '--markdown-font-size',
  '--markdown-line-height', '--shadow-xl-spread',
];

const NAV = `document.querySelector('nav[aria-label="User messages"]')`;
const ITEMS = `[...document.querySelectorAll('[data-thread-user-message-navigation-item-id]')]`;
const CARD = `document.querySelector('[data-thread-user-message-navigation-tooltip-preview]')`;
const TEXT_UNIT = `document.querySelector('[data-content-search-unit-key]')`;

const DESCRIBE = `
  (element) => {
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    const styles = {};
    for (const name of ${JSON.stringify(STYLE_PROBES)}) styles[name] = style[name];
    const node = {
      tag: element.tagName.toLowerCase(),
      role: element.getAttribute('role'),
      label: element.getAttribute('aria-label'),
      cls: (element.className || '').toString(),
      dataState: element.getAttribute('data-state'),
      ariaCurrent: element.getAttribute('aria-current'),
      inlineStyle: element.getAttribute('style'),
      text: (element.textContent || '').trim().slice(0, 200),
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

const evaluateJson = async (cdp, expression) =>
  JSON.parse(await cdp.evaluate(`(() => { ${describeFunction} return JSON.stringify(${expression}); })()`));

async function movePointer(cdp, x, y) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
}

async function clickAt(cdp, x, y) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await delay(40);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1 });
}

async function itemCentre(cdp, index) {
  return evaluateJson(
    cdp,
    `(() => {
      const item = ${ITEMS}[${index}];
      if (!item) return null;
      const rect = item.getBoundingClientRect();
      return [rect.x + rect.width / 2, rect.y + rect.height / 2];
    })()`,
  );
}

/// Marker widths as rendered: the marker span is 26px wide and the line is
/// scaled inside it, so the drawn dash is `lineWidth` px.
async function markerWidths(cdp) {
  return evaluateJson(
    cdp,
    `${ITEMS}.map((item) => {
      const marker = item.querySelector('span > span');
      const line = item.querySelector('span > span > span');
      const lineRect = line.getBoundingClientRect();
      return {
        label: item.getAttribute('aria-label'),
        current: item.getAttribute('aria-current') === 'true',
        bookmarked: item.querySelector('[aria-hidden="true"].rounded-full') != null,
        progress: getComputedStyle(marker).getPropertyValue('--marker-progress').trim(),
        transform: getComputedStyle(line).transform,
        dashWidth: Math.round(lineRect.width * 100) / 100,
        markerColour: getComputedStyle(marker).color,
        markerOpacity: getComputedStyle(marker).opacity,
      };
    })`,
  );
}

async function paneGeometry(cdp) {
  return evaluateJson(
    cdp,
    `(() => {
      const pane = document.querySelector('.thread-scroll-container');
      const paneRect = pane ? pane.getBoundingClientRect() : null;
      const columns = [...document.querySelectorAll('.thread-scroll-container *')].filter(
        (element) => getComputedStyle(element).maxWidth === '768px' && element.getBoundingClientRect().width > 600,
      );
      const column = columns[0] ? columns[0].getBoundingClientRect() : null;
      return {
        pane: paneRect ? [paneRect.x, paneRect.y, paneRect.width, paneRect.height].map((v) => Math.round(v * 100) / 100) : null,
        contentColumn: column ? [column.x, column.y, column.width, column.height].map((v) => Math.round(v * 100) / 100) : null,
        documentBackground: getComputedStyle(document.body).backgroundColor,
      };
    })()`,
  );
}

async function tokens(cdp) {
  return evaluateJson(
    cdp,
    `(() => {
      const root = getComputedStyle(document.documentElement);
      const theme = document.querySelector('[data-theme]');
      const host = theme ? getComputedStyle(theme) : root;
      const out = {};
      for (const name of ${JSON.stringify(TOKENS)}) out[name] = (host.getPropertyValue(name) || root.getPropertyValue(name) || '').trim();
      return out;
    })()`,
  );
}

async function openThread(cdp, title) {
  const point = await evaluateJson(
    cdp,
    `(() => {
      const rows = [...document.querySelectorAll('[data-app-action-sidebar-thread-row]')];
      const row = ${title === null ? 'rows.find((candidate) => candidate.getAttribute("data-app-action-sidebar-thread-selected") === "true") ?? rows[0]' : `rows.find((candidate) => candidate.getAttribute("data-app-action-sidebar-thread-title") === ${JSON.stringify(title)})`};
      if (!row) return null;
      const rect = row.getBoundingClientRect();
      return { point: [rect.x + 60, rect.y + rect.height / 2], title: row.getAttribute('data-app-action-sidebar-thread-title'), id: row.getAttribute('data-app-action-sidebar-thread-id') };
    })()`,
  );
  if (!point) throw new Error('no sidebar task row matched ' + JSON.stringify(title));
  await clickAt(cdp, Math.round(point.point[0]), Math.round(point.point[1]));
  await delay(2500);
  return point;
}

async function closeSidePanel(cdp) {
  // The side panel shares its column with the transcript. Closing the open tab
  // collapses it, which is the layout the native capture runs in.
  for (let attempt = 0; attempt < 4; attempt += 1) {
    const closeTab = await evaluateJson(
      cdp,
      `(() => {
        const button = [...document.querySelectorAll('button')].find((candidate) => /^Close .* tab$/.test(candidate.getAttribute('aria-label') || ''));
        if (!button) return null;
        const rect = button.getBoundingClientRect();
        return [rect.x + rect.width / 2, rect.y + rect.height / 2];
      })()`,
    );
    if (!closeTab) return true;
    await clickAt(cdp, Math.round(closeTab[0]), Math.round(closeTab[1]));
    await delay(1200);
  }
  throw new Error('the side panel never closed');
}

/// The transcript both restores its own position and keeps loading history for
/// a while after a task opens. Wait until two consecutive probes agree before
/// recording anything.
async function waitForStableTranscript(cdp, { timeoutMs = 20000, quietMs = 700 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let previous = null;
  let stableSince = null;
  while (Date.now() < deadline) {
    const state = await cdp.evaluate(
      `(() => {
        const pane = document.querySelector('.thread-scroll-container');
        const unit = document.querySelector('[data-content-search-unit-key]');
        if (!pane || !unit) return JSON.stringify(null);
        const paneRect = pane.getBoundingClientRect();
        const unitRect = unit.getBoundingClientRect();
        const current = [...document.querySelectorAll('[data-thread-user-message-navigation-item-id]')]
          .filter((item) => item.getAttribute('aria-current') === 'true')
          .map((item) => item.getAttribute('aria-label'))
          .join(',');
        return JSON.stringify([Math.round(unitRect.y * 10) / 10, Math.round(paneRect.height), current, document.body.textContent.length]);
      })()`,
    );
    if (state === previous) {
      stableSince ??= Date.now();
      if (Date.now() - stableSince >= quietMs) return JSON.parse(state);
    } else {
      stableSince = null;
      previous = state;
    }
    await delay(200);
  }
  throw new Error('transcript never settled');
}

async function waitForRail(cdp) {
  await waitFor(cdp, `document.querySelectorAll('[data-thread-user-message-navigation-item-id]').length >= 4`, { timeoutMs: 30000 });
  // The rail is mounted from an idle callback, then fades in.
  await delay(900);
  return evaluateJson(
    cdp,
    `(() => {
      const nav = ${NAV};
      const rect = nav.getBoundingClientRect();
      return {
        labels: ${ITEMS}.map((item) => item.getAttribute('aria-label')),
        rect: [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100),
        navOpacity: getComputedStyle(nav).opacity,
        listState: ${NAV}.firstElementChild.getAttribute('data-state'),
      };
    })()`,
  );
}

/// Hovering a marker opens the preview after the tooltip's own delay. Poll for
/// the card so the report carries the delay the build actually uses. The rail
/// is remounted whenever the thread re-renders, which drops the hover, so the
/// approach is retried with a fresh pointer move.
async function hoverAndTimeTooltip(cdp, index) {
  let delayMs = null;
  let point = null;
  for (let attempt = 0; attempt < 3 && delayMs === null; attempt += 1) {
    point = await itemCentre(cdp, index);
    if (!point) throw new Error('no rail marker at index ' + index);
    await movePointer(cdp, Math.round(point[0]), Math.round(point[1] - 40));
    await delay(120);
    const started = Date.now();
    await movePointer(cdp, Math.round(point[0]), Math.round(point[1]));
    for (let poll = 0; poll < 250; poll += 1) {
      if (await cdp.evaluate(`!!${CARD}`)) {
        delayMs = Date.now() - started;
        break;
      }
      await delay(20);
    }
  }
  await delay(700);
  return { point: point ? [Math.round(point[0]), Math.round(point[1])] : null, delayMs };
}

async function captureTheme(cdp, theme) {
  if (!(await setTheme(cdp, theme))) throw new Error(`reference app did not switch to ${theme}`);
  await delay(700);
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width: windowWidth,
    height: windowHeight,
    deviceScaleFactor: dpr,
    mobile: false,
  });
  await delay(600);
  await closeSidePanel(cdp);
  const opened = await openThread(cdp, requestedThread);
  const rail = await waitForRail(cdp);
  const pane = await paneGeometry(cdp);
  // The rail is centred in the transcript pane, so the capture is only
  // comparable while that pane spans the whole window.
  if (!pane.pane || Math.round(pane.pane[1]) !== 0) {
    throw new Error(
      `transcript pane starts at ${pane.pane ? pane.pane[1] : null}; the side panel is open`,
    );
  }
  const tokenValues = await tokens(cdp);

  // Deterministic transcript state: the first user message sits at the top of
  // the pane, which is what clicking the first marker does.
  const first = await itemCentre(cdp, 0);
  await clickAt(cdp, Math.round(first[0]), Math.round(first[1]));
  await waitForStableTranscript(cdp);

  // Park the pointer away from the rail for the resting state.
  await movePointer(cdp, Math.round(windowWidth / 2), Math.round(windowHeight / 2));
  await delay(500);
  const rest = await markerWidths(cdp);
  const geometry = {
    pane: pane.pane,
    contentColumn: pane.contentColumn,
    rail: await evaluateJson(cdp, `(() => { const rect = ${NAV}.getBoundingClientRect(); return [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100); })()`),
    list: await evaluateJson(cdp, `(() => { const rect = ${NAV}.firstElementChild.getBoundingClientRect(); return [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100); })()`),
    items: await evaluateJson(cdp, `${ITEMS}.map((item) => { const rect = item.getBoundingClientRect(); return [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100); })`),
  };
  const styles = {
    rail: await evaluateJson(cdp, `${DESCRIBE_SELF}(${NAV})`),
  };
  // The rail element's own subtree is deep; keep the item/marker styles flat.
  const parts = await evaluateJson(
    cdp,
    `(() => {
      const item = ${ITEMS}[0];
      const track = item.querySelector('span');
      const marker = item.querySelector('span > span');
      const line = item.querySelector('span > span > span');
      return { item: ${DESCRIBE_SELF}(item), track: ${DESCRIBE_SELF}(track), marker: ${DESCRIBE_SELF}(marker), line: ${DESCRIBE_SELF}(line) };
    })()`,
  );

  const windowShot = path.join(output, `${theme}-window.png`);
  await cdp.screenshot(windowShot);
  const paneShot = path.join(output, `${theme}-pane.png`);
  if (pane.pane) {
    await cdp.screenshot(paneShot, { clip: { x: pane.pane[0], y: pane.pane[1], width: pane.pane[2], height: pane.pane[3] } });
  }
  const railStrip = path.join(output, `${theme}-rail-strip.png`);
  if (pane.pane) {
    await cdp.screenshot(railStrip, {
      clip: { x: pane.pane[0], y: pane.pane[1], width: 120, height: pane.pane[3] },
    });
  }

  const hover = await hoverAndTimeTooltip(cdp, hoverIndex - 1);
  const hovered = await markerWidths(cdp);
  // A full-window frame of the hovered state, so both builds can be compared
  // over the same region without re-deriving the card's position.
  const hoverWindow = path.join(output, `${theme}-hover-window.png`);
  await cdp.screenshot(hoverWindow);
  const card = await evaluateJson(
    cdp,
    `(() => {
      const card = ${CARD};
      if (!card) return null;
      const tooltip = card.closest('[role=tooltip]');
      const header = card.firstElementChild;
      const label = header ? header.querySelector('span') : null;
      const bookmark = header ? header.querySelector('button') : null;
      const preview = card.children[1] ?? null;
      const paragraph = preview ? preview.querySelector('p') : null;
      const tooltipRect = tooltip ? tooltip.getBoundingClientRect() : null;
      const cardRect = card.getBoundingClientRect();
      return {
        rect: [cardRect.x, cardRect.y, cardRect.width, cardRect.height].map((v) => Math.round(v * 100) / 100),
        tooltipRect: tooltipRect ? [tooltipRect.x, tooltipRect.y, tooltipRect.width, tooltipRect.height].map((v) => Math.round(v * 100) / 100) : null,
        text: card.textContent.trim(),
        html: card.outerHTML,
        card: ${DESCRIBE_SELF}(card),
        header: ${DESCRIBE_SELF}(header),
        label: ${DESCRIBE_SELF}(label),
        bookmark: ${DESCRIBE_SELF}(bookmark),
        preview: ${DESCRIBE_SELF}(preview),
        paragraph: ${DESCRIBE_SELF}(paragraph),
      };
    })()`,
  );
  const cardShot = path.join(output, `${theme}-hover.png`);
  if (card) {
    const pad = 24;
    await cdp.screenshot(cardShot, {
      clip: {
        x: Math.max(0, card.rect[0] - pad),
        y: Math.max(0, card.rect[1] - pad),
        width: Math.min(windowWidth - Math.max(0, card.rect[0] - pad), card.rect[2] + pad * 2),
        height: Math.min(windowHeight - Math.max(0, card.rect[1] - pad), card.rect[3] + pad * 2),
      },
    });
  }

  // Clicking a marker jumps the transcript so the message sits at the top.
  const jumpBefore = await evaluateJson(
    cdp,
    `(() => {
      const units = ${ITEMS}.length;
      const current = ${ITEMS}.filter((item) => item.getAttribute('aria-current') === 'true').map((item) => item.getAttribute('aria-label'));
      const top = ${TEXT_UNIT};
      const rect = top ? top.getBoundingClientRect() : null;
      return { items: units, current, firstUnitTop: rect ? Math.round(rect.y * 100) / 100 : null };
    })()`,
  );
  await clickAt(cdp, Math.round(first[0]), Math.round(first[1]));
  await waitForStableTranscript(cdp);
  const jumpAfter = await evaluateJson(
    cdp,
    `(() => {
      const current = ${ITEMS}.filter((item) => item.getAttribute('aria-current') === 'true').map((item) => item.getAttribute('aria-label'));
      const unit = document.querySelector('[data-content-search-unit-key]');
      const rect = unit ? unit.getBoundingClientRect() : null;
      const paneRect = document.querySelector('.thread-scroll-container').getBoundingClientRect();
      return { current, firstUnitTop: rect ? Math.round(rect.y * 100) / 100 : null, paneTop: Math.round(paneRect.y * 100) / 100 };
    })()`,
  );
  await cdp.screenshot(path.join(output, `${theme}-jump.png`));

  const report = {
    thread: { title: opened.title, id: opened.id, requested: requestedThread },
    viewport: [windowWidth, windowHeight, dpr],
    tokens: tokenValues,
    geometry,
    styles,
    parts,
    rail,
    rest,
    hover: { ...hover, markers: hovered },
    card,
    jump: { before: jumpBefore, after: jumpAfter },
    screenshots: {
      window: path.basename(windowShot),
      pane: pane.pane ? path.basename(paneShot) : null,
      railStrip: pane.pane ? path.basename(railStrip) : null,
      hoverWindow: path.basename(hoverWindow),
      hover: card ? path.basename(cardShot) : null,
      jump: `${theme}-jump.png`,
    },
  };
  console.log(
    theme,
    'rail',
    JSON.stringify(geometry.rail),
    'items',
    rest.length,
    'hoverDelayMs',
    hover.delayMs,
    'card',
    card ? JSON.stringify(card.rect) : null,
  );
  return report;
}

const cdp = await connect(endpoint);
const report = { capturedAt: new Date().toISOString(), endpoint, hoverIndex, themes: {} };
for (const theme of requestedTheme ? [requestedTheme] : ['light', 'dark']) {
  report.themes[theme] = await captureTheme(cdp, theme);
}
fs.writeFileSync(path.join(output, 'report.json'), `${JSON.stringify(report, null, 1)}\n`);
cdp.socket.close();
