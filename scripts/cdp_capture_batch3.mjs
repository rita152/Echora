// Capture the batch-three surfaces from the dedicated ChatGPT reference: the
// `/review` Code review slash row and its submenu (uncommitted changes and
// the base-branch list), Git → Review delivery, the Configuration page's web
// search row that provider capabilities gate in Echora, the Codex memory card
// and `/memories` dialog that the memory status joins, and the Pinned section
// header that custom sections reuse. Dark and light themes, each state with
// its DOM geometry and computed styles.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9471 \
//     node scripts/cdp_capture_batch3.mjs --output=artifacts/batch3 --date=20260929 [--only=review,git]
//
// Run it only against an instance launched with its own
// CHATGPT_REFERENCE_CODEX_HOME clone: switching the theme and the review
// delivery writes that clone. Nothing here sends a model request: the review
// submenu is opened and closed, never selected, and chats are only opened.
//
// The page is emulated at 1470x923 @2x on the same CDP session that takes
// every screenshot, so each capture has Echora's window size and DPR.
import fs from 'node:fs';
import path from 'node:path';
import { Cdp, delay, option } from './stage4/cdp_client.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const root = option('output', 'artifacts/batch3');
const date = option('date', '20260929');
const only = option('only') ? option('only').split(',') : null;
const themes = option('theme') ? [option('theme')] : ['dark', 'light'];
// A chat of the repository's own project, so the review submenu lists its branches.
const reviewThread = option('review-thread', '01a0e75f-8eb6-7620-81f6-bf76b03b8cdb');
const memoryThread = option('memory-thread', '01a0e6df-5365-7252-be73-e37c99d68445');

const cdp = await Cdp.connect(endpoint);
await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1470, height: 923, deviceScaleFactor: 2, mobile: false });
const evaluate = async expression => cdp.evaluate(expression);
const json = async expression => JSON.parse(await cdp.evaluate(`JSON.stringify(${expression})`));

const directory = topic => {
  const dir = path.join(`${root}-${topic}-${date}`, 'reference');
  fs.mkdirSync(dir, { recursive: true });
  return dir;
};

// Geometry and computed style of every element with text or a box inside
// `scope` (the open popup, the dialog, or the main pane).
const DOM = scope => `(() => {
  const round = value => Math.round(value * 10) / 10;
  const pick = element => {
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return {
      tag: element.tagName, role: element.getAttribute('role'), label: element.getAttribute('aria-label'),
      text: element.childElementCount === 0 ? element.textContent.trim().slice(0, 80) : null,
      rect: [rect.x, rect.y, rect.width, rect.height].map(round),
      font: style.fontSize + '/' + style.lineHeight + ' ' + style.fontWeight, color: style.color,
      background: style.backgroundColor, border: style.borderTopWidth + ' ' + style.borderTopColor,
      radius: style.borderTopLeftRadius, padding: style.padding, opacity: style.opacity,
    };
  };
  const scope = ${scope};
  const nodes = [...scope.querySelectorAll('*')].filter(element => {
    const rect = element.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return false;
    const style = getComputedStyle(element);
    return (element.childElementCount === 0 && element.textContent.trim()) || element.tagName === 'svg'
      || style.backgroundColor !== 'rgba(0, 0, 0, 0)' || style.borderTopWidth !== '0px';
  }).slice(0, 500);
  const box = scope.getBoundingClientRect();
  return { viewport: [innerWidth, innerHeight, devicePixelRatio], theme: document.documentElement.dataset.theme,
    scope: [box.x, box.y, box.width, box.height].map(round), nodes: nodes.map(pick) };
})()`;
const MAIN = "document.querySelector('main') || document.body";
// The composer's slash popup: the listbox above the composer.
const POPUP = "(document.querySelector('[role=listbox]') || document.querySelector('[cmdk-list]') || document.body).closest('[data-radix-popper-content-wrapper],[role=dialog],div') || document.body";
const SIDEBAR = "document.querySelector('nav') || document.body";
const DIALOG = "[...document.querySelectorAll('[role=dialog]')].pop() || document.body";

async function capture(topic, name, theme, scope = MAIN) {
  await delay(500);
  const dir = directory(topic);
  const file = path.join(dir, `${name}-${theme}.png`);
  await cdp.screenshot(file);
  fs.writeFileSync(`${file}.dom.json`, `${JSON.stringify(await json(DOM(scope)), null, 1)}\n`);
  console.log(file);
}

async function navigate(route) {
  await evaluate(`(() => {
    if (!window.__router) {
      const container = document.getElementById('root');
      const key = Object.keys(container).find(name => name.startsWith('__reactContainer'));
      const stack = [container[key]];
      while (stack.length) {
        const fiber = stack.pop();
        if (!fiber) continue;
        const props = fiber.memoizedProps;
        if (props && props.router && typeof props.router.navigate === 'function') { window.__router = props.router; break; }
        if (fiber.child) stack.push(fiber.child);
        if (fiber.sibling) stack.push(fiber.sibling);
      }
    }
    window.__router.navigate(${JSON.stringify(route)});
  })()`);
  await delay(1800);
}

