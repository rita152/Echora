// Capture the batch-four surfaces from the dedicated ChatGPT reference: the
// sidebar pull-request chip of every fixture thread and its hover card, the
// thread summary panel (open, closed, the attached pull-request rows and their
// actions menu), and the Background processes section with its stop button,
// the stopping and failed states, the background terminal tab, and the
// command card labels. Dark and light themes, each state with its DOM
// geometry and computed styles.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9334 node scripts/cdp_capture_batch4.mjs \
//     --fixture=artifacts/batch4-fixture-20260929.json \
//     --faults="$HOME/Library/Logs/gpui-capture/batch4-wire-faults.json" \
//     [--output=artifacts/batch4] [--date=20260929] [--only=seed,chips,panel,background] [--theme=dark]
//
// Preconditions (see scripts/batch4_reference_fixture.py):
//   * the instance runs on its own CHATGPT_REFERENCE_CODEX_HOME clone, whose
//     fixture threads the fixture script created;
//   * for `background`, the clone's model provider points at the local fake
//     Responses endpoint of scripts/batch4_app_server_probe.py
//     (BackgroundResponses on 127.0.0.1), so the turns sent here never reach a
//     real model, and the instance was launched with
//     CHATGPT_REFERENCE_WIRE_FAULTS=<the --faults file>, through which the
//     stopping (delayed clean) and failed (clean error) states are produced.
//
// `seed` adds the fixture pull requests with `thread/attachment/add` through
// the page's own app-server connection, exactly as the reference's own write
// path builds it, and records the responses; it only writes the clone.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { Cdp, delay, option } from './stage4/cdp_client.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const root = option('output', 'artifacts/batch4');
const date = option('date', '20260929');
const only = option('only') ? option('only').split(',') : null;
const themes = option('theme') ? [option('theme')] : ['dark', 'light'];
const fixture = JSON.parse(fs.readFileSync(option('fixture', `artifacts/batch4-fixture-${date}.json`), 'utf8'));
const faults = option('faults');
const thread = title => fixture.threads.find(entry => entry.title === title);

const cdp = await Cdp.connect(endpoint);
const evaluate = async expression => cdp.evaluate(expression);
const json = async expression => JSON.parse(await cdp.evaluate(`JSON.stringify(${expression})`));
const viewport = await json('[innerWidth, innerHeight, devicePixelRatio]');
// Echora captures at 1470x923 @2; only emulate when the window differs.
if (viewport[0] !== 1470 || viewport[1] !== 923 || viewport[2] !== 2) {
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1470, height: 923, deviceScaleFactor: 2, mobile: false });
}

const directory = topic => {
  const dir = path.join(`${root}-${topic}-${date}`, 'reference');
  fs.mkdirSync(dir, { recursive: true });
  return dir;
};

const DOM = scope => `(() => {
  const round = value => Math.round(value * 10) / 10;
  const pick = element => {
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return {
      tag: element.tagName, role: element.getAttribute('role'), label: element.getAttribute('aria-label'),
      cls: typeof element.className === 'string' ? element.className.slice(0, 160) : null,
      text: element.childElementCount === 0 ? element.textContent.trim().slice(0, 80) : null,
      rect: [rect.x, rect.y, rect.width, rect.height].map(round),
      font: style.fontSize + '/' + style.lineHeight + ' ' + style.fontWeight + ' ' + style.fontFamily.slice(0, 40),
      color: style.color, background: style.backgroundColor, border: style.borderTopWidth + ' ' + style.borderTopColor,
      radius: style.borderTopLeftRadius, padding: style.padding, opacity: style.opacity, transform: style.transform,
    };
  };
  const scope = ${scope};
  if (!scope) return { missing: true };
  const nodes = [...scope.querySelectorAll('*')].filter(element => {
    const rect = element.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return false;
    const style = getComputedStyle(element);
    return (element.childElementCount === 0 && element.textContent.trim()) || element.tagName === 'svg'
      || element.tagName === 'BUTTON' || style.backgroundColor !== 'rgba(0, 0, 0, 0)' || style.borderTopWidth !== '0px';
  }).slice(0, 600);
  const box = scope.getBoundingClientRect();
  return { viewport: [innerWidth, innerHeight, devicePixelRatio], theme: document.documentElement.dataset.theme,
    scope: [box.x, box.y, box.width, box.height].map(round), nodes: nodes.map(pick) };
})()`;
const MAIN = "document.querySelector('main') || document.body";
const SIDEBAR = "document.querySelector('nav') || document.body";
// The pinned summary island (floating on the right of the thread).
const PANEL = "[...document.querySelectorAll('[class*=rounded-2xl][class*=bg-surface-elevated-secondary]')].pop()";
const POPUP = "[...document.querySelectorAll('[data-radix-popper-content-wrapper],[role=menu],[role=dialog]')].pop()";
const TOAST = "[...document.querySelectorAll('[data-sonner-toast]')].pop()";

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
  await delay(2200);
}

