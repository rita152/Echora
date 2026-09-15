// Shared helpers for driving the dedicated ChatGPT reference instance over CDP.
//
// Everything here dispatches real pointer/keyboard input and reads geometry,
// computed styles, and DOM structure back out. It never rewrites React state,
// DOM text, or CSS, so the recordings describe the reference build's real
// behaviour.
import { execFileSync } from 'node:child_process';
import { Cdp } from '../stage4/cdp_client.mjs';

export const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

export function option(name, fallback = null) {
  const prefix = '--' + name + '=';
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) ?? fallback;
}

export function referencePid() {
  const pids = execFileSync('pgrep', ['-f', 'MacOS/ChatGPT --user-data-dir=']).toString().trim().split('\n').filter(Boolean);
  if (pids.length !== 1) throw new Error('expected exactly one dedicated reference instance, found ' + pids.length);
  return pids[0];
}

export async function connect(endpoint = process.env.CHATGPT_CDP_HTTP ?? 'http://127.0.0.1:9333') {
  const cdp = await Cdp.connect(endpoint);
  cdp.endpoint = endpoint;
  return cdp;
}

/// Resize the real window (accessibility API). The app restores its saved
/// size on activation, so callers re-assert the size before a capture.
export async function resizeWindow(cdp, width = 1440, height = 900) {
  const pid = referencePid();
  const script =
    'tell application "System Events" to tell (first process whose unix id is ' +
    pid +
    ') to set size of window 1 to {' + width + ', ' + height + '}';
  execFileSync('osascript', ['-e', script]);
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const size = await cdp.evaluate('window.innerWidth + "x" + window.innerHeight');
    if (size === width + 'x' + height) return size;
    await delay(250);
  }
  throw new Error('window did not reach ' + width + 'x' + height);
}

export async function viewport(cdp) {
  return cdp.evaluate('window.innerWidth + "x" + window.innerHeight + "@" + window.devicePixelRatio');
}

/// Geometry + the computed style properties that matter for pixel parity.
export const STYLE_PROBES = [
  'display', 'position', 'width', 'height', 'padding', 'margin', 'gap',
  'fontFamily', 'fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'fontFeatureSettings',
  'color', 'backgroundColor', 'borderRadius', 'borderColor', 'borderWidth', 'boxShadow',
  'opacity', 'overflow', 'flexDirection', 'alignItems', 'justifyContent', 'textOverflow', 'whiteSpace',
];

export function describeExpression(selectorExpression, { styles = STYLE_PROBES, max = 40 } = {}) {
  return `(() => {
    const nodes = ${selectorExpression};
    return JSON.stringify([...nodes].slice(0, ${max}).map((element) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      const styles = {};
      for (const name of ${JSON.stringify(styles)}) styles[name] = style[name];
      return {
        tag: element.tagName.toLowerCase(),
        role: element.getAttribute('role'),
        label: element.getAttribute('aria-label'),
        cls: (element.className || '').toString().slice(0, 200),
        text: (element.textContent || '').trim().slice(0, 120),
        html: element.outerHTML.slice(0, 400),
        rect: [Math.round(rect.x * 100) / 100, Math.round(rect.y * 100) / 100, Math.round(rect.width * 100) / 100, Math.round(rect.height * 100) / 100],
        styles,
      };
    }), null, 1);
  })()`;
}

export async function describe(cdp, selectorExpression, options) {
  return JSON.parse(await cdp.evaluate(describeExpression(selectorExpression, options)));
}

export async function findAllByLabel(cdp, label) {
  return describe(cdp, `[...document.querySelectorAll('button, [role=button], [role=menuitem], a')].filter((e) => ((e.getAttribute('aria-label') || e.textContent || '').trim()) === ${JSON.stringify(label)})`, { max: 10 });
}

