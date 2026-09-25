// Record how the reference's user-message navigation rail moves: hover
// timing, marker transitions, the card's anchor and close grace, the marker
// click's scroll and highlight, scrubbing, wheel routing, and the rail's own
// scroll-follow.
//
// Everything is driven with real CDP pointer input. An in-page
// requestAnimationFrame recorder only reads geometry and computed styles, so
// the timelines describe the shipped build's behaviour frame by frame.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9371 \
//     node scripts/cdp_probe_user_message_rail_motion.mjs \
//       --output=artifacts/user-message-rail-motion/reference
//
// Options:
//   --output=DIR    artifact directory
//   --theme=NAME    appearance to record in (default dark)
//   --thread=TITLE  sidebar task row to open (default 提交本次修改)
//   --window=WxH    emulated viewport (default 1800x1000)
//   --only=a,b      run only the named scenarios
import fs from 'node:fs';
import path from 'node:path';
import { connect, delay, option, setTheme, waitFor } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = option('output', 'artifacts/user-message-rail-motion/reference');
const theme = option('theme', 'dark');
const thread = option('thread', '提交本次修改');
const [windowWidth, windowHeight] = option('window', '1800x1000').split('x').map(Number);
const only = option('only')?.split(',') ?? null;
fs.mkdirSync(output, { recursive: true });

const ITEMS = `[...document.querySelectorAll('[data-thread-user-message-navigation-item-id]')]`;
const LIST = `document.querySelector('[data-thread-user-message-navigation-rail-list]')`;
const PANE = `document.querySelector('.thread-scroll-container')`;

/// Installs a per-frame recorder. `marks` carries the moments the driver
/// dispatched input, on the page's own clock.
const RECORDER = `
  (() => {
    const record = { t0: performance.now(), frames: [], marks: [], running: true };
    window.__railRecord = record;
    const round = (value) => Math.round(value * 100) / 100;
    const frame = () => {
      if (!record.running) return;
      const items = ${ITEMS};
      const list = ${LIST};
      const pane = ${PANE};
      const card = document.querySelector('[data-thread-user-message-navigation-tooltip-preview]');
      const tooltip = card ? card.closest('[role=tooltip]') : null;
      const open = tooltip != null && !tooltip.hidden;
      const cardRect = open ? card.getBoundingClientRect() : null;
      const flash = window.__railFlashTarget;
      record.frames.push({
        t: round(performance.now() - record.t0),
        widths: items.map((item) => round(item.querySelector('span > span > span').getBoundingClientRect().width)),
        colours: items.map((item) => {
          const marker = item.querySelector('span > span');
          const style = getComputedStyle(marker);
          return style.color + '@' + style.opacity;
        }),
        current: items.map((item) => (item.getAttribute('aria-current') === 'true' ? 1 : 0)).join(''),
        scrubTarget: items.findIndex((item) => item.hasAttribute('data-scrub-target')),
        scrubbing: list ? list.hasAttribute('data-scrubbing') : null,
        card: open ? { rect: [cardRect.x, cardRect.y, cardRect.width, cardRect.height].map(round), label: (card.firstElementChild?.textContent || '').trim().slice(0, 40) } : null,
        paneScrollTop: pane ? round(pane.scrollTop) : null,
        listScrollTop: list ? round(list.scrollTop) : null,
        listFade: list ? [getComputedStyle(list).getPropertyValue('--top-fade'), getComputedStyle(list).getPropertyValue('--bottom-fade')] : null,
        flash: flash ? getComputedStyle(flash).backgroundColor : null,
      });
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
    return true;
  })()
`;

async function startRecording(cdp) {
  await cdp.evaluate(RECORDER);
}

async function mark(cdp, name) {
  await cdp.evaluate(`window.__railRecord.marks.push({ name: ${JSON.stringify(name)}, t: Math.round((performance.now() - window.__railRecord.t0) * 100) / 100 })`);
}

