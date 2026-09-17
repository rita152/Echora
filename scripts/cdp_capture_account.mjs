// Capture the ChatGPT account surfaces from one dedicated debug instance.
//
// Usage:
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9333 node scripts/cdp_capture_account.mjs \
//     --output artifacts/account-phase/chatgpt-reference --theme=light
//
// Pointer and keyboard input is dispatched through CDP input events; readings
// come from getComputedStyle/getBoundingClientRect on the live page, and images
// come from Page.captureScreenshot. The script never rewrites React state, DOM
// text, or CSS, and it never confirms a logout: the confirmation dialog is
// captured and then cancelled.
import fs from 'node:fs';
import path from 'node:path';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');

function argument(name, fallback) {
  const prefix = '--' + name + '=';
  const found = process.argv.find(value => value.startsWith(prefix));
  return found ? found.slice(prefix.length) : fallback;
}

const output = argument('output', 'artifacts/account-phase/chatgpt-reference');
const theme = argument('theme', 'light');
const surface = argument('surface', 'all');
fs.mkdirSync(output, { recursive: true });

const actions = [];
const record = (action, detail) => actions.push({ at: new Date().toISOString(), action, ...(detail || {}) });

const list = await (await fetch(endpoint + '/json/list')).json();
const page = list.find(target => target.url === 'app://-/index.html');
if (!page) throw Error('ChatGPT main window not found');

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
let id = 0;
const pending = new Map();
socket.onmessage = event => {
  const message = JSON.parse(event.data);
  const handler = pending.get(message.id);
  if (!handler) return;
  pending.delete(message.id);
  message.error ? handler.reject(Error(message.error.message)) : handler.resolve(message.result);
};
const send = (method, params) => new Promise((resolve, reject) => {
  const request = ++id;
  pending.set(request, { resolve, reject });
  socket.send(JSON.stringify({ id: request, method, params: params || {} }));
});
const evaluate = async expression => {
  const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
};
const json = async expression => JSON.parse(await evaluate('JSON.stringify(' + expression + ')'));
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const click = async (x, y) => {
  await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
  await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
  record('click', { x: Math.round(x), y: Math.round(y) });
};
const clickRect = async (rect, label) => {
  record('click-target', { label, rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } });
  await click(rect.x + rect.width / 2, rect.y + rect.height / 2);
};
const wheel = async (x, y, deltaY) => {
  await send('Input.dispatchMouseEvent', { type: 'mouseWheel', x, y, deltaX: 0, deltaY });
  record('wheel', { x: Math.round(x), y: Math.round(y), deltaY });
};
const key = async (keyName, code, windowsVirtualKeyCode, modifiers) => {
  const event = { key: keyName, code, windowsVirtualKeyCode, nativeVirtualKeyCode: windowsVirtualKeyCode, modifiers: modifiers || 0 };
  await send('Input.dispatchKeyEvent', Object.assign({ type: 'keyDown' }, event));
  await send('Input.dispatchKeyEvent', Object.assign({ type: 'keyUp' }, event));
  record('key', { key: keyName, modifiers: modifiers || 0 });
};
const escape = () => key('Escape', 'Escape', 27);

async function screenshot(name, clip) {
  const shot = await send('Page.captureScreenshot', Object.assign(
    { format: 'png', captureBeyondViewport: false },
    clip ? { clip: Object.assign({ scale: 1 }, clip) } : {},
  ));
  fs.writeFileSync(path.join(output, name + '.png'), Buffer.from(shot.data, 'base64'));
  record('screenshot', { name, clip: clip || null });
}

