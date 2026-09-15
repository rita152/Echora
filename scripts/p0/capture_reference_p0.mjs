#!/usr/bin/env node
// Capture the reference client's edit/revert, manual compaction, and file
// search surfaces for one theme.
//
// The script only drives real pointer/keyboard input and reads geometry,
// computed styles, and DOM structure back out. Settings navigation and the
// theme control are the app's own surfaces; no component state is rewritten.
//
// Usage:
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9333 node scripts/p0/capture_reference_p0.mjs --theme=dark
import fs from 'node:fs';
import path from 'node:path';

import {
  clickLabel,
  clickText,
  connect,
  delay,
  describe,
  option,
  hoverNewestUserMessage,
  openThread,
  resizeWindow,
  sendPrompt,
  setTheme,
  viewport,
  STYLE_PROBES,
} from './reference_ui.mjs';

const theme = option('theme', 'dark');
const threadMatch = option('thread', 'p0 stage');
const query = option('query', 'chat_search');
const output = option('output', 'artifacts/p0-stage/reference/' + theme);
fs.mkdirSync(output, { recursive: true });

const cdp = await connect();
const record = { theme, thread: null, viewport: null, steps: {}, note: [] };
console.log('theme', theme);
await setTheme(cdp, theme);
record.viewport = await resizeWindow(cdp, 1440, 900);

const writeRecord = () => {
  fs.writeFileSync(path.join(output, 'measurements.json'), JSON.stringify(record, null, 2));
};

const shoot = async (name) => {
  await cdp.screenshot(path.join(output, name + '.png'));
};

const measure = async (expression, max = 12) =>
  describe(cdp, expression, { styles: STYLE_PROBES, max });

// --- open the disposable thread ------------------------------------------
if (!(await openThread(cdp, threadMatch))) throw new Error('disposable thread did not open');
// The reference only offers the edit action for a plain composer turn, so the
// capture adds one more minimal prompt to that disposable thread.
await sendPrompt(cdp, 'Reply with exactly: p0 stage two');
await delay(1500);
record.thread = JSON.parse(
  await cdp.evaluate(
    "(() => { const unit = document.querySelector('[data-content-search-unit-key]'); return JSON.stringify({ unit: unit ? unit.getAttribute('data-content-search-unit-key') : null }); })()",
  ),
);

// --- A: user message hover actions ---------------------------------------
const userRect = await hoverNewestUserMessage(cdp);
record.steps.hover_message_rect = userRect;
record.steps.hover = {
  message_rect: userRect,
  actions: await measure(
    "[...document.querySelectorAll('button')].filter((e) => { const r = e.getBoundingClientRect(); return /Copy message|Edit message/.test(e.getAttribute('aria-label') || ''); })",
    6,
  ),
  footer: await measure(
    "[...document.querySelectorAll('h4')].filter((e) => (e.textContent || '').includes('You said')).map((e) => e.parentElement).filter(Boolean)",
    3,
  ),
};
await shoot('edit-hover');
writeRecord();

// --- A: inline edit state -------------------------------------------------
const editButton = await cdp.evaluate(
  "(() => { const node = [...document.querySelectorAll('button')].find((e) => (e.getAttribute('aria-label') || '') === 'Edit message'); if (!node) return null; const rect = node.getBoundingClientRect(); return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]); })()",
);
if (!editButton) throw new Error('edit message action missing');
await cdp.click(...JSON.parse(editButton));
await delay(1200);
record.steps.edit_state = {
  form: await measure("[...document.querySelectorAll('form')]", 3),
  editor: await measure("[...document.querySelectorAll('form .ProseMirror, form [contenteditable]')]", 3),
  editor_text: await cdp.evaluate(
    "(() => { const editor = document.querySelector('form [contenteditable]'); return editor ? editor.innerText : null; })()",
  ),
  buttons: await measure("[...document.querySelectorAll('form button')]", 6),
};
await shoot('edit-state');
writeRecord();
// Cancel restores the message.
const cancel = await cdp.evaluate(
  "(() => { const node = [...document.querySelectorAll('form button')].find((e) => (e.getAttribute('aria-label') || e.textContent || '').trim() === 'Cancel'); if (!node) return null; const rect = node.getBoundingClientRect(); return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]); })()",
);
if (cancel) {
  await cdp.click(...JSON.parse(cancel));
  await delay(900);
}
record.steps.edit_cancelled = {
  forms_after_cancel: await cdp.evaluate("document.querySelectorAll('form').length"),
  message_present: await cdp.evaluate(
    "JSON.stringify([...document.querySelectorAll('[data-turn-key]')].filter((e) => (e.textContent || '').includes('You said')).length)",
  ),
};
writeRecord();

