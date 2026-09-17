// Capture the ChatGPT plugin / app management surfaces from one dedicated
// debug instance.
//
// Usage:
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9334 node scripts/cdp_capture_manage_settings.mjs \
//     --output artifacts/p2-manage/chatgpt-reference --theme=light
//
// Pointer input goes through CDP input events; readings come from
// getComputedStyle/getBoundingClientRect on the live page and images from
// Page.captureScreenshot. The script never rewrites React state, DOM text, or
// CSS: it clicks the real navigation and the real segment chips.
import fs from 'node:fs';
import path from 'node:path';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');

function argument(name, fallback) {
  const prefix = '--' + name + '=';
  const found = process.argv.find(value => value.startsWith(prefix));
  return found ? found.slice(prefix.length) : fallback;
}

const output = argument('output', 'artifacts/p2-manage/chatgpt-reference');
const theme = argument('theme', 'light');
const surface = argument('surface', 'all');
fs.mkdirSync(output, { recursive: true });

const actions = [];
const record = (action, detail) =>
  actions.push({ at: new Date().toISOString(), action, ...(detail || {}) });

const list = await (await fetch(endpoint + '/json/list')).json();
const page = list.find(target => target.url === 'app://-/index.html');
if (!page) throw Error('ChatGPT main window not found');

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});
let id = 0;
const pending = new Map();
socket.onmessage = event => {
  const message = JSON.parse(event.data);
  const handler = pending.get(message.id);
  if (!handler) return;
  pending.delete(message.id);
  message.error ? handler.reject(Error(message.error.message)) : handler.resolve(message.result);
};
const send = (method, params) =>
  new Promise((resolve, reject) => {
    const request = ++id;
    pending.set(request, { resolve, reject });
    socket.send(JSON.stringify({ id: request, method, params: params || {} }));
  });
const evaluate = async expression => {
  const result = await send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
};
const json = async expression => JSON.parse(await evaluate('JSON.stringify(' + expression + ')'));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

const click = async (x, y) => {
  await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x,
    y,
    button: 'left',
    clickCount: 1,
  });
  await send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x,
    y,
    button: 'left',
    clickCount: 1,
  });
};
const clickRect = async (rect, label) => {
  await click(rect.x + rect.width / 2, rect.y + rect.height / 2);
  record(label, { rect });
  await sleep(500);
};
const clickByText = async (selector, text, label) => {
  const rect = await json(
    '(() => {' +
      '  const node = [...document.querySelectorAll(' +
      JSON.stringify(selector) +
      ')].find(element => (element.innerText || "").trim().startsWith(' +
      JSON.stringify(text) +
      '));' +
      '  return node ? node.getBoundingClientRect().toJSON() : null;' +
      '})()',
  );
  if (!rect) {
    record(label + '-missing', { text });
    return false;
  }
  await clickRect(rect, label);
  return true;
};
const screenshot = async name => {
  const result = await send('Page.captureScreenshot', { format: 'png', fromSurface: true });
  const file = path.join(output, name + '.png');
  fs.writeFileSync(file, Buffer.from(result.data, 'base64'));
  record('screenshot', { file: name + '.png' });
  return file;
};