/// Centre of the first visible element matching `selector`, or whose own text
/// or aria-label is `text`.
async function locate({ text = null, selector = null, within = 'document', includes = null }) {
  return json(`(() => {
    const scope = ${within};
    if (!scope) return null;
    const candidates = ${selector ? `[...scope.querySelectorAll(${JSON.stringify(selector)})]` : "[...scope.querySelectorAll('*')]"};
    const match = candidates.find(element => {
      const rect = element.getBoundingClientRect();
      if (!rect.width || !rect.height) return false;
      if (${JSON.stringify(includes)} !== null && !element.textContent.includes(${JSON.stringify(includes)})) return false;
      if (${JSON.stringify(text)} === null) return true;
      if (element.getAttribute('aria-label') === ${JSON.stringify(text)}) return true;
      return element.textContent.trim() === ${JSON.stringify(text)}
        && ![...element.children].some(child => child.textContent.trim() === ${JSON.stringify(text)});
    });
    if (!match) return null;
    const rect = match.getBoundingClientRect();
    return [rect.x + rect.width / 2, rect.y + rect.height / 2];
  })()`);
}

async function hover(target) {
  const point = await locate(target);
  if (!point) throw Error(`nothing to hover: ${JSON.stringify(target)}`);
  await cdp.hover(point[0], point[1]);
  await delay(600);
  return point;
}

async function click(target) {
  const point = await locate(target);
  if (!point) throw Error(`nothing to click: ${JSON.stringify(target)}`);
  await cdp.click(point[0], point[1]);
  await delay(700);
}

async function waitFor(target, attempts = 60) {
  for (let attempt = 0; attempt < attempts; attempt++) {
    if (await locate(target)) return true;
    await delay(250);
  }
  return false;
}

async function setTheme(theme) {
  if ((await evaluate('document.documentElement.dataset.theme')) === theme) return;
  await navigate('/settings/appearance');
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

/// The page's own app-server client, taken from the first `GU(scope, host)`
/// call a logging breakpoint sees; requests sent through it are the
/// reference's own, on its own connection.
async function rpc(method, params) {
  if (!(await evaluate('!!window.__codexRpc'))) {
    await cdp.send('Debugger.enable', {});
    // The breakpoint position comes from the same bundle, extracted from app.asar.
    const source = fs.readFileSync(option('app-shared', findAppShared()), 'utf8');
    const needle = 'function GU(e,t){let n=e.get(KU);';
    const at = source.indexOf(needle);
    if (at < 0) throw Error('app-shared no longer defines GU; update the rpc hook');
    const before = source.slice(0, at + needle.length);
    const lineNumber = before.split('\n').length - 1;
    const lineStart = before.lastIndexOf('\n') + 1;
    const columnNumber = before.slice(lineStart).length;
    const { breakpointId } = await cdp.send('Debugger.setBreakpointByUrl', {
      urlRegex: 'app-shared-[0-9a-f]+\\.js$', lineNumber, columnNumber,
      condition: '(n!=null&&(window.__codexRpc=n),false)',
    });
    for (let attempt = 0; attempt < 60 && !(await evaluate('!!window.__codexRpc')); attempt++) {
      await evaluate(`[...document.querySelectorAll('[data-app-action-sidebar-thread-id]')][${attempt} % 3]?.click()`);
      await delay(700);
    }
    await cdp.send('Debugger.removeBreakpoint', { breakpointId });
    await cdp.send('Debugger.disable', {});
  }
  return JSON.parse(await evaluate(`window.__codexRpc.forHost('local').sendRequest(${JSON.stringify(method)}, ${JSON.stringify(params)})
    .then(result => JSON.stringify({ result }), error => JSON.stringify({ error: String(error && (error.message || error)) }))`));
}

function findAppShared() {
  const assets = option('asar-assets');
  if (!assets) throw Error('pass --asar-assets=<extracted app.asar webview/assets> (or --app-shared=<file>) for the rpc hook');
  const name = fs.readdirSync(assets).find(file => /^app-shared-[0-9a-f]+\.js$/.test(file));
  return path.join(assets, name);
}

/// The reference's identityKey (`JSON.stringify([host, owner, repo, number])`,
/// all lowercased) and payload of a GitHub pull request.
function pullRequestAttachment(threadId, url, rootPath, headBranch) {
  const [, owner, repo, number] = url.match(/^https:\/\/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)$/);
  return {
    threadId, attachmentType: 'pull_request',
    identityKey: JSON.stringify(['github.com', owner.toLowerCase(), repo.toLowerCase(), Number(number)]),
    payload: { url: `https://github.com/${owner}/${repo}/pull/${number}`, root: rootPath, headBranch },
  };
}