/// Centre of the first visible element whose own text (or aria-label) is `text`.
async function locate(text, within = 'document') {
  return json(`(() => {
    const scope = ${within === 'dialog' ? "[...document.querySelectorAll('[role=dialog]')].pop()" : within};
    const match = [...scope.querySelectorAll('*')].find(element => {
      const rect = element.getBoundingClientRect();
      if (!rect.width || !rect.height) return false;
      if (element.getAttribute('aria-label') === ${JSON.stringify(text)}) return true;
      // The deepest element with exactly this text: labels are often split
      // into highlight spans.
      return element.textContent.trim() === ${JSON.stringify(text)}
        && ![...element.children].some(child => child.textContent.trim() === ${JSON.stringify(text)});
    });
    if (!match) return null;
    const rect = (match.closest('button,[role=button],[role=switch],[role=option],[role=menuitem],a') || match).getBoundingClientRect();
    return [rect.x + rect.width / 2, rect.y + rect.height / 2];
  })()`);
}

async function click(text, within) {
  const point = await locate(text, within);
  if (!point) throw Error(`nothing labelled "${text}" to click`);
  await cdp.click(point[0], point[1]);
  await delay(700);
}

async function hover(text, within) {
  const point = await locate(text, within);
  if (!point) throw Error(`nothing labelled "${text}" to hover`);
  await cdp.hover(point[0], point[1]);
  await delay(500);
}

async function waitFor(text) {
  for (let attempt = 0; attempt < 30; attempt++) {
    if (await locate(text)) return;
    await delay(200);
  }
  throw Error(`"${text}" never appeared`);
}

async function setTheme(theme) {
  if ((await evaluate('document.documentElement.dataset.theme')) === theme) return;
  await navigate('/settings/appearance');
  // Settings → Appearance → Mode: three unlabeled tiles, System, Light, Dark.
  const point = await json(`(() => {
    const mode = [...document.querySelectorAll('*')].find(node => node.childElementCount === 0 && node.textContent.trim() === 'Mode');
    let row = mode;
    while (row && row.getBoundingClientRect().width < 600) row = row.parentElement;
    const tiles = [...row.querySelectorAll('label')];
    const rect = tiles[${theme === 'dark' ? 2 : 1}].getBoundingClientRect();
    return [rect.x + rect.width / 2, rect.y + rect.height / 2];
  })()`);
  await cdp.click(point[0], point[1]);
  await delay(1200);
  const applied = await evaluate('document.documentElement.dataset.theme');
  if (applied !== theme) throw Error(`the reference did not switch to ${theme} (${applied})`);
}

async function focusComposer() {
  await evaluate(`(() => { const editor = document.querySelector('.ProseMirror,[contenteditable=true]'); editor.focus(); document.execCommand('selectAll'); document.execCommand('delete'); })()`);
  await delay(200);
}

async function type(text) {
  await cdp.send('Input.insertText', { text });
  await delay(800);
}

const wanted = topic => !only || only.includes(topic);

for (const theme of themes) {
  await setTheme(theme);
  if (wanted('review')) {
    await navigate(`/local/${reviewThread}`);
    await focusComposer();
    await type('/review');
    await capture('review', 'slash', theme, POPUP);
    await cdp.key('Enter');
    await waitFor('Review uncommitted changes');
    await delay(800);
    await capture('review', 'submenu', theme, POPUP);
    // Keyboard moves the highlight to the first base branch.
    await cdp.key('ArrowDown');
    await capture('review', 'submenu-branch', theme, POPUP);
    // The pointer highlights a row too.
    await hover('Review uncommitted changes');
    await capture('review', 'submenu-hover', theme, POPUP);
    // Escape leaves the submenu without starting a review.
    await cdp.key('Escape');
    await delay(400);
    await capture('review', 'submenu-escaped', theme, POPUP);
    await cdp.key('Escape');
    await focusComposer();
  }
  if (wanted('git')) {
    await navigate('/settings/git-settings');
    await waitFor('Review delivery');
    await capture('review', 'git-inline', theme);
    await click('Detached');
    await capture('review', 'git-detached', theme);
    // Back to inline, so every theme starts from the same state.
    await click('Inline');
  }
  if (wanted('capabilities')) {
    await navigate('/settings/general-settings');
    await click('Configuration');
    await waitFor('Web search');
    await capture('capabilities', 'web-search', theme);
  }
  if (wanted('memory')) {
    await navigate('/settings/personalization');
    await capture('memory', 'settings', theme);
    await navigate(`/local/${memoryThread}`);
    await focusComposer();
    await type('/mem');
    await cdp.key('Enter');
    await capture('memory', 'dialog-started', theme, DIALOG);
    await cdp.key('Escape');
  }
  if (wanted('sections')) {
    await navigate(`/local/${memoryThread}`);
    await hover('Pinned');
    await capture('sections', 'pinned-header-hover', theme, SIDEBAR);
  }
}
await cdp.send('Emulation.clearDeviceMetricsOverride');
cdp.close();
