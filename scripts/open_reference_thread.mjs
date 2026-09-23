// Opens one task in the reference app (or a new chat) so the capture scripts
// can release the thread another process needs to own.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9361 \
//     node scripts/open_reference_thread.mjs --title="94"
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9361 \
//     node scripts/open_reference_thread.mjs --new-chat
import { connect, delay, option, clickLabel, setTheme } from './p0/reference_ui.mjs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP');
const title = option('title');
const newChat = process.argv.includes('--new-chat');
const theme = option('theme', 'dark');
const cdp = await connect(endpoint);
await setTheme(cdp, theme);
await delay(400);

if (newChat) {
  await clickLabel(cdp, 'New chat');
  await delay(1200);
} else {
  if (!title) throw Error('pass --title=TITLE or --new-chat');
  const point = JSON.parse(await cdp.evaluate(`(() => {
    const row = [...document.querySelectorAll('[data-app-action-sidebar-thread-row]')].find((el) => el.getAttribute('data-app-action-sidebar-thread-title') === ${JSON.stringify(title)});
    if (!row) return null;
    const rect = row.getBoundingClientRect();
    return JSON.stringify([rect.x + 60, rect.y + rect.height / 2]);
  })()`));
  if (!point) throw new Error('no task named ' + title);
  await cdp.click(Math.round(point[0]), Math.round(point[1]));
  await delay(1500);
}

const current = await cdp.evaluate(`(() => {
  const row = [...document.querySelectorAll('[data-app-action-sidebar-thread-row]')].find((el) => el.getAttribute('data-app-action-sidebar-thread-selected') === 'true');
  return row ? row.getAttribute('data-app-action-sidebar-thread-title') : null;
})()`);
console.log('reference is now showing', JSON.stringify(current));
cdp.socket.close();
