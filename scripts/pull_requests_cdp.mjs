// Shared CDP helpers for capturing the reference Pull Requests page from a
// dedicated ChatGPT/Codex debug instance. Never attach to a port owned by
// another task: set CHATGPT_CDP_HTTP or pass --endpoint.
import fs from 'node:fs';
import path from 'node:path';

export function argument(name, fallback = undefined) {
  const prefix = `--${name}=`;
  const hit = process.argv.find(value => value.startsWith(prefix));
  return hit ? hit.slice(prefix.length) : fallback;
}

export const THEME_EXPR = theme => `document.documentElement.setAttribute('data-theme', ${JSON.stringify(theme)})`;

export async function connect(endpoint = process.env.CHATGPT_CDP_HTTP || argument('endpoint', 'http://127.0.0.1:9412')) {
  const targets = await (await fetch(`${endpoint}/json/list`)).json();
  const page = targets.find(t => t.url === 'app://-/index.html') || targets.find(t => t.type === 'page');
  if (!page) throw Error(`no page target at ${endpoint}`);
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let id = 0;
  const pending = new Map();
  const listeners = [];
  socket.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const entry = pending.get(message.id);
      pending.delete(message.id);
      clearTimeout(entry.timer);
      message.error ? entry.reject(Error(JSON.stringify(message.error))) : entry.resolve(message.result);
      return;
    }
    for (const listener of listeners) listener(message);
  };
  const send = (method, params = {}, timeout = 30000) => new Promise((resolve, reject) => {
    const requestId = ++id;
    const timer = setTimeout(() => { pending.delete(requestId); reject(Error(`${method} timed out`)); }, timeout);
    pending.set(requestId, { resolve, reject, timer });
    socket.send(JSON.stringify({ id: requestId, method, params }));
  });
  const evaluate = async (expression, { awaitPromise = true } = {}) => {
    const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise });
    if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails).slice(0, 2000));
    return result.result?.value;
  };
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const move = (x, y) => send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  const click = async (x, y, { button = 'left', settle = 120 } = {}) => {
    await move(x, y);
    await send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button, buttons: 1, clickCount: 1 });
    await sleep(24);
    await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button, buttons: 0, clickCount: 1 });
    await sleep(settle);
  };
  const type = async text => {
    for (const char of text) {
      await send('Input.dispatchKeyEvent', { type: 'keyDown', text: char, unmodifiedText: char });
      await send('Input.dispatchKeyEvent', { type: 'keyUp', text: char, unmodifiedText: char });
      await sleep(18);
    }
  };
  const key = async (name, code, virtualKey, modifiers = 0) => {
    const params = { key: name, code, windowsVirtualKeyCode: virtualKey, nativeVirtualKeyCode: virtualKey, modifiers };
    await send('Input.dispatchKeyEvent', { type: 'keyDown', ...params });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', ...params });
  };
  const scroll = async (x, y, deltaY) => send('Input.dispatchMouseEvent', { type: 'mouseWheel', x, y, deltaX: 0, deltaY });
  const locate = async (description, { index = 0 } = {}) => {
    const rect = await evaluate(`(() => {
      const finder = ${description};
      const hits = finder().filter(entry => entry);
      const hit = hits[${index}];
      return hit ? [hit.x, hit.y, hit.text, hit.width, hit.height] : null;
    })()`);
    if (!rect) throw Error(`locate failed: ${description}`);
    return { x: rect[0], y: rect[1], text: rect[2], width: rect[3], height: rect[4] };
  };
  const clickAt = async (description, options) => {
    const hit = await locate(description, options);
    await click(hit.x, hit.y);
    return hit;
  };
  const fit = async (width = 1440, height = 900, scale = 1) => {
    await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: scale, mobile: false });
    await sleep(450);
  };
  const theme = async mode => { await evaluate(THEME_EXPR(mode)); await sleep(500); };
  const screenshot = async (file, { clip } = {}) => {
    const options = { format: 'png', fromSurface: true, captureBeyondViewport: false };
    if (clip) options.clip = { ...clip, scale: 1 };
    const shot = await send('Page.captureScreenshot', options);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, Buffer.from(shot.data, 'base64'));
    return file;
  };
  return { endpoint, page, socket, send, evaluate, sleep, move, click, clickAt, locate, type, key, scroll, fit, theme, screenshot, on: listener => listeners.push(listener) };
}

