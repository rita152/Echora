// Drive the reference Pull Requests page through the acceptance sequence and
// record what each step does, so the same sequence can be replayed on the
// native page and compared item by item.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9412 node scripts/verify_pull_requests_ux_reference.mjs
import fs from 'node:fs';
import path from 'node:path';
import { byName, connect } from './pull_requests_cdp.mjs';

const output = process.argv[2] || 'artifacts/pull-requests-ux/reference';
fs.mkdirSync(output, { recursive: true });

const cdp = await connect();
await cdp.fit(1440, 900, 1);

const steps = [];

/// Runs one step, recording failures instead of aborting the sequence.
const run = async (id, description, action) => {
  try {
    await action();
  } catch (error) {
    steps.push({ id, description, error: String(error).slice(0, 300) });
    console.log(JSON.stringify({ id, description, error: String(error).slice(0, 200) }));
  }
};
const record = async (id, description, extra = {}) => {
  const state = await cdp.evaluate(`(() => {
    const visible = selector => !!document.querySelector(selector);
    const text = document.body.innerText;
    return {
      listRows: document.querySelectorAll('div[role="button"][aria-label], div[role="button"]').length,
      filterMenu: visible('[role="menu"]'),
      placeholder: text.includes('Select pull request to view'),
      summaryTab: text.includes('Summary'),
      codeTab: text.includes('Code'),
      reviewTabClose: !!document.querySelector('[aria-label^="Close"][aria-label$="tab"]'),
      fileTreeFilter: !!document.querySelector('[aria-label="Filter files"]'),
      statusMenu: text.includes('Ready for review'),
      reviewersDialog: text.includes('Search by name or GitHub username'),
      inlineComment: text.includes('Request change'),
      emptyState: text.includes("You're all caught up") || text.includes('No pull requests match this search'),
      title: (document.querySelector('[aria-label="Edit title"]') ? text.split('\\n').slice(10, 12).join(' ') : ''),
    };
  })()`);
  const file = `${String(steps.length + 1).padStart(2, '0')}-${id}`;
  await cdp.screenshot(path.join(output, `${file}.png`));
  steps.push({ id, description, ...state, ...extra });
  console.log(JSON.stringify({ id, description, state }));
};

const click = async (name, options) => {
  const hit = await cdp.locate(byName(name, options));
  await cdp.click(hit.x, hit.y);
  await cdp.sleep(500);
  return hit;
};

const clickIf = async (name, options) => {
  const hit = await cdp.locate(byName(name, options)).catch(() => null);
  if (!hit) return false;
  await cdp.click(hit.x, hit.y);
  await cdp.sleep(600);
  return true;
};

const selectRow = async index => {
  const point = await cdp.evaluate(`(() => {
    const rows = [...document.querySelectorAll('div[role="button"]')]
      .filter(node => { const r = node.getBoundingClientRect(); return r.x < 790 && r.width > 380 && r.height > 40 && r.height < 90; });
    const node = rows[${index}];
    if (!node) return null;
    const r = node.getBoundingClientRect();
    return [r.x + r.width / 2, r.y + r.height / 2];
  })()`);
  if (!point) return false;
  await cdp.click(point[0], point[1]);
  await cdp.sleep(2500);
  return cdp.evaluate(`document.body.innerText.includes('Branch')`);
};

await run('list-entry', '侧边栏 Pull requests 进入列表页', async () => {
  await clickIf('Pull requests', { tags: 'div,button,[role="button"]' });
  await cdp.sleep(2500);
  await record('list-entry', '侧边栏 Pull requests 进入列表页');
});

await run('tabs', 'All / Reviewing / Authored 标签', async () => {
  await clickIf('Reviewing', { tags: 'button' });
  await cdp.sleep(1200);
  await record('tab-reviewing', 'Reviewing 标签（空态）');
  await clickIf('Authored', { tags: 'button' });
  await cdp.sleep(1200);
  await record('tab-authored', 'Authored 标签');
  await clickIf('All', { tags: 'button' });
  await cdp.sleep(1200);
});

await run('filter', '漏斗菜单与 Status 子菜单', async () => {
  await clickIf('Filter pull requests', { tags: 'button' });
  await record('filter-menu', '漏斗菜单打开');
  const status = await cdp.locate(byName('Status', { tags: 'div,button,[role="menuitem"]' }));
  await cdp.move(status.x, status.y);
  await cdp.sleep(700);
  await record('filter-submenu', 'Status 子菜单展开');
  await cdp.key('Escape', 'Escape', 27);
  await cdp.sleep(400);
});

