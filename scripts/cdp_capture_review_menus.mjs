// Capture the three Review-panel popups (comparison scope, review options,
// branch picker) from a dedicated ChatGPT reference instance.
//
// The script never rewrites React state or CSS: it clicks the real controls,
// waits for the popup the app itself renders, then records geometry, computed
// styles, and a screenshot of the popup region.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9455 \
//     node scripts/cdp_capture_review_menus.mjs --output=artifacts/review-menus
//
// Options:
//   --output=DIR    artifact directory (default artifacts/review-menus)
//   --theme=dark    capture one theme; omitted captures both
import fs from 'node:fs';
import {
  clickLabel,
  connect,
  delay,
  option,
  screenshot,
  setTheme,
  waitFor,
} from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = option('output', 'artifacts/review-menus');
const requestedTheme = option('theme');
fs.mkdirSync(output, { recursive: true });

const STYLE_PROBES = [
  'display', 'position', 'width', 'height', 'minHeight', 'maxHeight', 'padding', 'margin', 'gap',
  'fontFamily', 'fontSize', 'fontWeight', 'lineHeight', 'letterSpacing',
  'color', 'backgroundColor', 'borderRadius', 'borderColor', 'borderWidth', 'boxShadow',
  'opacity', 'overflow', 'flexDirection', 'alignItems', 'justifyContent', 'whiteSpace', 'backdropFilter',
];

const describe = `
  (element) => {
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    const styles = {};
    for (const name of ${JSON.stringify(STYLE_PROBES)}) styles[name] = style[name];
    return {
      tag: element.tagName.toLowerCase(),
      role: element.getAttribute('role'),
      cls: (element.className || '').toString().slice(0, 160),
      label: (element.getAttribute('aria-label') || '').trim().slice(0, 60),
      text: (element.textContent || '').trim().slice(0, 60),
      rect: [Math.round(rect.x * 100) / 100, Math.round(rect.y * 100) / 100, Math.round(rect.width * 100) / 100, Math.round(rect.height * 100) / 100],
      styles,
    };
  }
`;

const popupQuery = `
  (() => {
    const menus = [...document.querySelectorAll('[role=menu]')];
    return menus[menus.length - 1];
  })()
`;

function dumpExpression() {
  return `(() => {
    const describe = ${describe};
    const menu = ${popupQuery};
    if (!menu) return JSON.stringify({ found: false });
    const wrapper = menu.parentElement;
    const walk = (element, depth) => {
      const node = describe(element);
      if (depth < 4 && element.children.length) node.children = [...element.children].map((child) => walk(child, depth + 1));
      return node;
    };
    return JSON.stringify({
      found: true,
      wrapper: wrapper ? describe(wrapper) : null,
      menu: walk(menu, 0),
    }, null, 1);
  })()`;
}

async function openReviewTab(cdp) {
  const open = await cdp.evaluate(
    "[...document.querySelectorAll('[role=tab]')].some((tab) => (tab.getAttribute('aria-label') || tab.textContent || '').trim() === 'Review')",
  );
  if (open) return;
  await clickLabel(cdp, 'Open side panel tab', { settle: 900 });
  const target = await cdp.evaluate(`(() => {
    const id = ${JSON.stringify('codex.threadSidePanelTab.review')};
    const item = [...document.querySelectorAll('[role=menuitem]')].find((node) => (node.textContent || '').startsWith(${'`Review`'}));
    if (!item) return null;
    const rect = item.getBoundingClientRect();
    return JSON.stringify([Math.round(rect.x + rect.width / 2), Math.round(rect.y + rect.height / 2)]);
  })()`);
  if (!target) throw Error('the side panel tab menu has no Review entry');
  await cdp.click(...JSON.parse(target));
  await delay(2500);
}

async function button(selectorLabel) {
  return JSON.parse(
    await cdp.evaluate(`(() => {
  const target = [...document.querySelectorAll('aside:last-of-type button')]
    .find((node) => (node.getAttribute('aria-label') || node.textContent || '').trim() === ${JSON.stringify(selectorLabel)});
  if (!target) return 'null';
  const rect = target.getBoundingClientRect();
  return JSON.stringify([Math.round(rect.x + rect.width / 2), Math.round(rect.y + rect.height / 2)]);
})()`),
  );
}

const cdp = await connect(endpoint);
await waitFor(cdp, '!!document.querySelector(\'[data-codex-composer]\')', { timeoutMs: 60000 });
await openReviewTab(cdp);

const themes = requestedTheme ? [requestedTheme] : ['dark', 'light'];
const report = { endpoint, capturedAt: new Date().toISOString(), themes: {} };