/// Locates an element by its accessible name (aria-label or trimmed text).
export const byName = (name, { exact = true, tags = 'button,[role="button"],[role="menuitem"],[role="tab"],a,span,div' } = {}) => `
  () => [...document.querySelectorAll(${JSON.stringify(tags)})]
    .filter(element => {
      const text = (element.textContent || '').trim();
      const label = element.getAttribute('aria-label') || '';
      const hit = ${exact} ? (text === ${JSON.stringify(name)} || label === ${JSON.stringify(name)})
                           : (text.includes(${JSON.stringify(name)}) || label.includes(${JSON.stringify(name)}));
      if (!hit) return false;
      const rect = element.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1) return false;
      // Keep only the innermost matches: skip containers that hold another match.
      return ![...element.querySelectorAll(${JSON.stringify(tags)})].some(descendant => {
        const descendantText = (descendant.textContent || '').trim();
        const descendantLabel = descendant.getAttribute('aria-label') || '';
        const descendantHit = ${exact} ? (descendantText === ${JSON.stringify(name)} || descendantLabel === ${JSON.stringify(name)})
                                       : (descendantText.includes(${JSON.stringify(name)}) || descendantLabel.includes(${JSON.stringify(name)}));
        if (!descendantHit) return false;
        const descendantRect = descendant.getBoundingClientRect();
        return descendantRect.width >= 1 && descendantRect.height >= 1;
      });
    })
    .map(element => {
      const rect = element.getBoundingClientRect();
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, text: (element.getAttribute('aria-label') || element.textContent || '').trim().slice(0, 60), width: rect.width, height: rect.height };
    })`;

export const byAriaLabel = label => byName(label, { exact: true });

/// Element outline plus computed styles for the current viewport.
export const OUTLINE_EXPR = `(() => {
  const PROPERTIES = ['display','position','flexDirection','alignItems','justifyContent','gap','padding','margin','width','height','fontFamily','fontSize','fontWeight','lineHeight','letterSpacing','color','backgroundColor','backgroundImage','borderTopWidth','borderTopColor','borderTopStyle','borderRadius','boxShadow','opacity','overflow','textAlign','textOverflow','whiteSpace','cursor','zIndex','transform'];
  const nodes = [];
  const walk = (element, depth, path = '') => {
    if (depth > 30) return;
    const rect = element.getBoundingClientRect();
    const visible = rect.width >= 0.5 && rect.height >= 0.5 && rect.x + rect.width >= 276 && rect.y + rect.height >= 0;
    if (visible) {
      const styles = getComputedStyle(element);
      const className = (element.className || '').toString();
      const own = [...element.childNodes].filter(node => node.nodeType === 3).map(node => node.textContent.trim()).join(' ').slice(0, 120);
      const record = {
        depth,
        path,
        tag: element.tagName.toLowerCase(),
        className: className.slice(0, 160),
        role: element.getAttribute('role') || undefined,
        ariaLabel: element.getAttribute('aria-label') || undefined,
        placeholder: element.getAttribute('placeholder') || undefined,
        disabled: element.getAttribute('aria-disabled') || element.getAttribute('disabled') || undefined,
        text: own || undefined,
        rect: [Math.round(rect.x * 100) / 100, Math.round(rect.y * 100) / 100, Math.round(rect.width * 100) / 100, Math.round(rect.height * 100) / 100],
        style: {},
      };
      for (const property of PROPERTIES) {
        const value = styles[property];
        if (value === '' || value === 'none' || value === 'normal' || value === 'auto' || value === '0px' || value === 'rgba(0, 0, 0, 0)') continue;
        record.style[property] = value;
      }
      nodes.push(record);
    }
    [...element.children].forEach((child, index) => walk(child, depth + 1, path + '/' + index));
    if (element.shadowRoot) {
      [...element.shadowRoot.children].forEach((child, index) => walk(child, depth + 1, path + '/s' + index));
    }
  };
  walk(document.body, 0);
  return { viewport: [innerWidth, innerHeight], dpr: devicePixelRatio, theme: document.documentElement.getAttribute('data-theme'), url: location.href, nodes };
})()`;

/// All theme tokens declared in the stylesheets, in both declared themes.
export const TOKENS_EXPR = `(() => {
  const collect = selectorTarget => {
    const values = {};
    for (const sheet of document.styleSheets) {
      let rules; try { rules = sheet.cssRules; } catch { continue; }
      const walk = list => {
        for (const rule of list) {
          if (rule.cssRules) { walk(rule.cssRules); continue; }
          if (!rule.style || !rule.selectorText) continue;
          if (!rule.selectorText.includes(selectorTarget)) continue;
          for (const property of rule.style) if (property.startsWith('--')) values[property] = rule.style.getPropertyValue(property).trim();
        }
      };
      walk(rules);
    }
    return values;
  };
  return { light: collect(':root'), dark: collect('[data-theme="dark"]'), current: document.documentElement.getAttribute('data-theme') };
})()`;
