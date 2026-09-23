// Prints the box tree of a sidebar row (navigation entry, project row, task
// row, "show more" entry) so the GPUI sidebar can be measured against the
// reference element by element.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9335 node scripts/cdp_sidebar_row_tree.mjs
import { connect, delay } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');

const QUERIES = {
  'nav row (Pull requests)': `[...document.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'Pull requests')`,
  'nav row (New chat)': `[...document.querySelectorAll('button')].find((b) => (b.textContent || '').trim() === 'New chat')`,
  'project row (GPUI)': `[...document.querySelectorAll('[data-app-action-sidebar-project-row]')].find((row) => (row.innerText || '').trim().startsWith('GPUI'))`,
  'thread row (project)': `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')][1]`,
  'thread row (recents)': `[...document.querySelectorAll('[data-app-action-sidebar-thread-row]')][6]`,
  'show more': `[...document.querySelectorAll('button, [role="button"]')].find((e) => (e.innerText || '').trim() === 'Show more')`,
  'section heading (Projects)': `document.querySelectorAll('[data-app-action-sidebar-section-toggle]')[0]`,
  'brand row': `[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') || '').startsWith('Switch mode')).parentElement`,
};

const cdp = await connect(endpoint);
await cdp.send('Emulation.setDeviceMetricsOverride', {
  width: 1440,
  height: 900,
  deviceScaleFactor: 1,
  mobile: false,
});
await delay(500);

for (const [name, query] of Object.entries(QUERIES)) {
  const tree = JSON.parse(
    await cdp.evaluate(`(() => {
      const flatten = (element) => {
        const out = [];
        for (const child of [...element.children]) {
          if (getComputedStyle(child).display === 'contents') out.push(...flatten(child));
          else out.push(child);
        }
        return out;
      };
      const describe = (element, depth) => {
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        const node = {
          tag: element.tagName.toLowerCase(),
          cls: (element.className || '').toString().slice(0, 46),
          rect: [rect.x, rect.y, rect.width, rect.height].map((v) => Math.round(v * 100) / 100),
          pad: style.padding,
          margin: style.margin,
          gap: style.gap,
          fontSize: style.fontSize,
          fontWeight: style.fontWeight,
          lineHeight: style.lineHeight,
          color: style.color,
          opacity: style.opacity,
          text: (element.innerText || '').trim().slice(0, 20).replace(/\\n/g, ' / '),
        };
        if (depth > 0) {
          const children = flatten(element);
          if (children.length) node.children = children.slice(0, 8).map((child) => describe(child, depth - 1));
        }
        return node;
      };
      const root = (() => { try { return ${query}; } catch (error) { return null; } })();
      return root ? JSON.stringify(describe(root, 3)) : null;
    })()`),
  );
  console.log('==', name);
  if (!tree) {
    console.log('   (not found)');
    continue;
  }
  const walk = (node, depth) => {
    console.log(
      '  '.repeat(depth) +
        node.rect.map((v) => Math.round(v * 100) / 100).join(',') +
        ` | ${node.tag} pad=${node.pad} m=${node.margin} gap=${node.gap} ${node.fontSize}/${node.fontWeight} lh=${node.lineHeight} op=${node.opacity} | ${JSON.stringify(node.text)} | ${node.cls}`,
    );
    for (const child of node.children ?? []) walk(child, depth + 1);
  };
  walk(tree, 0);
}
cdp.close();
