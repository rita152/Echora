// Pin the dedicated reference instance to the legacy sidebar Echora recreates.
// Both reference launchers (`scripts/launch_chatgpt_reference.sh` and
// `scripts/p0/launch_reference_instance.sh`) run it with `--layout=legacy
// --wait=…` after every launch; run it again after a reload of the reference,
// before any capture. `renderedLayout` reads `unknown` until the sidebar is
// mounted (or while it is collapsed), and `--layout=legacy` fails until it
// reads `legacy`.
//
// ChatGPT 26.924 chooses between the legacy sidebar and the navigation rail
// (icon rail plus an inset sidebar panel) with the Statsig gate `3085093835`.
// The gate is evaluated over the network when an instance starts, so any
// launch can land on the rail, the user's own app included. Capturing styles
// from the wrong layout gives wrong answers: the rail's sidebar panel stacks
// `surface` at 65% over the shell, while the legacy sidebar keeps a single
// `surface-tertiary` at 70%.
//
// The override only wraps the page's in-memory Statsig client and re-runs the
// app's own gate subscription; nothing is written to the profile or to the
// shared `~/.codex` state. Restarting the instance (or `--layout=network`)
// returns it to the network value.
//
// Usage:
//   CHATGPT_CDP_HTTP=http://127.0.0.1:9335 node scripts/cdp_pin_reference_layout.mjs --layout=legacy
//   CHATGPT_CDP_HTTP=… node scripts/cdp_pin_reference_layout.mjs --layout=rail
//   CHATGPT_CDP_HTTP=… node scripts/cdp_pin_reference_layout.mjs --layout=network
//   CHATGPT_CDP_HTTP=… node scripts/cdp_pin_reference_layout.mjs            (report only)
//
// `--wait=SECONDS` keeps retrying while the window, its Statsig client or the
// requested layout is not there yet (as right after a launch), and fails once
// the time is up.
const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');

const NAVIGATION_RAIL_GATE = '3085093835';
const option = name => {
  const argument = process.argv.find(value => value.startsWith(`--${name}=`));
  return argument ? argument.slice(name.length + 3) : null;
};
const layout = option('layout');
if (layout != null && !['legacy', 'rail', 'network'].includes(layout)) {
  throw Error(`unknown --layout=${layout}; use legacy, rail, or network`);
}
const waitSeconds = Number(option('wait') ?? 0);
if (!Number.isFinite(waitSeconds) || waitSeconds < 0) throw Error('--wait takes a number of seconds');

const expression = `(async () => {
  const gate = ${JSON.stringify(NAVIGATION_RAIL_GATE)};
  const layout = ${JSON.stringify(layout)};
  const client = window.__STATSIG__ && window.__STATSIG__.firstInstance;
  if (!client) return { error: 'no Statsig client on the page' };
  if (!client.__referenceLayoutOriginal) client.__referenceLayoutOriginal = client.getFeatureGate.bind(client);
  const original = client.__referenceLayoutOriginal;
  if (layout === 'network') {
    client.getFeatureGate = original;
    delete client.__referenceLayoutPin;
  } else if (layout != null) {
    client.__referenceLayoutPin = layout === 'rail';
    client.getFeatureGate = (name, options) => {
      const evaluation = original(name, options);
      if (name !== gate || client.__referenceLayoutPin === undefined) return evaluation;
      return { ...evaluation, value: client.__referenceLayoutPin, details: { ...evaluation.details, reason: 'LocalOverride' } };
    };
  }
  if (layout != null) {
    // The app re-reads every mounted gate on this event.
    client.$emt({ name: 'values_updated', status: 'Ready', values: null });
    await new Promise(resolve => setTimeout(resolve, 1500));
  }
  const network = original(gate, { disableExposureLog: true });
  // Only a mounted legacy sidebar counts as legacy: a page that has not
  // rendered its shell yet has no rail either.
  const renderedLayout = document.querySelector('nav[aria-label="App navigation"]') ? 'rail'
    : document.querySelector('nav[aria-label="Chat history"]') ? 'legacy' : 'unknown';
  return {
    networkValue: network.value,
    networkReason: network.details && network.details.reason,
    effectiveValue: client.getFeatureGate(gate, { disableExposureLog: true }).value,
    renderedLayout,
    visibility: document.visibilityState,
  };
})()`;

// One connection, one evaluation. Resolves to the page's report, or to
// `{ error }` when the instance is not ready to be pinned yet.
async function attempt() {
  const list = await (await fetch(endpoint + '/json/list')).json();
  const page = list.find(target => target.url === 'app://-/index.html');
  if (!page) return { error: 'ChatGPT main window not found' };
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let id = 0;
  const pending = new Map();
  socket.onmessage = event => {
    const message = JSON.parse(event.data);
    const handler = pending.get(message.id);
    if (!handler) return;
    pending.delete(message.id);
    message.error ? handler.reject(Error(message.error.message)) : handler.resolve(message.result);
  };
  const request = ++id;
  // A covered or minimized window still answers Runtime.evaluate, but give up
  // rather than hang if the renderer stops responding.
  const result = await Promise.race([
    new Promise((resolve, reject) => {
      pending.set(request, { resolve, reject });
      socket.send(JSON.stringify({ id: request, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }));
    }),
    new Promise(resolve => setTimeout(() => resolve({ timedOut: true }), 20000)),
  ]).finally(() => socket.close());
  if (result.timedOut) return { error: 'reference instance did not respond' };
  if (result.exceptionDetails) return { error: JSON.stringify(result.exceptionDetails) };
  return result.result.value;
}

const settled = state => !state.error && (layout !== 'legacy' && layout !== 'rail' || state.renderedLayout === layout);
const deadline = Date.now() + waitSeconds * 1000;
let state;
for (;;) {
  state = await attempt().catch(error => ({ error: error.message }));
  if (settled(state) || Date.now() >= deadline) break;
  await new Promise(resolve => setTimeout(resolve, 1000));
}
console.log(JSON.stringify(state, null, 2));
if (state.error) process.exit(2);
process.exit(settled(state) ? 0 : 1);
