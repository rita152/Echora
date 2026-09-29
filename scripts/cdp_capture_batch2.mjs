// Capture the batch-two surfaces from the dedicated ChatGPT reference: the
// hooks page and its source dialog, Configuration → Experimental features
// (Beta), Personalization → Codex memory, `/memories` and find in chat, in
// the dark and light themes, with each state's DOM geometry and styles.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9334 \
//     node scripts/cdp_capture_batch2.mjs --output=artifacts/batch2 --date=20260928 [--only=hooks,find]
//
// Run it only against an instance launched with its own
// CHATGPT_REFERENCE_CODEX_HOME clone: switching the theme, trusting hooks,
// toggling a feature or a memory switch writes that clone's config.toml. The
// hooks page needs the fixture hooks from the batch-two notes in the clone
// and in its fixture project. Nothing here sends a model request: chats are
// only opened, never sent to.
//
// The experimental section is behind Statsig gate 2106641128, which is off
// for this account; like the legacy-layout pin it is overridden in the page's
// memory only (chained on top of that pin), never persisted.
import fs from 'node:fs';
import path from 'node:path';
import { Cdp, delay, option } from './stage4/cdp_client.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const root = option('output', 'artifacts/batch2');
const date = option('date', '20260928');
const only = option('only') ? option('only').split(',') : null;
const themes = option('theme') ? [option('theme')] : ['dark', 'light'];
const findThread = option('find-thread', '01a0e6df-5365-7252-be73-e37c99d68445');
const EXPERIMENTAL_GATE = '2106641128';
// `trusted`: the clone's first PreToolUse and Stop hooks were trusted and the
// Stop command changed since, so the dialog shows trusted and modified rows.
const hooksVariant = option('hooks-variant');

const cdp = await Cdp.connect(endpoint);
const evaluate = async expression => cdp.evaluate(expression);
const json = async expression => JSON.parse(await cdp.evaluate(`JSON.stringify(${expression})`));

const directory = topic => {
  const dir = path.join(`${root}-${topic}-${date}`, 'reference');
  fs.mkdirSync(dir, { recursive: true });
  return dir;
};

// Geometry and computed style of every element with text or a box, so the
// Echora side can be checked against numbers and not only pixels.
const DOM = `(() => {
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
      radius: style.borderTopLeftRadius,
    };
  };
  const scope = [...document.querySelectorAll('[role=dialog]')].pop() || document.querySelector('main') || document.body;
  const nodes = [...scope.querySelectorAll('*')].filter(element => {
    const rect = element.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return false;
    const style = getComputedStyle(element);
    return (element.childElementCount === 0 && element.textContent.trim()) || element.tagName === 'svg'
      || style.backgroundColor !== 'rgba(0, 0, 0, 0)' || style.borderTopWidth !== '0px';
  }).slice(0, 400);
  return { viewport: [innerWidth, innerHeight, devicePixelRatio], theme: document.documentElement.dataset.theme, nodes: nodes.map(pick) };
})()`;

async function capture(topic, name, theme) {
  await delay(500);
  const dir = directory(topic);
  const file = path.join(dir, `${name}-${theme}.png`);
  await cdp.screenshot(file);
  fs.writeFileSync(`${file}.dom.json`, `${JSON.stringify(await json(DOM), null, 1)}\n`);
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
  await delay(1500);
}

/// Centre of the first visible element whose own text (or aria-label) is `text`.
async function locate(text, within = 'document') {
  return json(`(() => {
    const scope = ${within === 'dialog' ? "[...document.querySelectorAll('[role=dialog]')].pop()" : 'document'};
    const match = [...scope.querySelectorAll('*')].find(element => {
      const rect = element.getBoundingClientRect();
      if (!rect.width || !rect.height) return false;
      return element.getAttribute('aria-label') === ${JSON.stringify(text)}
        || (element.childElementCount === 0 && element.textContent.trim() === ${JSON.stringify(text)});
    });
    if (!match) return null;
    const rect = (match.closest('button,[role=button],[role=switch],a') || match).getBoundingClientRect();
    return [rect.x + rect.width / 2, rect.y + rect.height / 2];
  })()`);
}

async function click(text, within) {
  const point = await locate(text, within);
  if (!point) throw Error(`nothing labelled "${text}" to click`);
  await cdp.click(point[0], point[1]);
  await delay(700);
}

