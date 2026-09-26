// Capture the ChatGPT sidebar activity view (the bell in the sidebar header)
// from a dedicated debug instance: screenshots of every state the GPUI view
// reproduces, plus the geometry of each landmark as a spec the comparison
// script reads.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9347 \
//     node scripts/cdp_capture_activity_view.mjs --output=artifacts/activity-view-26917/reference
//
// Options:
//   --output=DIR   artifact directory
//   --theme=NAME   capture one theme; omitted captures dark then light
//   --window=WxH   emulated viewport (default 1470x924: the GPUI capture
//                  window rounds odd heights up on a 1x display)
//
// All interaction is real CDP pointer/keyboard input: the bell is clicked, the
// rows hovered, the options menu opened and the list scrolled with the wheel.
// Light and dark are read by setting `data-theme` on the dedicated instance's
// document (and restoring it): the app's own Appearance setting lives in the
// shared ~/.codex/config.toml, so switching it would change the user's app.
// Nothing that writes the shared state (pinning, archiving, the Show toggles,
// marking read) is ever clicked.
import fs from 'node:fs';
import path from 'node:path';
import { Cdp, delay, option } from './stage4/cdp_client.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = option('output', 'artifacts/activity-view-26917/reference');
const themes = option('theme') ? [option('theme')] : ['dark', 'light'];
const [windowWidth, windowHeight] = option('window', '1470x924').split('x').map(Number);
fs.mkdirSync(output, { recursive: true });

const cdp = await Cdp.connect(endpoint);
await cdp.send('Emulation.setDeviceMetricsOverride', {
  width: windowWidth,
  height: windowHeight,
  deviceScaleFactor: 1,
  mobile: false,
});
await delay(600);
const json = async expression => JSON.parse(await cdp.evaluate(`JSON.stringify(${expression})`));
const AWAY = [720, 480];

// Geometry of every landmark the GPUI view has to reproduce. Text rects come
// from a DOM Range over the text node, so they bound the glyph run itself.
const SPEC = `(() => {
  const rect = element => { if (!element) return null; const r = element.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(v => Math.round(v * 1000) / 1000); };
  const textRect = element => {
    if (!element) return null;
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    const range = document.createRange(); let first = null, last = null, node;
    while ((node = walker.nextNode())) { if (!node.textContent.trim()) continue; first ??= node; last = node; }
    if (!first) return null;
    range.setStart(first, 0); range.setEnd(last, last.textContent.length);
    const r = range.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(v => Math.round(v * 1000) / 1000);
  };
  const style = element => { if (!element) return null; const s = getComputedStyle(element); return { font: s.fontSize + '/' + s.lineHeight + ' ' + s.fontWeight, color: s.color, background: s.backgroundColor, opacity: s.opacity }; };
  const bell = [...document.querySelectorAll('button')].find(b => /activity/i.test(b.getAttribute('aria-label') || ''));
  const list = document.querySelector('.\\\\@container\\\\/priority-list');
  const headings = list ? [...list.querySelectorAll('.group\\\\/nav-section-title')] : [];
  const rows = list ? [...list.querySelectorAll('[role=listitem] .sidebar-item')] : [];
  const options = document.querySelector('button[aria-label="Activity view options"]');
  const empty = list ? [...list.querySelectorAll('div')].find(d => d.className.startsWith('p-2 text-start')) : null;
  const tooltip = document.querySelector('[role=tooltip]');
  const menu = [...document.querySelectorAll('[role=menu]')].find(m => m.getBoundingClientRect().width > 0);
  const scroller = document.querySelector('.vertical-scroll-fade-mask');
  const loader = list ? [...list.children].filter(c => c.getAttribute('role') === 'listitem').pop() : null;
  return {
    viewport: [innerWidth, innerHeight, devicePixelRatio],
    theme: document.documentElement.dataset.theme,
    scrollTop: scroller ? scroller.scrollTop : null,
    bell: { rect: rect(bell), label: bell && bell.getAttribute('aria-label'), glyph: rect(bell && bell.querySelector('svg')) },
    options: { rect: rect(options), glyph: rect(options && options.querySelector('svg')) },
    empty: { rect: rect(empty), text: textRect(empty), style: style(empty) },
    headings: headings.map(h => ({ text: h.innerText.trim(), rect: rect(h), title: textRect(h.firstElementChild), style: style(h.firstElementChild) })),
    rows: rows.slice(0, 12).map(row => {
      const title = row.querySelector('[class*=_content_19mhu]') || row.querySelector('.text-base');
      const detail = row.querySelector('span.line-clamp-2');
      const detailIcon = detail && detail.querySelector('svg');
      const rail = row.querySelector('div.absolute.end-0');
      return { label: row.getAttribute('aria-label'), rect: rect(row), title: textRect(title), detail: textRect(detail), detailIcon: rect(detailIcon),
        actions: rail ? [...rail.querySelectorAll('button')].map(rect) : [], hovered: row.matches(':hover'), background: getComputedStyle(row).backgroundColor };
    }),
    loader: loader && loader.querySelector('svg') ? { rect: rect(loader) } : null,
    tooltip: tooltip ? { rect: rect(tooltip), text: tooltip.innerText, label: textRect(tooltip.querySelector('.min-w-0')), kbd: rect(tooltip.querySelector('kbd')) } : null,
    menu: menu ? { rect: rect(menu), items: [...menu.querySelectorAll('[role^=menuitem], [class*=sectionLabel]')].map(item => ({ text: item.innerText.trim(), rect: rect(item), label: textRect(item), checked: item.getAttribute('aria-checked'), disabled: item.hasAttribute('data-disabled') })) } : null,
  };
})()`;

