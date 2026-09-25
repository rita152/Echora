// Capture the ChatGPT sidebar's layout as data: the geometry and computed
// styles of every landmark the GPUI sidebar has to reproduce, plus the window
// and sidebar screenshots those numbers describe.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9335 \
//     node scripts/cdp_capture_sidebar_layout.mjs --output=artifacts/sidebar-layout/reference
//
// Options:
//   --output=DIR  artifact directory (default artifacts/sidebar-layout/reference)
//   --window=WxH  emulated viewport (default 1440x900)
//   --dpr=N       device scale factor honoured by the screenshots (default 1)
//   --theme=NAME  capture one theme; omitted captures light then dark
//   --scale=N     screenshot scale for the crops (default 1)
import fs from 'node:fs';
import path from 'node:path';
import { byName } from './pull_requests_cdp.mjs';
import { connect as connectDriver } from './pull_requests_cdp.mjs';
import { connect, delay, option } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = option('output', 'artifacts/sidebar-layout/reference');
const [windowWidth, windowHeight] = option('window', '1440x900').split('x').map(Number);
const dpr = Number(option('dpr', '1'));
const scale = Number(option('scale', '1'));
const requestedTheme = option('theme');
fs.mkdirSync(output, { recursive: true });

const STYLE_PROBES = [
  'display', 'position', 'width', 'height', 'minWidth', 'minHeight', 'maxWidth', 'maxHeight',
  'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft', 'marginTop', 'marginRight',
  'marginBottom', 'marginLeft', 'rowGap', 'columnGap', 'fontFamily', 'fontSize', 'fontWeight',
  'lineHeight', 'letterSpacing', 'color', 'backgroundColor', 'borderRadius', 'borderWidth',
  'borderColor', 'borderTopWidth', 'borderTopColor', 'borderLeftWidth', 'borderLeftColor',
  'boxShadow', 'opacity', 'overflowY', 'flexDirection', 'alignItems', 'justifyContent',
  'whiteSpace', 'textOverflow', 'scrollbarWidth', 'scrollbarColor', 'maskImage',
];

// Every landmark is addressed exactly like the renderer does it: by the
// application's own test hooks where they exist, by role and text otherwise.
const LANDMARKS = {
  // The column's width is the user's persisted `sidebar-width` (275 by
  // default, 240 at the minimum), so it is found by class, not by size.
  sidebar: `document.querySelector('aside.app-shell-left-panel')`,
  sidebarNav: `document.querySelector('aside.app-shell-left-panel nav')`,
  sidebarScroll: `document.querySelector('[data-app-action-sidebar-scroll]')`,
  footer: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'Open profile menu').closest('.h-toolbar')`,
  footerHairline: `[...document.querySelectorAll('div')].find((e) => e.className === 'pointer-events-none absolute inset-x-0 top-0 z-10 border-t-hairline border-default')`,
  mainSurface: `document.querySelector('main[data-app-shell-main-surface]')`,
  brandButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '').startsWith('Switch mode'))`,
  searchButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'Search')`,
  activityButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'View activity')`,
  newChatRow: `[...document.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'New chat')`,
  pullRequestsRow: `[...document.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Pull requests')`,
  scheduledRow: `[...document.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Scheduled')`,
  pluginsRow: `[...document.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Plugins')`,
  projectsSection: `document.querySelectorAll('[data-app-action-sidebar-section]')[0]`,
  recentsSection: `document.querySelectorAll('[data-app-action-sidebar-section]')[1]`,
  projectsHeading: `document.querySelectorAll('[data-app-action-sidebar-section-toggle]')[0]`,
  projectsHeadingRow: `document.querySelectorAll('[data-app-action-sidebar-section-toggle]')[0].parentElement`,
  recentsHeading: `document.querySelectorAll('[data-app-action-sidebar-section-toggle]')[1]`,
  recentsHeadingRow: `document.querySelectorAll('[data-app-action-sidebar-section-toggle]')[1].parentElement`,
  projectOptionsButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'Project sidebar options')`,
  projectAddButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'Add new project')`,
  recentsOptionsButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'Chat sidebar options')`,
  recentsNewChatButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'New chat')`,
  profileButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'Open profile menu')`,
  helpButton: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '') === 'Open help menu')`,
  showAll: `[...document.querySelectorAll('button, [role="button"]')].find((e) => (e.innerText || '').trim() === 'Show more')`,
  brandRow: `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '').startsWith('Switch mode')).parentElement`,
};