async function setFaults(value) {
  if (!faults) throw Error('pass --faults=<CHATGPT_REFERENCE_WIRE_FAULTS file>');
  fs.writeFileSync(faults, JSON.stringify(value));
}

async function send(text) {
  await evaluate(`(() => { const editor = document.querySelector('.ProseMirror[contenteditable=true],[contenteditable=true]'); editor.focus(); document.execCommand('selectAll'); document.execCommand('delete'); })()`);
  await cdp.send('Input.insertText', { text });
  await delay(400);
  await cdp.key('Enter');
  await delay(4000);
}

const STOP = { selector: 'button[aria-label="Stop all background terminals"]' };
// The fixture command's row; the environment section's branch row is a panel
// item too.
const ROW = { selector: '[data-slot=thread-summary-panel-item] button', within: PANEL, includes: 'bg-start' };

async function ensureTerminal(text = 'BGTERM-LONG start') {
  if (await locate({ text: 'Background processes', within: PANEL })) return;
  await send(text);
  if (!(await waitFor({ text: 'Background processes', within: PANEL }, 80))) throw Error('no background terminal appeared');
}

/// Expands every collapsed "Ran a command" group of the thread and scrolls the
/// last command label starting with `prefix` to the middle of the view.
async function revealCard(prefix) {
  const find = `(() => {
    const starts = e => e.textContent.trim().startsWith(${JSON.stringify(prefix)});
    const label = [...document.querySelectorAll('main *')].filter(e => starts(e) && ![...e.children].some(starts)).pop();
    if (!label) return false;
    label.scrollIntoView({ block: 'center' });
    return true;
  })()`;
  if (await evaluate(find)) return delay(600).then(() => true);
  const headers = await json(`[...document.querySelectorAll('main *')]
    .filter(e => e.childElementCount === 0 && /^Ran (a|\\d+) commands?$/.test(e.textContent.trim()))
    .map(e => { const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })`);
  for (const [x, y] of headers) {
    await cdp.click(x, y);
    await delay(500);
  }
  const found = await evaluate(find);
  await delay(600);
  return found;
}

/// Closes the right panel's tabs and hides it: with it open the thread is too
/// narrow for the pinned summary, which then becomes an overlay popover.
async function closeRightPanel() {
  for (let attempt = 0; attempt < 6; attempt++) {
    const close = await locate({ selector: 'button[aria-label^="Close "][aria-label$=" tab"]' });
    if (!close) break;
    await cdp.click(close[0], close[1]);
    await delay(500);
  }
  const hide = await locate({ text: 'Hide tabs' });
  if (hide) {
    await cdp.click(hide[0], hide[1]);
    await delay(700);
  }
}

const wanted = topic => !only || only.includes(topic);

if (wanted('seed')) {
  const seeded = [];
  for (const entry of fixture.threads.filter(entry => entry.url)) {
    const params = pullRequestAttachment(entry.threadId, entry.url, entry.cwd, entry.branch);
    seeded.push({ params, response: await rpc('thread/attachment/add', params) });
  }
  // A pull request of another repository, attached without root or branch the
  // way the reference's attach_artifact tool does: it matches no environment
  // section and lands in the "Pull requests" section.
  const unmatched = pullRequestAttachment(thread('Fixture PR failing').threadId, 'https://github.com/rita152/Echora/pull/13', null, null);
  seeded.push({ params: unmatched, response: await rpc('thread/attachment/add', unmatched) });
  for (const entry of fixture.threads) {
    seeded.push({ list: entry.threadId, response: await rpc('thread/attachment/list', { threadId: entry.threadId, cursor: null, limit: 100 }) });
  }
  const dir = path.join(`${root}-reference-${date}`);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'seed.json'), `${JSON.stringify(seeded, null, 1)}\n`);
  console.log(path.join(dir, 'seed.json'));
}