async function stopRecording(cdp) {
  return JSON.parse(await cdp.evaluate(`(() => { const r = window.__railRecord; r.running = false; return JSON.stringify({ frames: r.frames, marks: r.marks }); })()`));
}

async function move(cdp, x, y, buttons = 0) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, buttons, button: buttons ? 'left' : 'none' });
}

async function press(cdp, x, y) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1 });
}

async function release(cdp, x, y) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1 });
}

async function itemCentres(cdp) {
  return JSON.parse(await cdp.evaluate(`JSON.stringify(${ITEMS}.map((item) => { const r = item.getBoundingClientRect(); return [Math.round(r.x + r.width / 2), Math.round(r.y + r.height / 2)]; }))`));
}

async function park(cdp) {
  await move(cdp, Math.round(windowWidth / 2), Math.round(windowHeight / 2));
  // Longer than the tooltip's skip-delay window, so the next hover starts cold.
  await delay(900);
}

async function openThread(cdp, title) {
  const point = JSON.parse(await cdp.evaluate(`(() => {
    const row = [...document.querySelectorAll('[data-app-action-sidebar-thread-row]')].find((candidate) => candidate.getAttribute('data-app-action-sidebar-thread-title') === ${JSON.stringify(title)});
    if (!row) return 'null';
    row.scrollIntoView({ block: 'center' });
    const rect = row.getBoundingClientRect();
    return JSON.stringify([rect.x + 60, rect.y + rect.height / 2]);
  })()`));
  if (!point) throw new Error('no sidebar task row titled ' + title);
  await move(cdp, Math.round(point[0]), Math.round(point[1]));
  await press(cdp, Math.round(point[0]), Math.round(point[1]));
  await release(cdp, Math.round(point[0]), Math.round(point[1]));
  await delay(2500);
}

async function closeSidePanel(cdp) {
  for (let attempt = 0; attempt < 4; attempt += 1) {
    const target = await cdp.evaluate(`(() => {
      const button = [...document.querySelectorAll('button')].find((candidate) => /^Close .* tab$/.test(candidate.getAttribute('aria-label') || ''));
      if (!button) return null;
      const rect = button.getBoundingClientRect();
      return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]);
    })()`);
    if (!target) return;
    const [x, y] = JSON.parse(target);
    await cdp.click(Math.round(x), Math.round(y));
    await delay(1200);
  }
}

async function waitForSettledPane(cdp) {
  let previous = null;
  for (let poll = 0; poll < 80; poll += 1) {
    const state = await cdp.evaluate(`String(Math.round(${PANE}.scrollTop)) + '|' + ${ITEMS}.map((item) => item.getAttribute('aria-current')).join(',')`);
    if (state === previous) return;
    previous = state;
    await delay(150);
  }
}

// Scenarios -----------------------------------------------------------------

/// Pointer enters one marker from outside the rail: when the taper starts,
/// how it moves, and when the card opens.
async function hoverEnter(cdp, centres) {
  await park(cdp);
  const [x, y] = centres[3];
  // Approach from the transcript side: the sidebar's own row hover card
  // would cover the rail if the pointer came from the left.
  await move(cdp, x + 60, y);
  await delay(200);
  await startRecording(cdp);
  await delay(80);
  await mark(cdp, 'enter-marker-4');
  await move(cdp, x, y);
  await delay(700);
  return stopRecording(cdp);
}

/// With the card open, step to a marker three rows lower: the card's anchor
/// and label, and the taper's retargeting.
async function hoverStep(cdp, centres) {
  await park(cdp);
  const [x, y] = centres[3];
  await move(cdp, x, y);
  await delay(600);
  await startRecording(cdp);
  await delay(80);
  await mark(cdp, 'step-to-marker-7');
  await move(cdp, centres[6][0], centres[6][1]);
  await delay(500);
  return stopRecording(cdp);
}