// The scope button is labelled with the active comparison source; the branch
// picker with the current base ref. Both are read from the app itself.
async function scopeLabel() {
  return cdp.evaluate(`(() => {
    const labels = ['This branch', 'Branch', 'Last turn', 'Last Turn', 'Uncommitted', 'Unstaged', 'Staged', 'Committed'];
    const target = [...document.querySelectorAll('aside:last-of-type button')]
      .find((node) => labels.includes((node.textContent || '').trim()));
    return target ? (target.textContent || '').trim() : null;
  })()`);
}

/// The branch trigger is the second-row control that sits directly under the
/// comparison dropdown; locate it by its own geometry instead of its label,
/// because the label is the current base ref.
async function branchTrigger() {
  return JSON.parse(
    await cdp.evaluate(`(() => {
  const scope = [...document.querySelectorAll('aside:last-of-type button')]
    .find((node) => /^(This branch|Branch|Last turn|Last Turn|Uncommitted|Unstaged|Staged|Committed)$/.test((node.textContent || '').trim()));
  if (!scope) return 'null';
  const anchor = scope.getBoundingClientRect();
  const target = [...document.querySelectorAll('aside:last-of-type button')]
    .filter((node) => {
      const rect = node.getBoundingClientRect();
      return rect.y > anchor.y + 20 && rect.y < anchor.y + 60 && Math.abs(rect.x - anchor.x) < 8 && rect.width > 40;
    })
    .sort((left, right) => left.getBoundingClientRect().y - right.getBoundingClientRect().y)[0];
  if (!target) return 'null';
  const rect = target.getBoundingClientRect();
  return JSON.stringify([Math.round(rect.x + rect.width / 2), Math.round(rect.y + rect.height / 2)]);
})()`),
  );
}

async function capture(cdp, name, clickPoint, theme, directory) {
  await cdp.click(...clickPoint);
  await delay(900);
  const dump = await cdp.evaluate(dumpExpression());
  const data = JSON.parse(dump);
  if (!data.found) throw Error(`no popup appeared for ${name}`);
  fs.writeFileSync(`${directory}/${name}.json`, JSON.stringify({ theme, point: clickPoint, ...data }, null, 1));
  const rect = data.menu.rect;
  const clip = {
    x: Math.max(0, rect[0] - 24),
    y: Math.max(0, rect[1] - 24),
    width: rect[2] + 48,
    height: rect[3] + 48,
  };
  await screenshot(cdp, `${directory}/${name}-crop.png`, { clip });
  await screenshot(cdp, `${directory}/${name}-window.png`);
  await cdp.key('Escape');
  await delay(400);
  return data;
}

for (const theme of themes) {
  const directory = `${output}/${theme}`;
  fs.mkdirSync(directory, { recursive: true });
  if (!(await setTheme(cdp, theme))) throw Error(`could not switch the reference instance to ${theme}`);
  await delay(1200);

  const entries = {};

  const scope = await button(await scopeLabel());
  if (!scope) throw Error('review scope dropdown not found');
  entries.scope = await capture(cdp, 'scope-menu', scope, theme, directory);

  const options = await button('Review options');
  if (!options) throw Error('review options button not found');
  entries.options = await capture(cdp, 'options-menu', options, theme, directory);

  // The branch picker is only reachable while the comparison source is a
  // branch, so switch the scope through the app's own menu first.
  await cdp.click(...(await button(await scopeLabel())));
  await delay(800);
  const branchItem = await cdp.evaluate(`(() => {
    const menu = ${popupQuery};
    const item = [...menu.children].find((node) => /^(Branch|This branch)$/.test((node.textContent || '').trim()));
    if (!item) return null;
    const rect = item.getBoundingClientRect();
    return JSON.stringify([Math.round(rect.x + rect.width / 2), Math.round(rect.y + rect.height / 2)]);
  })()`);
  if (!branchItem) throw Error('branch entry missing from the comparison menu');
  await cdp.click(...JSON.parse(branchItem));
  await delay(2500);

  const branchControl = await branchTrigger();
  if (!branchControl) throw Error('review branch picker trigger not found');
  entries.branch = await capture(cdp, 'branch-picker', branchControl, theme, directory);

  report.themes[theme] = Object.fromEntries(
    Object.entries(entries).map(([key, value]) => [key, { rect: value.menu.rect, styles: value.menu.styles, items: (value.menu.children || []).length }]),
  );
}

fs.writeFileSync(`${output}/summary.json`, JSON.stringify(report, null, 1));
console.log(JSON.stringify({ output, themes: Object.keys(report.themes) }));
process.exit(0);