async function pinExperimentalGate() {
  await evaluate(`(async () => {
    const pins = window.__GATE_PINS__ || {};
    pins[${JSON.stringify(EXPERIMENTAL_GATE)}] = true;
    window.__GATE_PINS__ = pins;
    const client = window.__STATSIG__.firstInstance;
    // Re-wrap whenever the gate does not read as pinned: the legacy-layout pin
    // replaces getFeatureGate each time it runs, dropping this wrapper.
    if (client.getFeatureGate(${JSON.stringify(EXPERIMENTAL_GATE)}, { disableExposureLog: true }).value !== true) {
      const inner = client.getFeatureGate.bind(client);
      client.getFeatureGate = (name, options) => {
        const evaluation = inner(name, options);
        return name in window.__GATE_PINS__
          ? { ...evaluation, value: window.__GATE_PINS__[name], details: { ...evaluation.details, reason: 'LocalOverride' } }
          : evaluation;
      };
    }
    client.$emt({ name: 'values_updated', status: 'Ready', values: null });
    await new Promise(resolve => setTimeout(resolve, 1500));
  })()`);
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

/// Waits up to five seconds for an element whose own text is `text`.
async function waitFor(text) {
  for (let attempt = 0; attempt < 25; attempt++) {
    if (await locate(text)) return;
    await delay(200);
  }
  throw Error(`"${text}" never appeared`);
}

/// Scrolls the settings content so `heading` starts at `top`.
async function scrollHeadingTo(heading, top) {
  await waitFor(heading);
  await evaluate(`(() => {
    const element = [...document.querySelectorAll('*')].find(node => node.childElementCount === 0 && node.textContent.trim() === ${JSON.stringify(heading)});
    let scroller = element.parentElement;
    while (scroller && !(scroller.scrollHeight > scroller.clientHeight && /(auto|scroll)/.test(getComputedStyle(scroller).overflowY))) scroller = scroller.parentElement;
    scroller.scrollTop += element.getBoundingClientRect().top - ${top};
  })()`);
  await delay(400);
}

async function focusComposer() {
  await evaluate(`(() => { const editor = document.querySelector('.ProseMirror,[contenteditable=true]'); editor.focus(); document.execCommand('selectAll'); document.execCommand('delete'); })()`);
  await delay(200);
}

async function type(text) {
  await cdp.send('Input.insertText', { text });
  await delay(600);
}

const wanted = topic => !only || only.includes(topic);

for (const theme of themes) {
  await setTheme(theme);
  if (wanted('hooks') && hooksVariant === 'trusted') {
    await navigate('/settings/hooks-settings');
    await capture('hooks', 'trusted-overview', theme);
    await click('User config');
    await capture('hooks', 'trusted', theme);
    await cdp.key('Escape');
  } else if (wanted('hooks')) {
    await navigate('/settings/hooks-settings');
    await capture('hooks', 'overview', theme);
    await click('User config');
    await capture('hooks', 'dialog', theme);
    await click('1 - Checking the command', 'dialog');
    await capture('hooks', 'expanded', theme);
    await click('1 - Checking the command', 'dialog');
    await click('1 issue loading hooks for this source', 'dialog');
    await capture('hooks', 'issues', theme);
    await cdp.key('Escape');
  }
  if (wanted('features')) {
    await navigate('/settings/general-settings');
    await click('Configuration');
    await waitFor('Agent defaults').catch(() => {});
    await pinExperimentalGate();
    // Every state starts with the flag off; a remount clears the restart note.
    if ((await evaluate(`document.querySelector('[aria-label="Toggle Analytics plan history"]').getAttribute('aria-checked')`)) === 'true') {
      await click('Toggle Analytics plan history');
      await delay(1500);
      await navigate('/settings/general-settings');
      await click('Configuration');
      await pinExperimentalGate();
    }
    // Where Echora's shorter Configuration page shows the heading unscrolled.
    await scrollHeadingTo('Experimental features (Beta)', 595.6);
    await capture('features', 'list', theme);
    await click('Toggle Analytics plan history');
    await delay(1500);
    await capture('features', 'restart', theme);
    // Back to off, so every theme starts from the same state.
    await click('Toggle Analytics plan history');
    await delay(1500);
  }
  if (wanted('memories')) {
    await navigate('/settings/personalization');
    await capture('memories', 'settings-on', theme);
    await click('Delete');
    await capture('memories', 'delete-confirm', theme);
    await click('Cancel', 'dialog');
    await navigate('/');
    await focusComposer();
    await type('/mem');
    await capture('memories', 'slash', theme);
    await cdp.key('Enter');
    await capture('memories', 'dialog-new', theme);
    await cdp.key('Escape');
    await navigate(`/local/${findThread}`);
    await delay(1500);
    await focusComposer();
    await type('/mem');
    await cdp.key('Enter');
    await capture('memories', 'dialog-started', theme);
    await click('Generate memories', 'dialog');
    await capture('memories', 'dialog-generate-off', theme);
    await click('Generate memories', 'dialog');
    await cdp.key('Escape');
  }
  if (wanted('find')) {
    await navigate(`/local/${findThread}`);
    await delay(1500);
    await cdp.click(800, 300);
    // What the menu accelerator (⌘F) sends: CDP key events never reach it.
    await evaluate(`window.dispatchEvent(new MessageEvent('message', { data: { type: 'find-in-thread' } }))`);
    await delay(600);
    await capture('find', 'open', theme);
    await type('hello');
    // Wait for the answer, not only the field.
    for (let attempt = 0; attempt < 20 && !(await locate('1 / 2 results')); attempt++) await delay(250);
    await capture('find', 'results', theme);
    await cdp.key('Enter');
    await capture('find', 'second', theme);
    await evaluate(`(() => { const input = document.activeElement; input.select(); })()`);
    await type('zzz');
    await delay(1000);
    await capture('find', 'none', theme);
    await cdp.key('Escape');
  }
}
cdp.close();