// Live measurements for every account surface this phase renders.
const PROBE = [
  '(() => {',
  '  const rect = element => element ? element.getBoundingClientRect().toJSON() : null;',
  '  const visible = element => { const box = element.getBoundingClientRect(); return box.width > 0 && box.height > 0; };',
  '  const style = element => {',
  '    const computed = getComputedStyle(element);',
  '    return {',
  '      fontFamily: computed.fontFamily, fontSize: computed.fontSize, fontWeight: computed.fontWeight,',
  '      lineHeight: computed.lineHeight, color: computed.color, backgroundColor: computed.backgroundColor,',
  '      borderRadius: computed.borderRadius, border: computed.border, boxShadow: computed.boxShadow,',
  '      padding: computed.padding, gap: computed.gap, width: computed.width, height: computed.height,',
  '      backdropFilter: computed.backdropFilter, margin: computed.margin, opacity: computed.opacity,',
  '    };',
  '  };',
  '  const describe = element => ({',
  '    tag: element.tagName, role: element.getAttribute("role"), label: element.getAttribute("aria-label"),',
  '    state: element.getAttribute("data-state"), expanded: element.getAttribute("aria-expanded"),',
  '    text: element.innerText, rect: rect(element), style: style(element),',
  '  });',
  '  const trigger = document.querySelector(\'button[aria-label="打开个人资料菜单"]\');',
  '  const menu = [...document.querySelectorAll(\'[role="menu"]\')].find(visible);',
  '  const dialog = [...document.querySelectorAll(\'[role="dialog"],[role="alertdialog"]\')].find(visible);',
  '  return JSON.stringify({',
  '    url: location.href, viewport: [innerWidth, innerHeight], dpr: devicePixelRatio,',
  '    htmlClass: document.documentElement.className, colorScheme: getComputedStyle(document.documentElement).colorScheme,',
  '    trigger: trigger ? describe(trigger) : null,',
  '    menu: menu ? Object.assign({}, describe(menu), {',
  '      items: [...menu.querySelectorAll(\'[role="menuitem"]\')].map(item => ({',
  '        text: item.innerText, rect: rect(item), style: style(item),',
  '        icon: (() => { const svg = item.querySelector("svg"); return svg ? rect(svg) : null; })(),',
  '        avatar: (() => { const image = item.querySelector("img"); return image ? Object.assign(rect(image), { src: image.getAttribute("src") }) : null; })(),',
  '      })),',
  '    }) : null,',
  '    dialog: dialog ? Object.assign({}, describe(dialog), { text: dialog.innerText, buttons: [...dialog.querySelectorAll("button")].map(describe) }) : null,',
  '  });',
  '})()',
].join('\n');

const probe = async () => JSON.parse(await evaluate(PROBE));
const triggerRect = () => json('(() => { const button = document.querySelector(\'button[aria-label="打开个人资料菜单"]\'); return button ? button.getBoundingClientRect().toJSON() : null; })()');

// Settings replaces the main route, so leaving it is part of returning to the
// account menu instead of a synthetic route change.
async function leaveSettings() {
  const back = await json('(() => {' +
    '  const button = [...document.querySelectorAll("button,a")].find(candidate =>' +
    '    /返回应用|返回 ChatGPT|Back to/.test(candidate.innerText.trim()));' +
    '  return button ? button.getBoundingClientRect().toJSON() : null;' +
    '})()');
  if (!back) return false;
  await clickRect(back, 'leave-settings');
  await sleep(1800);
  return true;
}

async function openProfileMenu() {
  let state = await probe();
  if (state.menu) return state;
  let rect = state.trigger ? state.trigger.rect : await triggerRect();
  if (!rect) {
    await leaveSettings();
    rect = await triggerRect();
  }
  if (!rect) throw Error('Profile menu trigger is unavailable on this route');
  await clickRect(rect, 'profile-menu-trigger');
  await sleep(800);
  state = await probe();
  if (!state.menu) throw Error('Profile menu did not open');
  return state;
}

async function closeMenus() {
  await escape();
  await sleep(500);
  await escape();
  await sleep(400);
}

const menuItem = needle => json('(() => {' +
  '  const menu = [...document.querySelectorAll(\'[role="menu"]\')].find(element => {' +
  '    const box = element.getBoundingClientRect(); return box.width > 0 && box.height > 0; });' +
  '  if (!menu) return null;' +
  '  const item = [...menu.querySelectorAll(\'[role="menuitem"]\')].find(candidate => candidate.innerText.includes(' + JSON.stringify(needle) + '));' +
  '  return item ? item.getBoundingClientRect().toJSON() : null;' +
  '})()');

async function clickByText(selector, needle, label) {
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const found = await json('(() => {' +
      '  const candidates = [...document.querySelectorAll(' + JSON.stringify(selector) + ')].filter(element => {' +
      '    const box = element.getBoundingClientRect();' +
      '    return box.width > 0 && box.height > 0 && box.top > 0 && box.bottom < innerHeight; });' +
      '  const exact = candidates.find(candidate => candidate.innerText.trim() === ' + JSON.stringify(needle) + ');' +
      '  const element = exact || candidates.find(candidate => candidate.innerText.includes(' + JSON.stringify(needle) + '));' +
      '  return element ? element.getBoundingClientRect().toJSON() : null;' +
      '})()');
    if (found) {
      record('click-text', { label, needle, rect: found, attempt });
      await click(found.x + found.width / 2, found.y + found.height / 2);
      return true;
    }
    await wheel(400, 500, 320);
    await sleep(350);
  }
  record('click-text-missing', { label, needle });
  return false;
}