await run('detail', '选中 PR 进入 Summary，再切 Code', async () => {
  let selected = await selectRow(1);
  if (!selected) selected = await selectRow(0);
  await record('detail-summary', '选中 PR 后进入 Summary', { selected });
  await clickIf('Code', { tags: 'button' });
  await cdp.sleep(3500);
  await record('detail-code', 'Code 标签（diff 工具栏与文件头）');
});

await run('file-tree', 'Show / Hide file tree', async () => {
  if (!(await clickIf('Show file tree', { tags: 'button' }))) {
    await clickIf('Hide file tree', { tags: 'button' });
    await clickIf('Show file tree', { tags: 'button' });
  }
  await cdp.sleep(800);
  await record('file-tree', 'Show file tree 打开文件树');
  await clickIf('Hide file tree', { tags: 'button' });
  await cdp.sleep(600);
});

await run('diff-toolbar', 'split/unified、collapse/expand、Review options', async () => {
  await clickIf('Switch to split diff', { tags: 'button' });
  await cdp.sleep(1200);
  await record('split-diff', 'Switch to split diff');
  await clickIf('Switch to unified diff', { tags: 'button' });
  await cdp.sleep(900);
  await clickIf('Collapse all diffs', { tags: 'button' });
  await cdp.sleep(900);
  await record('collapse-all', 'Collapse all diffs');
  await clickIf('Expand all diffs', { tags: 'button' });
  await cdp.sleep(900);
  await clickIf('Review options', { tags: 'button' });
  await record('review-options', 'Review options 菜单');
  await cdp.key('Escape', 'Escape', 27);
  await cdp.sleep(400);
});

await run('review-tab', '变更统计按钮打开/关闭 Review 标签', async () => {
  await clickIf('Summary', { tags: 'button' });
  await cdp.sleep(1200);
  await clickIf('Review pull request changes', { tags: 'button' });
  await cdp.sleep(3000);
  await record('review-tab', '变更统计按钮打开 Review 标签');
  const close = await cdp.locate(byName('Close', { exact: false, tags: 'button' })).catch(() => null);
  if (close) {
    await cdp.click(close.x, close.y);
    await cdp.sleep(1200);
    await record('review-tab-closed', 'Close 关闭 Review 标签');
  }
});

await run('detail-controls', '标题编辑、评审请求、状态与描述菜单', async () => {
  await clickIf('Summary', { tags: 'button' });
  await cdp.sleep(900);
  await clickIf('Edit title', { tags: 'button' });
  await record('edit-title', 'Edit title 进入编辑态');
  await cdp.key('Escape', 'Escape', 27);
  await cdp.sleep(500);
  await clickIf('Request reviewers', { tags: 'button' });
  await cdp.sleep(700);
  await record('reviewers-dialog', 'Request reviewers 对话框');
  await cdp.key('Escape', 'Escape', 27);
  await cdp.sleep(500);
  await clickIf('Change pull request status', { tags: 'button' });
  await record('status-menu', 'Change pull request status 菜单');
  await cdp.key('Escape', 'Escape', 27);
  await cdp.sleep(400);
  await clickIf('Description actions', { tags: 'button' });
  await record('description-menu', 'Description actions 菜单');
  await cdp.key('Escape', 'Escape', 27);
  await cdp.sleep(400);
});

await run('line-hover', 'diff 行 hover 的 + 按钮', async () => {
  await clickIf('Code', { tags: 'button' });
  await cdp.sleep(3000);
  const line = await cdp.evaluate(`(() => {
    const nodes = [...document.querySelectorAll('div')].filter(node => {
      const r = node.getBoundingClientRect();
      return r.x > 850 && r.width > 300 && r.height > 18 && r.height < 30 && r.y > 150 && r.y < 320;
    });
    const node = nodes[2];
    if (!node) return null;
    const r = node.getBoundingClientRect();
    return [r.x + r.width / 2, r.y + r.height / 2];
  })()`);
  if (line) {
    await cdp.move(line[0], line[1]);
    await cdp.sleep(600);
    await record('line-hover', 'diff 行 hover（出现 + 按钮）');
  }
});

fs.writeFileSync(path.join(output, 'sequence.json'), JSON.stringify(steps, null, 2) + '\n');
console.log(JSON.stringify({ output, steps: steps.length }));
cdp.socket.close();