/// Sweep across several markers before the card has opened: does moving
/// restart the open delay?
async function hoverSweep(cdp, centres) {
  await park(cdp);
  await move(cdp, centres[1][0] + 60, centres[1][1]);
  await delay(200);
  await startRecording(cdp);
  await delay(80);
  await mark(cdp, 'enter-marker-2');
  for (let index = 1; index <= 9; index += 1) {
    await move(cdp, centres[index][0], centres[index][1]);
    await delay(40);
  }
  await mark(cdp, 'sweep-done-marker-10');
  await delay(600);
  return stopRecording(cdp);
}

/// Leave the rail to the left, away from the card: the close grace and the
/// taper's return. Then re-enter inside and outside the skip-delay window.
async function hoverLeaveAndReturn(cdp, centres) {
  await park(cdp);
  const [x, y] = centres[3];
  await move(cdp, x, y);
  await delay(600);
  await startRecording(cdp);
  await delay(80);
  await mark(cdp, 'leave-left');
  await move(cdp, x - 30, y);
  await delay(250);
  await mark(cdp, 'reenter-after-250ms');
  await move(cdp, x, y);
  await delay(500);
  await mark(cdp, 'leave-left-again');
  await move(cdp, x - 30, y);
  await delay(700);
  await mark(cdp, 'reenter-after-700ms');
  await move(cdp, x, y);
  await delay(600);
  return stopRecording(cdp);
}

/// Travel from the marker onto the card and back out of it.
async function hoverToCard(cdp, centres) {
  await park(cdp);
  const [x, y] = centres[3];
  await move(cdp, x, y);
  await delay(600);
  const card = JSON.parse(await cdp.evaluate(`(() => { const c = document.querySelector('[data-thread-user-message-navigation-tooltip-preview]'); if (!c) return 'null'; const r = c.getBoundingClientRect(); return JSON.stringify([r.x, r.y, r.width, r.height]); })()`));
  if (!card) return { error: 'card never opened' };
  await startRecording(cdp);
  await delay(80);
  await mark(cdp, 'travel-to-card');
  const steps = 6;
  const targetX = Math.round(card[0] + 40);
  const targetY = Math.round(card[1] + card[3] / 2);
  for (let step = 1; step <= steps; step += 1) {
    await move(cdp, Math.round(x + ((targetX - x) * step) / steps), Math.round(y + ((targetY - y) * step) / steps));
    await delay(16);
  }
  await mark(cdp, 'on-card');
  await delay(400);
  await mark(cdp, 'leave-card-right');
  await move(cdp, Math.round(card[0] + card[2] + 40), targetY);
  await delay(500);
  return { card, ...(await stopRecording(cdp)) };
}

/// Leave the marker and stop in the gap below the card's triangle: does the
/// grace timer close the card while the pointer rests outside both?
async function hoverStopInGap(cdp, centres) {
  await park(cdp);
  const [x, y] = centres[3];
  await move(cdp, x, y);
  await delay(600);
  await startRecording(cdp);
  await delay(80);
  await mark(cdp, 'leave-right-into-gap');
  // Just right of the rail, outside the card's vertical span, far below it.
  await move(cdp, x + 22, y + 200);
  await delay(600);
  return stopRecording(cdp);
}