for (const theme of themes) {
  await setTheme(theme);
  if (wanted('chips')) {
    await navigate(`/local/${thread('Fixture background terminal').threadId}`);
    await closeRightPanel();
    await cdp.hover(1200, 600);
    await delay(3000);
    await capture('chips', 'sidebar', theme, SIDEBAR);
    // The chip hides while its row is hovered; the row's hover card lists the PR.
    for (const title of ['Fixture PR failing', 'Fixture PR merged']) {
      await hover({ text: title, within: SIDEBAR });
      await delay(900);
      await capture('chips', `hover-${title.split(' ').pop()}`, theme, "[...document.querySelectorAll('[data-radix-popper-content-wrapper]')].pop() || document.querySelector('nav')");
    }
    await cdp.hover(1200, 600);
  }
  if (wanted('panel')) {
    // With the checkout on the pull request's head branch, the environment
    // section lists that attached PR under its branch row; the PR of another
    // repository stays in the "Pull requests" section.
    const failing = thread('Fixture PR failing');
    execFileSync('git', ['-C', failing.cwd, 'switch', '-q', failing.branch]);
    try {
      await navigate(`/local/${failing.threadId}`);
      await closeRightPanel();
      await cdp.hover(700, 600);
      await delay(2500);
      await evaluate(`(() => {
        const panel = ${PANEL};
        const toggle = [...panel.querySelectorAll('button[aria-expanded]')].find(b => b.textContent.trim() === 'codex');
        if (toggle && toggle.getAttribute('aria-expanded') === 'false') toggle.click();
      })()`);
      await delay(4000);
      await capture('panel', 'open', theme, PANEL);
      await hover({ text: 'codex', within: PANEL });
      await capture('panel', 'section-hover', theme, PANEL);
      await click({ text: 'Toggle pinned summary' });
      await delay(800);
      await capture('panel', 'closed', theme);
      await click({ text: 'Toggle pinned summary' });
      await delay(1200);
      await capture('panel', 'reopened', theme, PANEL);
      const row = await locate({ selector: 'button[aria-label^="Bump rust-toolchain"]', within: PANEL });
      if (row) {
        await cdp.hover(row[0], row[1]);
        await delay(600);
        await capture('panel', 'pr-row-hover', theme, PANEL);
        await click({ selector: 'button[aria-label^="Actions for Bump"]', within: PANEL });
        await delay(600);
        await capture('panel', 'pr-actions-menu', theme, POPUP);
        await cdp.key('Escape');
        await delay(400);
      }
      await hover({ selector: 'button[aria-label^="Pin the reference"]', within: PANEL });
      await capture('panel', 'unmatched-pr-hover', theme, PANEL);
    } finally {
      execFileSync('git', ['-C', failing.cwd, 'switch', '-q', 'main']);
    }
  }
  if (wanted('background')) {
    // A new chat in the fixture project per theme, so its transcript holds
    // only the turns captured here.
    await navigate(`/local/${thread('Fixture background terminal').threadId}`);
    await closeRightPanel();
    await setFaults({});
    // "New chat" starts in the selected project, which the fixture script set
    // to batch4-codex (the per-project button sits under a hover card).
    await click({ text: 'New chat', within: SIDEBAR });
    await delay(1500);
    await ensureTerminal();
    await cdp.hover(700, 600);
    await capture('background', 'section', theme, PANEL);
    await hover(ROW);
    await capture('background', 'row-hover', theme, PANEL);
    // Keyboard focus reveals the stop button too.
    await cdp.hover(700, 600);
    await evaluate(`[...document.querySelectorAll('[data-slot=thread-summary-panel-item] button')].find(b => b.textContent.includes('bg-start'))?.focus()`);
    await capture('background', 'row-focus', theme, PANEL);
    // The running command card.
    await revealCard('Started background terminal');
    await capture('background', 'card-running', theme);
    // Row click opens the background terminal tab in the right panel.
    await click(ROW);
    await delay(1200);
    await capture('background', 'terminal-tab', theme, 'document.body');
    await closeRightPanel();
    // Stopping: the clean request is held back by the shim.
    await setFaults({ 'thread/backgroundTerminals/clean': { delayMs: 5000 } });
    await hover(ROW);
    await click(STOP);
    await delay(300);
    await capture('background', 'stopping', theme, PANEL);
    await delay(6000);
    await setFaults({});
    await revealCard('Background terminal stopped');
    await capture('background', 'card-stopped', theme);
    // Failure: the clean request is answered with an error.
    await ensureTerminal();
    await setFaults({ 'thread/backgroundTerminals/clean': { error: { code: -32603, message: 'batch4 capture fault' } } });
    await hover(ROW);
    await click(STOP);
    await delay(700);
    await capture('background', 'stop-failed', theme, TOAST);
    await setFaults({});
    await delay(5500);
    await hover(ROW);
    await click(STOP);
    await delay(1500);
    // Natural exit: the command finishes by itself after its turn.
    await send('BGTERM-SHORT start');
    await delay(6000);
    await revealCard('Ran echo bg-start; sleep 2');
    await capture('background', 'card-finished', theme);
  }
}
if (viewport[0] !== 1470 || viewport[1] !== 923 || viewport[2] !== 2) {
  await cdp.send('Emulation.clearDeviceMetricsOverride');
}
cdp.close();
process.exit(0);