/// The management surfaces report their visible rows, the list geometry and the
/// palette actually computed by the renderer.
const dumpSurface = () =>
  json(
    '(() => {' +
      '  const rows = [...document.querySelectorAll("div")].filter(element => {' +
      '    const box = element.getBoundingClientRect();' +
      '    return box.width > 300 && box.height > 60 && box.height < 80 && element.children.length >= 2;' +
      '  });' +
      '  const style = node => node ? getComputedStyle(node) : null;' +
      '  return {' +
      '    text: document.body.innerText.slice(0, 4000),' +
      '    rows: rows.slice(0, 12).map(element => {' +
      '      const box = element.getBoundingClientRect();' +
      '      const label = element.children[1];' +
      '      const title = label && label.children[0] ? label.children[0] : null;' +
      '      const subtitle = label && label.children[1] ? label.children[1] : null;' +
      '      return {' +
      '        rect: box.toJSON(),' +
      '        title: title ? title.innerText : null,' +
      '        subtitle: subtitle ? subtitle.innerText : null,' +
      '        titleStyle: title ? { fontSize: style(title).fontSize, fontWeight: style(title).fontWeight, color: style(title).color, lineHeight: style(title).lineHeight } : null,' +
      '        subtitleStyle: subtitle ? { fontSize: style(subtitle).fontSize, fontWeight: style(subtitle).fontWeight, color: style(subtitle).color, lineHeight: style(subtitle).lineHeight } : null' +
      '      };' +
      '    }),' +
      '    switches: [...document.querySelectorAll("[role=switch],button[aria-checked]")].slice(0, 12).map(element => ({' +
      '      checked: element.getAttribute("aria-checked"), rect: element.getBoundingClientRect().toJSON() })),' +
      '    viewport: [innerWidth, innerHeight, devicePixelRatio],' +
      '    scroll: (() => { const scroller = [...document.querySelectorAll("div")].find(element => element.scrollHeight > element.clientHeight + 50); return scroller ? scroller.scrollTop : 0; })()' +
      '  };' +
      '})()',
  );

async function enterSettings() {
  // Cmd+, is the app's own Settings shortcut; it avoids depending on the
  // profile menu's current wording.
  for (const type of ['keyDown', 'rawKeyDown']) {
    await send('Input.dispatchKeyEvent', {
      type,
      modifiers: 4,
      key: ',',
      code: 'Comma',
      windowsVirtualKeyCode: 188,
      nativeVirtualKeyCode: 43,
    });
  }
  await send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    modifiers: 4,
    key: ',',
    code: 'Comma',
    windowsVirtualKeyCode: 188,
    nativeVirtualKeyCode: 43,
  });
  record('settings-shortcut', {});
  await sleep(2500);
  const open = await evaluate('document.body.innerText.includes("Plugins") || document.body.innerText.includes("插件")');
  if (!open) throw Error('Settings did not open');
}

/// Clicks a settings entry whose visible label is either language.
const clickByEither = async (selector, labels, label) => {
  for (const text of labels) {
    if (await clickByText(selector, text, label)) return true;
  }
  return false;
};

await send('Page.enable');
await send('Runtime.enable');
await send('Emulation.setEmulatedMedia', {
  features: [{ name: 'prefers-color-scheme', value: theme }],
});
record('window', { endpoint, tab: page.id, url: page.url, theme, surface });

if (surface === 'all' || surface === 'plugins') {
  await enterSettings();
  await clickByEither(
    'button,a,[role="menuitem"],div[role="button"]',
    ['插件', 'Plugins'],
    'settings-nav-plugins',
  );
  await sleep(2500);
  const segmentLanguages = {
    插件: ['Plugin', 'Plugins'],
    应用: ['App', 'Apps'],
    MCP: ['MCP', 'MCP'],
    技能: ['Skill', 'Skills'],
  };
  for (const segment of ['插件', '应用', 'MCP', '技能']) {
    const clicked = await clickByEither(
      'button,div[role="button"],div',
      [segment + ' ', segmentLanguages[segment].map(label => label + ' ')[0], segmentLanguages[segment].map(label => label + ' ')[1]],
      'segment-' + segment,
    );
    await sleep(2500);
    const dump = await dumpSurface();
    fs.writeFileSync(
      path.join(output, 'plugins-' + segment + '-' + theme + '.json'),
      JSON.stringify(dump, null, 2),
    );
    await screenshot('plugins-' + segment + '-' + theme);
    record('segment', { segment, clicked });
  }
}

fs.writeFileSync(
  path.join(output, 'actions-' + theme + '-' + surface + '.jsonl'),
  actions.map(item => JSON.stringify(item)).join('\n') + '\n',
);
console.log('captured', surface, theme, '->', output);