/// Click a marker far from the current position: the smooth scroll curve,
/// where the message lands, and the bubble highlight.
async function clickJump(cdp, centres) {
  await park(cdp);
  // Start from the bottom of the task so the jump travels a long way.
  const last = centres.at(-1);
  await move(cdp, last[0], last[1]);
  await press(cdp, last[0], last[1]);
  await release(cdp, last[0], last[1]);
  await delay(1500);
  await park(cdp);
  await waitForSettledPane(cdp);
  const target = 1;
  await cdp.evaluate(`(() => {
    const id = ${ITEMS}[${target}].getAttribute('data-thread-user-message-navigation-item-id');
    const unit = document.querySelector('[data-content-search-unit-key="' + CSS.escape(id) + '"]');
    window.__railFlashTarget = unit ? (unit.querySelector('[data-user-message-bubble]') ?? unit.querySelector('[data-composer-attachment-pill]')) : null;
    return true;
  })()`);
  const before = JSON.parse(await cdp.evaluate(`JSON.stringify({ scrollTop: ${PANE}.scrollTop, scrollHeight: ${PANE}.scrollHeight, clientHeight: ${PANE}.clientHeight })`));
  await startRecording(cdp);
  await delay(80);
  await mark(cdp, 'click-marker-2');
  await move(cdp, centres[target][0], centres[target][1]);
  await press(cdp, centres[target][0], centres[target][1]);
  await release(cdp, centres[target][0], centres[target][1]);
  await delay(1800);
  const record = await stopRecording(cdp);
  const landing = JSON.parse(await cdp.evaluate(`(() => {
    const id = ${ITEMS}[${target}].getAttribute('data-thread-user-message-navigation-item-id');
    const unit = document.querySelector('[data-content-search-unit-key="' + CSS.escape(id) + '"]');
    const pane = ${PANE};
    const paneRect = pane.getBoundingClientRect();
    const unitRect = unit.getBoundingClientRect();
    const bubble = unit.querySelector('[data-user-message-bubble]');
    const style = getComputedStyle(unit);
    return JSON.stringify({
      paneTop: paneRect.top,
      unitTop: unitRect.top,
      bubbleTop: bubble ? bubble.getBoundingClientRect().top : null,
      scrollMarginTop: style.scrollMarginTop,
      paneScrollPaddingTop: getComputedStyle(pane).scrollPaddingTop,
      panePaddingTop: getComputedStyle(pane).paddingTop,
      scrollTop: pane.scrollTop,
      turn: unit.closest('[data-turn-key]') ? unit.closest('[data-turn-key]').getBoundingClientRect().top : null,
    });
  })()`));
  return { before, landing, ...record };
}

/// Click nearby markers whose messages are already mounted: the reference
/// scrolls those with the browser's smooth scrollIntoView, then flashes the
/// bubble.
async function clickNear(cdp, centres) {
  await park(cdp);
  const results = [];
  for (const [from, to] of [[1, 2], [2, 4], [4, 1]]) {
    await move(cdp, centres[from][0], centres[from][1]);
    await press(cdp, centres[from][0], centres[from][1]);
    await release(cdp, centres[from][0], centres[from][1]);
    await delay(1600);
    await park(cdp);
    await waitForSettledPane(cdp);
    const mounted = await cdp.evaluate(`(() => {
      const id = ${ITEMS}[${to}].getAttribute('data-thread-user-message-navigation-item-id');
      const unit = document.querySelector('[data-content-search-unit-key="' + CSS.escape(id) + '"]');
      window.__railFlashTarget = unit ? (unit.querySelector('[data-user-message-bubble]') ?? unit.querySelector('[data-composer-attachment-pill]')) : null;
      return window.__railFlashTarget != null;
    })()`);
    const restingFlash = await cdp.evaluate(`window.__railFlashTarget ? getComputedStyle(window.__railFlashTarget).backgroundColor : null`);
    await startRecording(cdp);
    await delay(80);
    await mark(cdp, `click-${from + 1}-to-${to + 1}`);
    await move(cdp, centres[to][0], centres[to][1]);
    await press(cdp, centres[to][0], centres[to][1]);
    await release(cdp, centres[to][0], centres[to][1]);
    await delay(1800);
    const record = await stopRecording(cdp);
    const landing = JSON.parse(await cdp.evaluate(`(() => {
      const id = ${ITEMS}[${to}].getAttribute('data-thread-user-message-navigation-item-id');
      const unit = document.querySelector('[data-content-search-unit-key="' + CSS.escape(id) + '"]');
      const pane = ${PANE};
      return JSON.stringify({ unitTop: unit.getBoundingClientRect().top - pane.getBoundingClientRect().top, scrollTop: pane.scrollTop });
    })()`));
    results.push({ from, to, mounted, restingFlash, landing, ...record });
  }
  return results;
}