/// Rows are collected as lists so the capture stays useful when the visible
/// content changes: project rows, task rows, their labels and trailing icon
/// buttons, plus the section containers that wrap them.
const LISTS = {
  projectRows: `[...document.querySelectorAll('[data-app-action-sidebar-project-row]')]`,
  threadRows: `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')]`,
  sections: `[...document.querySelectorAll('[data-app-action-sidebar-section]')]`,
  navRows: `[...document.querySelectorAll('button')].filter((b) => ['New chat', 'Pull requests', 'Scheduled', 'Plugins'].includes((b.textContent || '').trim()))`,
};

function describeExpression(expressions) {
  return `(() => {
    const styles = ${JSON.stringify(STYLE_PROBES)};
    const box = (element) => {
      if (!element) return null;
      const rect = element.getBoundingClientRect();
      const computed = getComputedStyle(element);
      const style = {};
      for (const name of styles) style[name] = computed[name];
      return {
        tag: element.tagName.toLowerCase(),
        role: element.getAttribute('role'),
        aria: element.getAttribute('aria-label'),
        text: (element.innerText || '').trim().replace(/\\n/g, ' / ').slice(0, 80),
        cls: (element.className || '').toString().slice(0, 200),
        rect: [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100),
        style,
      };
    };
    const result = {};
    ${Object.entries(expressions)
      .map(
        ([name, expression]) =>
          `{ const value = (${expression}); result[${JSON.stringify(name)}] = Array.isArray(value) ? value.map(box) : box(value); }`,
      )
      .join('\n    ')}
    return JSON.stringify(result);
  })()`;
}

/// Switches the reference app's own appearance through its settings surface.
/// `data-theme` alone is not enough: the sidebar material is composited from
/// the app's real setting, so the capture has to drive the same control a user
/// would click.
async function applyTheme(driver, theme) {
  const applied = async () => driver.evaluate(`document.documentElement.getAttribute('data-theme')`);
  if ((await applied()) === theme) return true;
  await driver.clickAt(byName('Open profile menu'));
  await driver.sleep(600);
  await driver.clickAt(byName('Settings', { exact: false, tags: 'button,[role="menuitem"],a' }));
  await driver.sleep(1400);
  await driver.clickAt(byName('Appearance', { tags: 'button,[role="button"],a,div' }));
  await driver.sleep(900);
  const label = theme === 'light' ? 'Light' : 'Dark';
  for (let attempt = 0; attempt < 6 && (await applied()) !== theme; attempt += 1) {
    try {
      await driver.clickAt(byName(label, { tags: 'button,div,label,[role="radio"]' }));
    } catch {
      // The control can still be mounting; retry after a beat.
    }
    await driver.sleep(900);
  }
  try {
    await driver.clickAt(byName('Back to app', { tags: 'button,div,a' }));
  } catch {
    await driver.key('Escape', 'Escape', 27);
  }
  await driver.sleep(900);
  return (await applied()) === theme;
}

async function capture(theme) {
  if (!(await applyTheme(driver, theme))) {
    throw new Error(`reference app did not switch to ${theme}`);
  }
  await delay(500);
  const landmarks = JSON.parse(await cdp.evaluate(describeExpression(LANDMARKS)));
  const lists = JSON.parse(await cdp.evaluate(describeExpression(LISTS)));
  const viewport = JSON.parse(
    await cdp.evaluate(
      `JSON.stringify({ width: window.innerWidth, height: window.innerHeight, dpr: window.devicePixelRatio, theme: document.documentElement.getAttribute('data-theme') })`,
    ),
  );
  const spec = { theme, viewport, landmarks, lists };
  fs.writeFileSync(path.join(output, `${theme}-spec.json`), `${JSON.stringify(spec, null, 1)}\n`);
  await cdp.screenshot(path.join(output, `${theme}-window.png`));
  // The material spans the whole column, including the titlebar strip the
  // app reserves above `nav`; crop that, not the nav element.
  const sidebar = [0, 0, (landmarks.sidebar?.rect[2] ?? 275) + 1, windowHeight];
  await cdp.screenshot(path.join(output, `${theme}-sidebar.png`), {
    clip: { x: sidebar[0], y: sidebar[1], width: sidebar[2], height: sidebar[3], scale },
  });
  console.log(
    theme,
    JSON.stringify(viewport),
    'sidebar',
    JSON.stringify(landmarks.sidebar?.rect ?? null),
    'rows',
    lists.projectRows.length + lists.threadRows.length,
  );
  return spec;
}

const cdp = await connect(endpoint);
const driver = await connectDriver(endpoint);
await cdp.send('Emulation.setDeviceMetricsOverride', {
  width: windowWidth,
  height: windowHeight,
  deviceScaleFactor: dpr,
  mobile: false,
});
await driver.fit(windowWidth, windowHeight, dpr);
await delay(800);

for (const theme of requestedTheme ? [requestedTheme] : ['light', 'dark']) {
  await capture(theme);
}
cdp.close();
driver.socket.close();