async function waitFor(predicate, attempts, delay) {
  for (let attempt = 0; attempt < (attempts || 12); attempt += 1) {
    if (await predicate()) return true;
    await sleep(delay || 400);
  }
  return false;
}

await send('Page.enable');
await send('Runtime.enable');
await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }] });
record('window', { endpoint, tab: page.id, url: page.url, theme, surface });
await closeMenus();
const viewport = await json('[innerWidth, innerHeight, devicePixelRatio]');
record('viewport', { viewport });

// "all" is the product capture set; the appearance dump and the theme switch
// are explicit preparation surfaces.
const META_SURFACES = ['appearance-dump', 'set-theme'];
const want = name => surface === 'all'
  ? !META_SURFACES.includes(name)
  : surface.split(',').includes(name);

async function enterSettings() {
  await openProfileMenu();
  const settings = await menuItem('设置');
  if (!settings) throw Error('Settings entry missing from the profile menu');
  await clickRect(settings, 'menu-settings');
  await sleep(2000);
}

if (want('appearance-dump')) {
  await enterSettings();
  const opened = await clickByText('button,a,[role="menuitem"],div[role="button"]', '外观', 'settings-nav-appearance');
  await sleep(1500);
  const detail = await json('(() => {' +
    '  const nodes = [...document.querySelectorAll("button,[role=radio],[role=switch],[role=combobox],img,svg")].filter(element => {' +
    '    const box = element.getBoundingClientRect(); return box.width > 0 && box.height > 0; });' +
    '  return { opened: ' + JSON.stringify(opened) + ', text: document.body.innerText.slice(0, 2500), controls: nodes.slice(0, 50).map(element => ({' +
    '    tag: element.tagName, role: element.getAttribute("role"), state: element.getAttribute("data-state"),' +
    '    label: element.getAttribute("aria-label"), text: (element.innerText || "").slice(0, 60), rect: element.getBoundingClientRect().toJSON() })) };' +
    '})()');
  fs.writeFileSync(path.join(output, 'appearance-dump-' + theme + '.json'), JSON.stringify(detail, null, 2));
  console.log(JSON.stringify({ surface: 'appearance-dump', controls: detail.controls.length }));
  fs.writeFileSync(path.join(output, 'actions-' + theme + '-' + surface + '.jsonl'), actions.map(item => JSON.stringify(item)).join('\n') + '\n');
  socket.close();
  process.exit(0);
}

if (want('menu')) {
  const closed = await probe();
  fs.writeFileSync(path.join(output, 'account-closed-' + theme + '.json'), JSON.stringify(closed, null, 2));
  await screenshot('account-closed-' + theme);

  const opened = await openProfileMenu();
  fs.writeFileSync(path.join(output, 'account-menu-' + theme + '.json'), JSON.stringify(opened, null, 2));
  const box = opened.menu.rect;
  await screenshot('account-menu-' + theme, {
    x: Math.max(0, box.x - 16), y: Math.max(0, box.y - 16), width: box.width + 32, height: box.height + 32,
  });

  if (want('logout')) {
    const logout = await menuItem('退出登录');
    record('menu-logout-item', { rect: logout });
    if (logout) {
      await clickRect(logout, 'menu-logout');
      await sleep(900);
      const dialog = await probe();
      fs.writeFileSync(path.join(output, 'logout-confirm-' + theme + '.json'), JSON.stringify(dialog, null, 2));
      if (dialog.dialog) {
        const bounds = dialog.dialog.rect;
        await screenshot('logout-confirm-' + theme, {
          x: Math.max(0, bounds.x - 24), y: Math.max(0, bounds.y - 24),
          width: bounds.width + 48, height: bounds.height + 48,
        });
      } else {
        await screenshot('logout-confirm-' + theme);
      }
      await escape();
      await sleep(700);
      let after = await probe();
      if (after.dialog) {
        await clickByText('[role="dialog"] button,[role="alertdialog"] button', '取消', 'logout-cancel');
        await sleep(700);
        after = await probe();
      }
      record('logout-confirm-cancelled', { dialogStillOpen: Boolean(after.dialog) });
      fs.writeFileSync(path.join(output, 'after-logout-cancel-' + theme + '.json'), JSON.stringify(after, null, 2));
    }
  }
  await closeMenus();
}

if (want('settings')) {
  await openProfileMenu();
  const settings = await menuItem('设置');
  record('menu-settings-item', { rect: settings });
  if (settings) {
    await clickRect(settings, 'menu-settings');
    await sleep(2000);
    const url = await evaluate('location.href');
    record('settings-route', { url });
  }
}

