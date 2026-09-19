// Extract the Pull Requests page icons from the reference DOM.
//
// The reference draws these as inline SVGs; the page must use the same shapes,
// so they are pulled out of a live capture and written as `assets/icons/*.svg`
// with `fill="currentColor"` so GPUI can tint them like the other icons.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9412 node scripts/extract_pull_request_icons.mjs
//
// `--groups=` selects which icon families to read (default `controls,meta`).
// The meta rows only exist on the Summary tab and the file tree only exists on
// the Code tab with the tree open, so the two families need their own runs.
import fs from 'node:fs';
import path from 'node:path';
import { argument, byName, connect } from './pull_requests_cdp.mjs';

const output = process.argv[2] || 'assets/icons';
fs.mkdirSync(output, { recursive: true });

const cdp = await connect();
const groups = new Set(argument('groups', 'controls,meta').split(','));

// The detail meta rows are `dt` elements that hold the row icon plus its label.
const wanted = ['Branch', 'Reviewers', 'Comments', 'Checks', 'Status'];

const seen = [];

// Controls that own an icon, by their accessible name.
const controls = {
  'pr-merge': 'Merge unavailable',
  'pr-edit-title': 'Edit title',
  'pr-description-actions': 'Description actions',
  'pr-open-browser': 'Open in browser',
  'pr-merge': 'Merge',
  'pr-fullscreen': 'Enter full screen',
  'pr-review-options': 'Review options',
  'pr-split-diff': 'Switch to split diff',
  'pr-collapse-diffs': 'Collapse all diffs',
  'pr-file-tree': 'Show file tree',
  'pr-copy-path': 'Copy path',
  'pr-toggle-file': 'Toggle file diff',
  'pr-open-file': 'Open file',
  'pr-comment-actions': 'Comment actions',
  'pr-request-reviewers': 'Request reviewers',
  'pr-clear-search': 'Clear search',
};

if (groups.has('controls'))
for (const [name, label] of Object.entries(controls)) {
  const svg = await cdp.evaluate(`(() => {
    const control = [...document.querySelectorAll('[aria-label]')].find(
      node => (node.getAttribute('aria-label') || '').startsWith(${JSON.stringify(label)}),
    );
    const node = control ? control.querySelector('svg') : null;
    return node ? node.outerHTML : null;
  })()`);
  if (!svg) {
    seen.push({ name, written: false });
    continue;
  }
  const normalized = svg
    .replace(/ class="[^"]*"/g, '')
    .replace(/ fill="(?!none)[^"]*"/g, ' fill="currentColor"');
  fs.writeFileSync(path.join(output, `${name}.svg`), normalized + '\n');
  seen.push({ name, written: true, bytes: normalized.length });
}

if (groups.has('meta'))
for (const label of wanted) {
  const name = `pr-meta-${label.toLowerCase()}`;
  const svg = await cdp.evaluate(`(() => {
    const row = [...document.querySelectorAll('dt')].find(node => node.textContent.trim() === ${JSON.stringify(label)});
    const node = row ? row.querySelector('svg') : null;
    return node ? node.outerHTML : null;
  })()`);
  if (!svg) {
    seen.push({ name, written: false });
    continue;
  }
  const normalized = svg
    .replace(/ class="[^"]*"/g, '')
    .replace(/ fill="(?!none)[^"]*"/g, ' fill="currentColor"');
  fs.writeFileSync(path.join(output, `${name}.svg`), normalized + '\n');
  seen.push({ name, written: true, bytes: normalized.length });
}

// The diff file tree draws a disclosure chevron on folder rows and a language
// glyph on file rows, both as inline SVGs inside the row button. The rows carry
// no accessible name of their own; they are the buttons inside the tree host.
const treeIcons = [
  ['pr-tree-chevron', '() => { const host = document.querySelector("file-tree-container"); return host ? [...host.querySelectorAll("button")].find(b => !(b.textContent || "").includes(".")) : null; }'],
  ['pr-tree-file', '() => { const host = document.querySelector("file-tree-container"); return host ? [...host.querySelectorAll("button")].find(b => (b.textContent || "").trim().endsWith(".rs")) : null; }'],
];
if (groups.has('tree'))
for (const [name, finder] of treeIcons) {
  const svg = await cdp.evaluate(`(() => {
    const row = (${finder})();
    const node = row ? row.querySelector('svg') : null;
    return node ? node.outerHTML : null;
  })()`);
  if (!svg) {
    seen.push({ name, written: false });
    continue;
  }
  const normalized = svg
    .replace(/ class="[^"]*"/g, '')
    .replace(/ fill="(?!none)[^"]*"/g, ' fill="currentColor"');
  fs.writeFileSync(path.join(output, `${name}.svg`), normalized + '\n');
  seen.push({ name, written: true, bytes: normalized.length });
}

console.log(JSON.stringify(seen, null, 1));
cdp.socket.close();
