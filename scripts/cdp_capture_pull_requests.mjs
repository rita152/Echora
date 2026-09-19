// Capture the reference Pull Requests page (structure, computed styles, and
// screenshots in both themes) from a dedicated ChatGPT/Codex debug instance.
//
//   CAPTURE_CDP_PORT=9412 node scripts/cdp_capture_pull_requests.mjs --states=list,detail,code
//
// Never point this at a port another task owns. The page is driven through real
// CDP input events; the app theme is switched with the DOM attribute the app
// itself uses (`data-theme`), so no persisted setting changes.
import fs from 'node:fs';
import path from 'node:path';
import { argument, byName, connect, OUTLINE_EXPR, TOKENS_EXPR } from './pull_requests_cdp.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP || `http://127.0.0.1:${process.env.CAPTURE_CDP_PORT || 9412}`;
const output = argument('output', 'artifacts/pull-requests-reference');
const themes = (argument('themes', 'light,dark')).split(',');
const wanted = (argument('states', 'list')).split(',');
const width = Number(argument('width', 1440));
const height = Number(argument('height', 900));
const scale = Number(argument('scale', 1));
const activityScroll = Number(argument('activity-scroll', 2100));

fs.mkdirSync(output, { recursive: true });
const log = [];
const record = (kind, payload) => {
  const entry = { at: new Date().toISOString(), kind, ...payload };
  log.push(entry);
  console.log(JSON.stringify(entry));
};

const cdp = await connect(endpoint);
await cdp.fit(width, height, scale);

const capture = async (name, { themes: names = themes } = {}) => {
  // Park the pointer on neutral chrome so hover states never leak into a
  // comparison capture.
  await cdp.move(1430, 880);
  await cdp.sleep(250);
  for (const theme of names) {
    await cdp.theme(theme);
    // The embedded diff viewer chooses its palette from `color-scheme` on its
    // host element rather than from the app's `data-theme` attribute, so mirror
    // the app theme onto it before every capture.
    await cdp.evaluate(
      `[...document.querySelectorAll('diffs-container')].forEach(node => { node.style.colorScheme = ${JSON.stringify(theme)}; })`,
    );
    await cdp.sleep(200);
    const outline = await cdp.evaluate(OUTLINE_EXPR);
    fs.writeFileSync(`${output}/${name}-${theme}.json`, JSON.stringify(outline, null, 1));
    await cdp.screenshot(`${output}/${name}-${theme}.png`);
    const pressedTab = await cdp.evaluate(
      `(() => { const b = [...document.querySelectorAll('button')].find(b => b.getAttribute('aria-pressed') === 'true' && ['All','Reviewing','Authored'].includes((b.textContent||'').trim())); return b ? b.textContent.trim() : null; })()`,
    );
    record('capture', { name, theme, nodes: outline.nodes.length, pressedTab });
  }
};

const openPage = async () => {
  const already = await cdp.evaluate(`!!document.querySelector('[aria-label="Pull request view"]')`);
  if (already) return;
  const nav = await cdp.locate(byName('Pull requests', { tags: 'div,button,[role="button"]' }));
  await cdp.click(nav.x, nav.y);
  await cdp.sleep(2500);
  for (let attempt = 0; attempt < 20; attempt++) {
    const text = await cdp.evaluate('document.body.innerText');
    if (!/Checking GitHub access|Loading pull request details/.test(text)) return;
    await cdp.sleep(1000);
  }
  throw Error('pull requests page did not settle');
};

const paneOpenFor = async title => {
  return await cdp.evaluate(`(() => {
    const text = document.body.innerText;
    if (text.includes('Select pull request to view')) return false;
    if (!text.includes('Summary')) return false;
    const wanted = ${JSON.stringify(title.slice(0, 40))};
    return wanted.length === 0 || text.includes(wanted);
  })()`);
};