if (want('logout-dump')) {
  await openProfileMenu();
  const item = await menuItem('退出登录');
  if (!item) throw Error('Logout entry missing from the account menu');
  await clickRect(item, 'menu-logout');
  await sleep(900);
  const detail = await json('(() => {' +
    '  const dialog = [...document.querySelectorAll("[role=dialog],[role=alertdialog]")].find(element => element.getBoundingClientRect().width > 0);' +
    '  if (!dialog) return { missing: true };' +
    '  const leaves = [...dialog.querySelectorAll("*")].filter(element => element.children.length === 0 && (element.textContent || "").trim().length > 0);' +
    '  return { rect: dialog.getBoundingClientRect().toJSON(),' +
    '    leaves: leaves.map(element => { const computed = getComputedStyle(element); const box = element.getBoundingClientRect();' +
    '      return { tag: element.tagName, text: element.textContent.trim().slice(0, 40), rect: box.toJSON(),' +
    '        fontSize: computed.fontSize, fontWeight: computed.fontWeight, lineHeight: computed.lineHeight, color: computed.color, background: computed.backgroundColor, radius: computed.borderRadius, padding: computed.padding }; }),' +
    '    buttons: [...dialog.querySelectorAll("button")].map(element => { const computed = getComputedStyle(element); const box = element.getBoundingClientRect();' +
    '      return { label: element.getAttribute("aria-label"), text: element.innerText.trim().slice(0, 20), rect: box.toJSON(), background: computed.backgroundColor, color: computed.color, radius: computed.borderRadius, padding: computed.padding, fontSize: computed.fontSize, border: computed.border }; }) };' +
    '})()');
  fs.writeFileSync(path.join(output, 'logout-dump-' + theme + '.json'), JSON.stringify(detail, null, 2));
  // Leave the confirmation without submitting it.
  await escape();
  await sleep(600);
  console.log(JSON.stringify({ surface: 'logout-dump', leaves: detail.leaves ? detail.leaves.length : 0 }));
  // The app owns its theme, so switching it uses the same Appearance controls a
// user would click instead of overriding CSS.
if (want('set-theme')) {
  await enterSettings();
  await clickByText('button,a,[role="menuitem"],div[role="button"]', '外观', 'settings-nav-appearance');
  await sleep(1400);
  const option = theme === 'dark' ? '深色' : '浅色';
  // The theme options are a segmented control: the option label is the deepest
  // node carrying that text, so the click targets the label the user sees.
  const switched = await json('(() => {' +
    '  const leaves = [...document.querySelectorAll("*")].filter(element =>' +
    '    element.children.length === 0 && element.textContent.trim() === ' + JSON.stringify(option) + ');' +
    '  const leaf = leaves.find(element => { const box = element.getBoundingClientRect(); return box.width > 0 && box.height > 0; });' +
    '  return leaf ? leaf.getBoundingClientRect().toJSON() : null;' +
    '})()');
  record('theme-option', { option, rect: switched });
  if (switched) {
    await clickRect(switched, 'theme-' + theme);
    await sleep(1500);
  }
  const applied = await json('({ htmlClass: document.documentElement.className, colorScheme: getComputedStyle(document.documentElement).colorScheme, background: getComputedStyle(document.body).backgroundColor })');
  record('theme-applied', applied);
  fs.writeFileSync(path.join(output, 'theme-' + theme + '.json'), JSON.stringify(applied, null, 2));
  await leaveSettings();
  fs.writeFileSync(path.join(output, 'actions-' + theme + '-set-theme.jsonl'), actions.map(item => JSON.stringify(item)).join('\n') + '\n');
  console.log(JSON.stringify({ surface: 'set-theme', theme, applied }));
  socket.close();
  process.exit(0);
}

fs.writeFileSync(path.join(output, 'actions-' + theme + '-' + surface + '.jsonl'), actions.map(item => JSON.stringify(item)).join('\n') + '\n');
fs.writeFileSync(path.join(output, 'session-' + theme + '-' + surface + '.json'), JSON.stringify({
  endpoint, tab: page.id, url: page.url, theme, surface, viewport,
}, null, 2));
console.log(JSON.stringify({ output, theme, surface, viewport, actions: actions.length }));
socket.close();
fs.writeFileSync(path.join(output, 'actions-' + theme + '-' + surface + '.jsonl'), actions.map(item => JSON.stringify(item)).join('\n') + '\n');
  socket.close();
  process.exit(0);
}