export async function clickLabel(cdp, label, { exact = true, settle = 700 } = {}) {
  const expression = `[...document.querySelectorAll('button, [role=button], [role=menuitem], [role=option], a')].find((e) => { const label = (e.getAttribute('aria-label') || e.textContent || '').trim(); return ${exact ? 'label === ' : 'label.startsWith('}${JSON.stringify(label)}${exact ? '' : ')'}; })`;
  const target = await cdp.evaluate(`(() => { const element = ${expression}; if (!element) return null; const rect = element.getBoundingClientRect(); return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]); })()`);
  if (!target) throw new Error('no clickable element labelled ' + label);
  const [x, y] = JSON.parse(target);
  await cdp.click(Math.round(x), Math.round(y));
  await delay(settle);
  return [Math.round(x), Math.round(y)];
}

export async function clickText(cdp, text, { settle = 700, tag = '*' } = {}) {
  const target = await cdp.evaluate(`(() => {
    const candidates = [...document.querySelectorAll(${JSON.stringify(tag)})].filter((e) => e.children.length === 0 && (e.textContent || '').trim() === ${JSON.stringify(text)});
    const element = candidates.at(-1);
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]);
  })()`);
  if (!target) throw new Error('no element with text ' + text);
  const [x, y] = JSON.parse(target);
  await cdp.click(Math.round(x), Math.round(y));
  await delay(settle);
  return [Math.round(x), Math.round(y)];
}

export async function hover(cdp, x, y, settle = 400) {
  await cdp.hover(x, y);
  await delay(settle);
}

export async function typeText(cdp, text) {
  await cdp.send('Input.insertText', { text });
  await delay(120);
}

export async function press(cdp, key, modifiers = 0) {
  await cdp.key(key, modifiers);
  await delay(150);
}

/// Waits until the expression returns a truthy value; returns its value.
export async function waitFor(cdp, expression, { timeoutMs = 20000, intervalMs = 250 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await cdp.evaluate('(() => { try { return JSON.stringify(' + expression + '); } catch (error) { return JSON.stringify({ error: String(error) }); } })()');
    const value = JSON.parse(last);
    if (value && !(value && value.error)) return value;
    if (value && value.error) throw new Error('waitFor expression failed: ' + value.error);
    await delay(intervalMs);
  }
  throw new Error('waitFor timed out: ' + expression + ' last=' + last);
}

export async function screenshot(cdp, file, options) {
  await cdp.screenshot(file, options);
  return file;
}

/// Switches the reference appearance through its own settings surface using a
/// real pointer click on the theme control, then waits until the document
/// carries the matching appearance class.
export async function setTheme(cdp, theme) {
  const label = theme === 'light' ? 'Light' : 'Dark';
  const className = 'electron-' + theme;
  const settingsOpen = async () =>
    cdp.evaluate("!!document.querySelector('input[name=appearance-theme]')");
  if (!(await settingsOpen())) {
    // The settings surface opens through the application's own Settings
    // command; its keyboard shortcut is the reliable path for a scripted
    // capture, and the page navigation below uses the surface's own controls.
    if (process.platform === 'darwin') {
      try {
        const pid = referencePid();
        execFileSync('osascript', [
          '-e',
          'tell application "System Events" to tell (first process whose unix id is ' + pid + ') to set frontmost to true',
        ]);
        await delay(500);
        execFileSync('osascript', [
          '-e',
          'tell application "System Events" to keystroke "," using command down',
        ]);
        await delay(2000);
      } catch (error) {
        console.log('settings shortcut failed:', error.message);
      }
    }
    // The settings surface remembers its own page; make sure the appearance
    // page is the one on screen.
    for (let attempt = 0; attempt < 40; attempt += 1) {
      if (await settingsOpen()) break;
      const navReady = await cdp.evaluate(
        "[...document.querySelectorAll('button')].some((e) => (e.getAttribute('aria-label') || '').trim() === 'Appearance')",
      );
      if (navReady) {
        await cdp.evaluate(
          "[...document.querySelectorAll('button')].find((e) => (e.getAttribute('aria-label') || '').trim() === 'Appearance')?.click()",
        );
      }
      await delay(300);
    }
  }
  if (!(await settingsOpen())) {
    console.log('appearance settings unavailable');
    return false;
  }
  const clicked = await cdp.evaluate(
    "(() => { const input = document.querySelector('input[name=appearance-theme][aria-label=\"" + label + "\"]'); if (!input) return false; input.click(); return true; })()",
  );
  if (!clicked) {
    console.log('theme control missing for', theme);
    return false;
  }
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const applied = await cdp.evaluate(
      "document.documentElement.getAttribute('data-theme') === " + JSON.stringify(theme),
    );
    if (applied) break;
    await delay(250);
  }
  // Return to the conversation surface; the capture scripts always run there.
  await cdp.evaluate(
    "[...document.querySelectorAll('button')].find((e) => (e.innerText || '').trim() === 'Back to app')?.click()",
  );
  await delay(1200);
  const applied = await cdp.evaluate(
    "document.documentElement.getAttribute('data-theme') === " + JSON.stringify(theme),
  );
  console.log('theme', theme, applied ? 'applied' : 'NOT applied');
  return applied;
}