/// Press on one marker and drag down the rail with the button held.
async function scrub(cdp, centres) {
  await park(cdp);
  await waitForSettledPane(cdp);
  await startRecording(cdp);
  await delay(80);
  const [x, y] = centres[1];
  await move(cdp, x, y);
  await mark(cdp, 'press-marker-2');
  await press(cdp, x, y);
  await delay(120);
  for (let step = 1; step <= 8; step += 1) {
    await mark(cdp, 'drag-to-' + (step + 2));
    await move(cdp, x, centres[1 + step][1], 1);
    await delay(90);
  }
  // Past the rail's end: the scrub clamps to the last row.
  await mark(cdp, 'drag-below-rail');
  await move(cdp, x + 80, centres.at(-1)[1] + 120, 1);
  await delay(200);
  await mark(cdp, 'release');
  await release(cdp, x + 80, centres.at(-1)[1] + 120);
  await delay(600);
  return stopRecording(cdp);
}

/// Hover every marker in turn with the card open: each card's rect.
async function cardSweep(cdp, centres) {
  await park(cdp);
  await move(cdp, centres[0][0], centres[0][1]);
  await delay(450);
  await startRecording(cdp);
  for (let index = 0; index < centres.length; index += 1) {
    await mark(cdp, `card-${index + 1}`);
    await move(cdp, centres[index][0], centres[index][1]);
    await delay(120);
  }
  return stopRecording(cdp);
}

/// Alt+↑/↓ from the first prompt: where each step lands.
async function altArrows(cdp, centres) {
  const tops = async () => JSON.parse(await cdp.evaluate(`(() => {
    const pane = ${PANE};
    const paneTop = pane.getBoundingClientRect().top;
    return JSON.stringify(${ITEMS}.map((item) => {
      const unit = document.querySelector('[data-content-search-unit-key="' + CSS.escape(item.getAttribute('data-thread-user-message-navigation-item-id')) + '"]');
      if (!unit) return null;
      const bubble = unit.querySelector('[data-user-message-bubble]') ?? unit;
      return Math.round(bubble.getBoundingClientRect().top - paneTop);
    }));
  })()`));
  const key = async (name) => {
    const code = name === 'ArrowUp' ? 38 : 40;
    await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: name, code: name, windowsVirtualKeyCode: code, modifiers: 1 });
    await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: name, code: name, windowsVirtualKeyCode: code, modifiers: 1 });
  };
  await park(cdp);
  await move(cdp, centres[0][0], centres[0][1]);
  await press(cdp, centres[0][0], centres[0][1]);
  await release(cdp, centres[0][0], centres[0][1]);
  await delay(1600);
  await park(cdp);
  const steps = [{ key: 'start', tops: await tops() }];
  for (const [name, label] of [['ArrowDown', 'alt-down'], ['ArrowDown', 'alt-down'], ['ArrowDown', 'alt-down'], ['ArrowUp', 'alt-up']]) {
    await key(name);
    await delay(900);
    steps.push({ key: label, tops: await tops() });
  }
  return steps;
}