/// Selects the row whose title is `title`. Selection is verified against the
/// detail pane instead of the row background, which looks the same when a row
/// is merely hovered.
const selectRow = async title => {
  const hit = await cdp.evaluate(rowSelectorExpr(1));
  if (!hit) return false;
  if (await paneOpenFor(title)) return true;
  await cdp.click(hit[0], hit[1]);
  for (let attempt = 0; attempt < 25; attempt++) {
    await cdp.sleep(400);
    const text = await cdp.evaluate('document.body.innerText');
    if (/Loading pull request/.test(text)) continue;
    if (await paneOpenFor(title)) return true;
  }
  return false;
};

const click = async (name, options) => {
  const hit = await cdp.locate(byName(name, options));
  await cdp.click(hit.x, hit.y);
  return hit;
};

const hover = async (name, options) => {
  const hit = await cdp.locate(byName(name, options));
  await cdp.move(hit.x, hit.y);
  await cdp.sleep(350);
  return hit;
};

const pressEscape = () => cdp.key('Escape', 'Escape', 27);

/// Brings the page back to its default state: All tab, empty search, Summary
/// tab, nothing selected, no open popover.
const reset = async () => {
  await openPage();
  await pressEscape();
  await cdp.sleep(250);
  // The change-stats button opens a Review tab that stays open across states,
  // which hides the Summary tab. Close it before anything else.
  for (let attempt = 0; attempt < 3; attempt++) {
    const close = await cdp
      .locate(`() => [...document.querySelectorAll('button[aria-label]')]
        .filter(button => /^Close .* tab$/.test(button.getAttribute('aria-label') || ''))
        .map(button => {
          const rect = button.getBoundingClientRect();
          return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, text: button.getAttribute('aria-label'), width: rect.width, height: rect.height };
        })`)
      .catch(() => null);
    if (!close) break;
    await cdp.click(close.x, close.y);
    await cdp.sleep(700);
  }
  // A status filter applied by an earlier state would otherwise leak into every
  // later capture. The page itself opens on `Open`, and `Open` is the item that
  // carries the checked icon, so restore that one.
  const funnel = await cdp.locate(byName('Filter pull requests', { tags: 'button' })).catch(() => null);
  if (funnel) {
    await cdp.click(funnel.x, funnel.y);
    await cdp.sleep(600);
    const checked = await cdp.evaluate(`(() => {
      const items = [...document.querySelectorAll('[role="menuitem"]')].filter(item => ['All states', 'Open', 'Merged', 'Closed'].includes((item.textContent || '').trim()));
      const active = items.find(item => item.querySelector('svg'));
      return active ? active.textContent.trim() : null;
    })()`);
    if (checked !== 'Open') {
      const status = await cdp.locate(byName('Status', { tags: 'div,button,[role="menuitem"]' })).catch(() => null);
      if (status) {
        await cdp.move(status.x, status.y);
        await cdp.sleep(500);
        const openItem = await cdp.locate(byName('Open', { tags: '[role="menuitem"],button,div' })).catch(() => null);
        if (openItem) {
          await cdp.click(openItem.x, openItem.y);
          await cdp.sleep(2500);
        }
      }
    }
    await pressEscape();
    await cdp.sleep(400);
  }
  const clear = await cdp.locate(byName('Clear search', { tags: 'button' })).catch(() => null);
  if (clear) {
    await cdp.click(clear.x, clear.y);
    await cdp.sleep(500);
  }
  // The tab row is a three-way switch; a capture must never inherit whichever
  // tab the previous state left behind, so click until `All` actually reports
  // itself as pressed.
  for (let attempt = 0; attempt < 4; attempt++) {
    const pressed = await cdp.evaluate(
      `[...document.querySelectorAll('button')].some(b => (b.textContent||'').trim() === 'All' && b.getAttribute('aria-pressed') === 'true')`,
    );
    if (pressed) break;
    const all = await cdp.locate(byName('All', { tags: 'button' })).catch(() => null);
    if (!all) break;
    await cdp.click(all.x, all.y);
    await cdp.sleep(1000);
  }
  const summary = await cdp.locate(byName('Summary', { tags: 'button,[role="tab"]' })).catch(() => null);
  if (summary) {
    await cdp.click(summary.x, summary.y);
    await cdp.sleep(500);
  }
  // The placeholder pane is the state every list capture starts from. A row
  // click toggles the detail pane, so click the first visible row twice: the
  // first click selects it, the second one clears the selection.
  for (let attempt = 0; attempt < 3; attempt++) {
    const placeholder = await cdp.evaluate(
      `document.body.innerText.includes('Select pull request to view')`,
    );
    if (placeholder) break;
    const row = await cdp.evaluate(`(() => {
      const rows = [...document.querySelectorAll('div[role="button"],button[role="button"]')]
        .filter(candidate => { const rect = candidate.getBoundingClientRect(); return rect.x < 790 && rect.width > 380 && rect.height > 40 && rect.height < 90; });
      const hit = rows[0];
      if (!hit) return null;
      const rect = hit.getBoundingClientRect();
      return [rect.x + rect.width / 2, rect.y + rect.height / 2];
    })()`);
    if (!row) break;
    await cdp.click(row[0], row[1]);
    await cdp.sleep(1400);
    const again = await cdp.evaluate(`(() => {
      const rows = [...document.querySelectorAll('div[role="button"],button[role="button"]')]
        .filter(candidate => { const rect = candidate.getBoundingClientRect(); return rect.x < 790 && rect.width > 380 && rect.height > 40 && rect.height < 90; });
      const hit = rows[0];
      if (!hit) return null;
      const rect = hit.getBoundingClientRect();
      return [rect.x + rect.width / 2, rect.y + rect.height / 2];
    })()`);
    if (!again) break;
    await cdp.click(again[0], again[1]);
    await cdp.sleep(900);
  }
};