export function captureRegionOf(rect, { scale = 1 } = {}) {
  return { x: rect[0] * scale, y: rect[1] * scale, width: rect[2] * scale, height: rect[3] * scale };
}

/// Opens one chat through the command menu and waits until its composer and
/// first user turn are on screen.
export async function openThread(cdp, match) {
  await cdp.key('Escape');
  await delay(300);
  await clickLabel(cdp, 'Search', { settle: 1200 });
  const target = await cdp.evaluate(
    "(() => { const node = [...document.querySelectorAll('[cmdk-item]')].find((e) => (e.textContent || '').includes(" + JSON.stringify(match) + ")); if (!node) return null; const rect = node.getBoundingClientRect(); return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]); })()",
  );
  if (!target) throw new Error('chat not found in the command menu: ' + match);
  await cdp.click(...JSON.parse(target));
  for (let attempt = 0; attempt < 40; attempt += 1) {
    const ready = await cdp.evaluate(
      "!!document.querySelector('[data-codex-composer]') && !!document.querySelector('[data-turn-key]')",
    );
    if (ready) break;
    await delay(300);
  }
  return cdp.evaluate("!!document.querySelector('[data-codex-composer]')");
}

/// Sends one prompt through the composer and waits until the turn finishes.
export async function sendPrompt(cdp, text, { timeoutMs = 180000 } = {}) {
  const composer = await cdp.evaluate(
    "(() => { const element = document.querySelector('[data-codex-composer]'); if (!element) return null; const rect = element.getBoundingClientRect(); return JSON.stringify([Math.round(rect.x + rect.width / 2), Math.round(rect.y + rect.height / 2)]); })()",
  );
  if (!composer) throw new Error('composer missing');
  await cdp.click(...JSON.parse(composer));
  await delay(300);
  await cdp.send('Input.insertText', { text });
  await delay(300);
  await cdp.key('Enter');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await delay(2000);
    const label = await cdp.evaluate(
      "(() => { const element = [...document.querySelectorAll('button')].find((e) => ['Send', 'Stop'].includes((e.getAttribute('aria-label') || '').trim())); return element ? element.getAttribute('aria-label') : 'none'; })()",
    );
    if (label === 'Send') return true;
  }
  return false;
}

/// Hovers the newest user message and returns its action bar entries.
export async function hoverNewestUserMessage(cdp) {
  const rect = JSON.parse(
    await cdp.evaluate(
      "(() => { const blocks = [...document.querySelectorAll('[data-turn-key]')].filter((e) => (e.textContent || '').includes('You said')); const target = blocks.at(-1); if (!target) return 'null'; const userBlock = [...target.querySelectorAll('*')].find((e) => (e.getAttribute('aria-label') || '').startsWith('You said')); const rect = (userBlock || target).getBoundingClientRect(); return JSON.stringify([rect.x, rect.y, rect.width, rect.height]); })()",
    ),
  );
  await cdp.hover(rect[0] + rect[2] / 2, rect[1] + rect[3] / 2);
  await delay(900);
  return rect;
}