/// Wheel over the rail while the list itself cannot scroll.
async function wheelOverRail(cdp, centres) {
  await park(cdp);
  // Start mid-transcript so the pane could scroll either way.
  await move(cdp, centres[4][0], centres[4][1]);
  await press(cdp, centres[4][0], centres[4][1]);
  await release(cdp, centres[4][0], centres[4][1]);
  await delay(1600);
  await park(cdp);
  await waitForSettledPane(cdp);
  const [x, y] = centres[5];
  const before = Number(await cdp.evaluate(`${PANE}.scrollTop`));
  const listBefore = Number(await cdp.evaluate(`${LIST}.scrollTop`));
  await move(cdp, x, y);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseWheel', x, y, deltaX: 0, deltaY: 240 });
  await delay(700);
  const after = Number(await cdp.evaluate(`${PANE}.scrollTop`));
  const listAfter = Number(await cdp.evaluate(`${LIST}.scrollTop`));
  const list = JSON.parse(await cdp.evaluate(`JSON.stringify({ scrollHeight: ${LIST}.scrollHeight, clientHeight: ${LIST}.clientHeight, overscroll: getComputedStyle(${LIST}).overscrollBehaviorY, navParentIsPane: document.querySelector('nav[aria-label="User messages"]').parentElement === ${PANE} || ${PANE}.contains(document.querySelector('nav[aria-label="User messages"]')) })`));
  return { paneBefore: before, paneAfter: after, listBefore, listAfter, list };
}

/// Wheel the transcript itself: how aria-current moves.
async function transcriptScroll(cdp, centres) {
  await park(cdp);
  const [x, y] = centres[0];
  await move(cdp, x, y);
  await press(cdp, x, y);
  await release(cdp, x, y);
  await delay(1500);
  await park(cdp);
  await startRecording(cdp);
  await delay(80);
  for (let step = 0; step < 12; step += 1) {
    await mark(cdp, 'wheel-' + step);
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: Math.round(windowWidth / 2), y: Math.round(windowHeight / 2), deltaX: 0, deltaY: 400 });
    await delay(250);
  }
  const record = await stopRecording(cdp);
  const units = JSON.parse(await cdp.evaluate(`(() => {
    const pane = ${PANE};
    const paneRect = pane.getBoundingClientRect();
    return JSON.stringify(${ITEMS}.map((item) => {
      const id = item.getAttribute('data-thread-user-message-navigation-item-id');
      const unit = document.querySelector('[data-content-search-unit-key="' + CSS.escape(id) + '"]');
      const turn = unit ? unit.closest('[data-turn-key]') ?? unit : null;
      const r = turn ? turn.getBoundingClientRect() : null;
      return { current: item.getAttribute('aria-current') === 'true', turn: r ? [Math.round(r.top - paneRect.top), Math.round(r.bottom - paneRect.top)] : null };
    }));
  })()`));
  return { units, ...record };
}

const SCENARIOS = {
  hoverEnter,
  hoverStep,
  hoverSweep,
  hoverLeaveAndReturn,
  hoverToCard,
  hoverStopInGap,
  clickJump,
  cardSweep,
  clickNear,
  scrub,
  altArrows,
  wheelOverRail,
  transcriptScroll,
};

const cdp = await connect(endpoint);
// The reference follows the system appearance until a theme is chosen; only
// switch when what is on screen differs from the requested appearance.
const onScreen = await cdp.evaluate(`document.documentElement.getAttribute('data-theme') ?? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')`);
if (onScreen !== theme && !(await setTheme(cdp, theme))) throw new Error(`reference app did not switch to ${theme}`);
await cdp.send('Emulation.setDeviceMetricsOverride', { width: windowWidth, height: windowHeight, deviceScaleFactor: 1, mobile: false });
await cdp.send('Emulation.setFocusEmulationEnabled', { enabled: true });
await delay(600);
await closeSidePanel(cdp);
await openThread(cdp, thread);
await waitFor(cdp, `${ITEMS}.length >= 4`, { timeoutMs: 30000 });
await delay(900);
const centres = await itemCentres(cdp);
const report = {
  capturedAt: new Date().toISOString(),
  endpoint,
  theme,
  thread,
  viewport: [windowWidth, windowHeight],
  centres,
  scenarios: {},
};
for (const [name, run] of Object.entries(SCENARIOS)) {
  if (only && !only.includes(name)) continue;
  report.scenarios[name] = await run(cdp, centres);
  console.log('recorded', name);
}
await park(cdp);
fs.writeFileSync(path.join(output, 'motion.json'), `${JSON.stringify(report)}\n`);
cdp.socket.close();