const bellState = () => json(`(() => { const b = [...document.querySelectorAll('button')].find(b => /activity/i.test(b.getAttribute('aria-label') || '')); const r = b.getBoundingClientRect(); return { label: b.getAttribute('aria-label'), x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);

async function clickBell() {
  const bell = await bellState();
  await cdp.click(bell.x, bell.y);
  await delay(900);
}

/// Opens the view fresh: a new Priority snapshot, ten visible rows, the list
/// at its top.
async function resetView() {
  let bell = await bellState();
  if (bell.label === 'Turn off activity view') await clickBell();
  await clickBell();
  bell = await bellState();
  if (bell.label !== 'Turn off activity view') throw Error('activity view did not open: ' + bell.label);
  await cdp.scroll(120, 500, -5000);
  await delay(500);
  await cdp.hover(...AWAY);
  await delay(600);
}

async function capture(name, theme, extra = {}) {
  const file = path.join(output, `${theme}-${name}.png`);
  await cdp.screenshot(file);
  const spec = await json(SPEC);
  fs.writeFileSync(path.join(output, `${theme}-${name}.json`), JSON.stringify({ ...spec, ...extra }, null, 1) + '\n');
  console.log(theme, name, 'rows', spec.rows.length, 'headings', spec.headings.map(h => h.text).join(','));
  return spec;
}

const originalTheme = await cdp.evaluate(`document.documentElement.dataset.theme`);
try {
  for (const theme of themes) {
    await cdp.evaluate(`document.documentElement.dataset.theme = ${JSON.stringify(theme)}`);
    await delay(700);
    await resetView();
    const rest = await capture('default', theme);

    // Hover the first day row before its marquee (350 ms) and card (~240 ms).
    const row = rest.rows[0];
    await cdp.hover(row.rect[0] + 60, row.rect[1] + row.rect[3] / 2);
    await delay(120);
    await capture('hover', theme);
    await cdp.hover(...AWAY);
    await delay(500);

    // The bell's tooltip while the view is open.
    await cdp.hover(rest.bell.rect[0] + 12, rest.bell.rect[1] + 12);
    await delay(700);
    await capture('tooltip', theme);
    await cdp.hover(...AWAY);
    await delay(500);

    // The options menu, then Escape.
    await cdp.click(rest.options.rect[0] + 12, rest.options.rect[1] + 12);
    await delay(700);
    await capture('options', theme);
    await cdp.key('Escape', 0, 'Escape');
    await delay(600);
    await cdp.hover(...AWAY);
    await delay(400);

    // Scrolled so the day headings stick and clip their rows. The list can
    // only scroll past ~100 px once its loader has revealed more rows, so
    // the wheel keeps going until it reaches the offset.
    for (const scroll of [100, 200]) {
      await resetView();
      for (let attempt = 0; attempt < 6; attempt += 1) {
        const top = await cdp.evaluate(`document.querySelector('.vertical-scroll-fade-mask').scrollTop`);
        if (top >= scroll) break;
        await cdp.scroll(120, 500, scroll - top);
        await delay(500);
      }
      await cdp.hover(...AWAY);
      await delay(300);
      await capture(`scroll-${scroll}`, theme);
    }
    await resetView();
  }
} finally {
  await cdp.evaluate(`document.documentElement.dataset.theme = ${JSON.stringify(originalTheme)}`);
  await cdp.send('Emulation.clearDeviceMetricsOverride');
  cdp.socket.close();
}