/// The list rows are `div[role="button"]` whose text is the pull request title.
const rowSelectorExpr = (index = 1) => `(() => {
  const rows = [...document.querySelectorAll('div[role="button"],button[role="button"],button[aria-label]')]
    .filter(row => {
      const rect = row.getBoundingClientRect();
      return rect.x < 790 && rect.width > 380 && rect.height > 40 && rect.height < 90;
    });
  const row = rows[Math.min(${index}, rows.length - 1)];
  if (!row) return null;
  const rect = row.getBoundingClientRect();
  return [rect.x + rect.width / 2, rect.y + rect.height / 2, (row.getAttribute('aria-label') || row.textContent || '').trim().slice(0, 120)];
})()`;

const firstPrTitle = () => cdp.evaluate(`(() => { const hit = ${rowSelectorExpr(1)}; return hit ? hit[2] : null; })()`);


/// Opens the first visible row and waits for its detail pane. Repeats because a
/// row click can race the list re-render right after `reset`.
const openDetail = async () => {
  for (let attempt = 0; attempt < 4; attempt++) {
    const title = await firstPrTitle();
    if (title && (await selectRow(title))) return title;
    await cdp.sleep(900);
  }
  throw Error('could not open a pull request detail');
};

const states = {
  list: async () => {
    // `reset` clears the search box, returns to the All tab, deselects the row
    // (so the placeholder pane shows), and closes any open popover.
    await reset();
    await capture('list-default');
    record('state', { name: 'list', placeholder: await cdp.evaluate(`document.body.innerText.includes('Select pull request to view')`) });
  },

  list_tabs: async () => {
    await openPage();
    for (const [tab, name] of [['Reviewing', 'list-reviewing'], ['Authored', 'list-authored']]) {
      await click(tab, { tags: 'button' });
      await cdp.sleep(1500);
      await capture(name);
    }
    await click('All', { tags: 'button' });
    await cdp.sleep(1200);
  },

  list_search: async () => {
    await reset();
    const field = await cdp.evaluate(`(() => {
      const input = document.querySelector('input');
      if (!input) return null;
      input.focus();
      const rect = input.getBoundingClientRect();
      return [rect.x + rect.width / 2, rect.y + rect.height / 2];
    })()`);
    await cdp.click(field[0], field[1]);
    await cdp.type('workspace');
    await cdp.sleep(1200);
    await capture('list-search');
    await cdp.type('zzzzzz');
    await cdp.sleep(1200);
    await capture('list-search-empty');
    const clear = await cdp.locate(byName('Clear search', { tags: 'button' })).catch(() => null);
    if (clear) {
      await cdp.click(clear.x, clear.y);
      await cdp.sleep(900);
      await capture('list-search-cleared');
    }
  },

  list_filter: async () => {
    await reset();
    await click('Filter pull requests', { tags: 'button' });
    await cdp.sleep(600);
    await capture('list-filter-menu');
    await hover('Status', { tags: 'div,button,[role="menuitem"]' });
    await cdp.sleep(600);
    await capture('list-filter-status');
    await hover('Repository', { tags: 'div,button,[role="menuitem"]' });
    await cdp.sleep(600);
    await capture('list-filter-repository');
    await hover('Status', { tags: 'div,button,[role="menuitem"]' });
    await cdp.sleep(500);
    const submenu = await cdp.locate(byName('Open', { tags: '[role="menuitem"],button,div' })).catch(() => null);
    record('probe', { name: 'status-submenu-open-item', found: !!submenu });
    if (submenu) {
      await cdp.click(submenu.x, submenu.y);
      await cdp.sleep(600);
      await capture('list-filter-applied-loading');
      await cdp.sleep(2500);
      await capture('list-filter-applied');
    }
    await pressEscape();
    await cdp.sleep(400);
  },

  list_groups: async () => {
    await reset();
    // The grouping header shares its name with the `Authored` tab, so match the
    // header by position: it lives in the list column, below the tab row.
    const header = () =>
      cdp.locate(`() => [...document.querySelectorAll('div[role="button"],button,[role="button"]')]
        .filter(element => {
          const text = (element.textContent || '').trim();
          const rect = element.getBoundingClientRect();
          return (text === 'Authored' || text === 'Previously reviewed') && rect.x < 790 && rect.y > 100 && rect.height < 60 && rect.width > 60;
        })
        .map(element => {
          const rect = element.getBoundingClientRect();
          return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, text: (element.textContent || '').trim().slice(0, 40), width: rect.width, height: rect.height };
        })`);
    const collapsed = await header();
    await cdp.click(collapsed.x, collapsed.y);
    await cdp.sleep(800);
    await capture('list-group-collapsed');
    const expanded = await header();
    await cdp.click(expanded.x, expanded.y);
    await cdp.sleep(800);
  },

  detail: async () => {
    await reset();
    const title = await openDetail();
    const loaded = true;
    // The toolbar tabs are sticky across runs: always return to Summary so the
    // capture is the summary surface regardless of the previous state.
    const summary = await cdp.locate(byName('Summary', { tags: 'button' })).catch(() => null);
    if (summary) {
      await cdp.click(summary.x, summary.y);
      await cdp.sleep(1200);
    }
    record('state', { name: 'detail', title, loaded });
    await capture('detail-summary');
  },

  detail_title: async () => {
    await reset();
    await openDetail();
    const edit = await cdp.locate(byName('Edit title', { tags: 'button' }));
    await cdp.click(edit.x, edit.y);
    await cdp.sleep(600);
    await capture('detail-title-edit');
    await pressEscape();
    await cdp.sleep(400);
  },

  detail_status: async () => {
    await reset();
    await openDetail();
    await click('Change pull request status', { tags: 'button' });
    await cdp.sleep(600);
    await capture('detail-status-menu');
    await pressEscape();
    await cdp.sleep(300);
  },

  detail_description: async () => {
    await reset();
    await openDetail();
    await click('Description actions', { tags: 'button' });
    await cdp.sleep(600);
    await capture('detail-description-menu');
    await pressEscape();
    await cdp.sleep(300);
    const description = await cdp.locate(byName('Description', { exact: false, tags: 'button,div' })).catch(() => null);
    if (description) {
      await cdp.click(description.x, description.y);
      await cdp.sleep(700);
      await capture('detail-description-collapsed');
      await cdp.click(description.x, description.y);
      await cdp.sleep(600);
    }
  },

  detail_reviewers: async () => {
    await reset();
    await openDetail();
    await click('Request reviewers', { tags: 'button' });
    await cdp.sleep(900);
    await capture('detail-reviewers-dialog');
    await cdp.type('zzzz');
    await cdp.sleep(1200);
    await capture('detail-reviewers-no-match');
    await pressEscape();
    await cdp.sleep(400);
  },

  detail_sections: async () => {
    await reset();
    await openDetail();
    for (const name of ['Checks', 'Activity', 'commits']) {
      const hit = await cdp.locate(byName(name, { exact: false, tags: 'button,div' })).catch(() => null);
      if (!hit) continue;
      await cdp.click(hit.x, hit.y);
      await cdp.sleep(900);
      await capture(`detail-section-${name}`);
      await cdp.click(hit.x, hit.y);
      await cdp.sleep(700);
    }
  },

  detail_comment: async () => {
    await reset();
    await openDetail();
    const actions = await cdp.locate(byName('Comment actions', { tags: 'button' })).catch(() => null);
    if (!actions) {
      record('skip', { name: 'detail_comment', reason: 'no comment on this PR' });
      return;
    }
    await cdp.click(actions.x, actions.y);
    await cdp.sleep(600);
    await capture('detail-comment-actions');
    await pressEscape();
    await cdp.sleep(300);
  },

  activity: async () => {
    await reset();
    // Only merged pull requests carry review comments in this repository.
    await click('Filter pull requests', { tags: 'button' });
    await cdp.sleep(600);
    await hover('Status', { tags: 'div,button,[role="menuitem"]' });
    await cdp.sleep(500);
    const merged = await cdp.locate(byName('Merged', { tags: '[role="menuitem"],button,div' })).catch(() => null);
    if (!merged) {
      record('skip', { name: 'activity', reason: 'Merged filter item missing' });
      return;
    }
    await cdp.click(merged.x, merged.y);
    await cdp.sleep(2500);
    // Both ends must land on the same pull request, so select by title rather
    // than by row index: the two clients order the merged list differently.
    const wanted = argument('activity-title', 'Reconcile the app-server integration table');
    const row = await cdp.evaluate(`(() => {
      const rows = [...document.querySelectorAll('div[role="button"],button[role="button"]')]
        .filter(candidate => { const rect = candidate.getBoundingClientRect(); return rect.x < 790 && rect.width > 380 && rect.height > 40 && rect.height < 90; });
      const hit = rows.find(candidate => ((candidate.getAttribute('aria-label') || candidate.textContent || '').includes(${JSON.stringify(wanted)})));
      if (!hit) return null;
      const rect = hit.getBoundingClientRect();
      return [rect.x + rect.width / 2, rect.y + rect.height / 2];
    })()`);
    if (!row) {
      record('skip', { name: 'activity', reason: `no row matching ${wanted}` });
      return;
    }
    await cdp.click(row[0], row[1]);
    await cdp.sleep(4000);
    // Bring the comment body to the same offset the aligned reference uses.
    await cdp.evaluate(`(() => {
      const scroller = [...document.querySelectorAll('*')].find(node => node.scrollHeight > node.clientHeight + 200 && node.clientHeight > 400 && node.getBoundingClientRect().x > 790);
      if (scroller) scroller.scrollTop = ${activityScroll};
    })()`);
    await cdp.sleep(600);
    record('probe', {
      name: 'activity-cards',
      cards: await cdp.evaluate(`(() => {
        const out = [];
        const walk = (root) => {
          for (const el of root.querySelectorAll('div')) {
            const rect = el.getBoundingClientRect();
            const style = getComputedStyle(el);
            if (rect.x > 800 && rect.width > 400 && rect.height > 20 && rect.height < 900 && parseFloat(style.borderTopWidth) > 0) {
              out.push({ y: +rect.y.toFixed(1), h: +rect.height.toFixed(1), border: style.borderTopWidth, text: (el.innerText || '').slice(0, 70).split('\\n').join(' | ') });
            }
            if (el.shadowRoot) walk(el.shadowRoot);
          }
        };
        walk(document);
        return out.slice(0, 10);
      })()`),
    });
    await capture('detail-activity');
  },

  code: async () => {
    await reset();
    await openDetail();
    // The diff toolbar toggles the tree both ways; make sure it starts closed.
    const hide = await cdp.locate(byName('Hide file tree', { tags: 'button' })).catch(() => null);
    if (hide) {
      await cdp.click(hide.x, hide.y);
      await cdp.sleep(700);
    }
    await click('Code', { tags: 'button' });
    for (let attempt = 0; attempt < 30; attempt++) {
      await cdp.sleep(1000);
      const text = await cdp.evaluate('document.body.innerText');
      if (!/Loading pull request changes/.test(text) && /unmodified lines/.test(text)) break;
    }
    // The tree toggle is sticky across runs; a code capture must not inherit it.
    for (let attempt = 0; attempt < 3; attempt++) {
      const hide = await cdp.locate(byName('Hide file tree', { tags: 'button' })).catch(() => null);
      if (!hide) break;
      await cdp.click(hide.x, hide.y);
      await cdp.sleep(800);
    }
    await capture('code-diff');
  },

  code_menu: async () => {
    await states.code();
    const options = await cdp.locate(byName('Review options', { tags: 'button' })).catch(() => null);
    if (options) {
      await cdp.click(options.x, options.y);
      await cdp.sleep(600);
      await capture('code-review-options');
      await pressEscape();
      await cdp.sleep(300);
    }
  },

  code_file_tree: async () => {
    await states.code();
    // The tree toggle is sticky across runs, so normalize before opening.
    const hide = await cdp.locate(byName('Hide file tree', { tags: 'button' })).catch(() => null);
    if (hide) {
      await cdp.click(hide.x, hide.y);
      await cdp.sleep(700);
    }
    const toggle = await cdp.locate(byName('Show file tree', { tags: 'button' })).catch(() => null);
    if (toggle) {
      await cdp.click(toggle.x, toggle.y);
      await cdp.sleep(900);
      await capture('code-file-tree');
      for (let attempt = 0; attempt < 3; attempt++) {
        const hide = await cdp.locate(byName('Hide file tree', { tags: 'button' })).catch(() => null);
        if (!hide) break;
        await cdp.click(hide.x, hide.y);
        await cdp.sleep(700);
      }
    } else {
      record('skip', { name: 'code_file_tree', reason: 'file tree toggle missing' });
    }
  },

  review_tab: async () => {
    await reset();
    await openDetail();
    const stats = await cdp.locate(byName('Review pull request changes', { tags: 'button' })).catch(() => null);
    if (!stats) {
      record('skip', { name: 'review_tab', reason: 'no change stats button' });
      return;
    }
    await cdp.click(stats.x, stats.y);
    // The review tab renders the diff asynchronously; wait for its content so
    // the capture never records the spinner.
    for (let attempt = 0; attempt < 30; attempt++) {
      await cdp.sleep(1000);
      const text = await cdp.evaluate('document.body.innerText');
      if (/unmodified lines/.test(text)) break;
    }
    // The review tab renders the same diff surface, and its file-tree toggle is
    // sticky, so close it here rather than before the tab exists.
    for (let attempt = 0; attempt < 3; attempt++) {
      const hide = await cdp.locate(byName('Hide file tree', { tags: 'button' })).catch(() => null);
      if (!hide) break;
      await cdp.click(hide.x, hide.y);
      await cdp.sleep(800);
    }
    await cdp.sleep(400);
    await capture('review-tab');
  },
};

try {
  await cdp.send('Page.bringToFront').catch(() => {});
  fs.writeFileSync(`${output}/tokens.json`, JSON.stringify(await cdp.evaluate(TOKENS_EXPR), null, 1));
  for (const name of wanted) {
    if (!states[name]) {
      record('skip', { name, reason: 'unknown state' });
      continue;
    }
    try {
      await states[name]();
      record('done', { name });
    } catch (error) {
      record('error', { name, message: String(error).slice(0, 500) });
    }
  }
} finally {
  fs.writeFileSync(`${output}/capture-log.jsonl`, log.map(entry => JSON.stringify(entry)).join('\n') + '\n');
  cdp.socket.close();
}
