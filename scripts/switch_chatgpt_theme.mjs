// Switch the reference app's own theme through its Appearance settings.
//
// The main window follows `data-theme`, which the capture script can set
// directly, but the embedded diff viewer reads `color-scheme` at render time,
// so a true dark diff capture needs the app itself to switch themes.
//
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9412 node scripts/switch_chatgpt_theme.mjs dark
import { byName, connect } from './pull_requests_cdp.mjs';

const wanted = (process.argv[2] || '').toLowerCase();
if (wanted !== 'light' && wanted !== 'dark') {
  throw Error('usage: switch_chatgpt_theme.mjs <light|dark>');
}

const cdp = await connect();
// The capture scripts also set `data-theme` directly, so the attribute is not
// proof of the app's own setting: always drive the Appearance control.

const profile = await cdp.locate(byName('Open profile menu', { tags: 'button' }));
await cdp.click(profile.x, profile.y);
await cdp.sleep(600);
const settings = await cdp.locate(byName('Settings', { exact: false, tags: 'button,[role="menuitem"],a' }));
await cdp.click(settings.x, settings.y);
await cdp.sleep(1200);

// The settings shell lists its sections on the left; Appearance holds the
// Light/Dark/System control.
const appearance = await cdp.locate(byName('Appearance', { tags: 'button,[role="button"],a,div' }));
await cdp.click(appearance.x, appearance.y);
await cdp.sleep(900);

// The control needs a moment to mount after the settings section opens, and a
// single click can land before the option list is interactive, so retry until
// the attribute actually changes.
let applied = await cdp.evaluate(`document.documentElement.getAttribute('data-theme')`);
for (let attempt = 0; attempt < 5 && applied !== wanted; attempt++) {
  const option = await cdp
    .locate(byName(wanted === 'dark' ? 'Dark' : 'Light', { tags: 'button,div,label,[role="radio"]' }))
    .catch(() => null);
  if (option) {
    await cdp.click(option.x, option.y);
    await cdp.sleep(1200);
  } else {
    await cdp.sleep(700);
  }
  applied = await cdp.evaluate(`document.documentElement.getAttribute('data-theme')`);
}
const back = await cdp.locate(byName('Back to app', { tags: 'button,div,a' })).catch(() => null);
if (back) {
  await cdp.click(back.x, back.y);
  await cdp.sleep(600);
}
console.log(JSON.stringify({ theme: applied, changed: true, requested: wanted, matches: applied === wanted }));
cdp.socket.close();