// --- C: file search inside the command menu ------------------------------
await clickLabel(cdp, 'Search', { settle: 1200 });
const filesEntry = await cdp.evaluate(
  "(() => { const node = [...document.querySelectorAll('[cmdk-item]')].find((e) => (e.textContent || '').includes('Search files')); if (!node) return null; const rect = node.getBoundingClientRect(); return JSON.stringify([rect.x + rect.width / 2, rect.y + rect.height / 2]); })()",
);
if (!filesEntry) throw new Error('search files action missing');
await cdp.click(...JSON.parse(filesEntry));
await delay(900);
record.steps.files_empty = {
  dialog: await measure("[...document.querySelectorAll('[cmdk-dialog]')]", 2),
  input: await measure("[...document.querySelectorAll('[cmdk-input]')]", 2),
  placeholder: await cdp.evaluate("document.querySelector('[cmdk-input]')?.placeholder ?? null"),
  rows: await measure("[...document.querySelectorAll('[cmdk-item]')]", 4),
  text: await cdp.evaluate("document.querySelector('[cmdk-dialog]')?.innerText.slice(0, 160) ?? null"),
};
await shoot('files-empty');
writeRecord();
await cdp.click(...JSON.parse(await cdp.evaluate(
  "(() => { const input = document.querySelector('[cmdk-input]'); const rect = input.getBoundingClientRect(); return JSON.stringify([rect.x + 40, rect.y + rect.height / 2]); })()",
)));
await cdp.send('Input.insertText', { text: query });
await delay(2500);
record.steps.files_results = {
  rows: await measure("[...document.querySelectorAll('[cmdk-item]')]", 8),
  row_html: await cdp.evaluate("document.querySelector('[cmdk-item]')?.outerHTML.slice(0, 1500) ?? null"),
  dialog: await measure("[...document.querySelectorAll('[cmdk-dialog]')]", 2),
  highlights: await measure(
    "[...document.querySelectorAll('[cmdk-item] span, [cmdk-item] div')].filter((e) => (e.textContent || '').trim())",
    12,
  ),
};
await shoot('files-results');
writeRecord();
await cdp.send('Input.insertText', { text: 'zzzz-no-such-file' });
await delay(2000);
record.steps.files_none = {
  text: await cdp.evaluate("document.querySelector('[cmdk-dialog]')?.innerText.slice(0, 160) ?? null"),
  dialog: await measure("[...document.querySelectorAll('[cmdk-dialog]')]", 2),
};
await shoot('files-none');
writeRecord();
await cdp.key('Escape');
await delay(700);

// --- B: manual compaction entry ------------------------------------------
const composer = await cdp.evaluate(
  "(() => { const element = document.querySelector('[data-codex-composer]'); const rect = element.getBoundingClientRect(); return JSON.stringify([Math.round(rect.x + rect.width / 2), Math.round(rect.y + rect.height / 2)]); })()",
);
await cdp.click(...JSON.parse(composer));
await delay(300);
await cdp.send('Input.insertText', { text: '/' });
await delay(1500);
record.steps.compact_menu = {
  rows: await measure(
    "[...document.querySelectorAll('[cmdk-item], [role=option], [role=menuitem]')].filter((e) => e.getBoundingClientRect().width > 100)",
    12,
  ),
  dialog_text: await cdp.evaluate(
    "(document.querySelector('[cmdk-dialog]') || document.querySelector('[role=dialog]'))?.innerText.slice(0, 200) ?? null",
  ),
};
await shoot('compact-menu');
writeRecord();

writeRecord();
console.log('captured', Object.keys(record.steps), 'into', output);
process.exit(0);
