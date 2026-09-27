// Pin the dedicated reference instance to the sidebar layout the user's own
// ChatGPT shows.
//
// ChatGPT 26.924 chooses between the legacy sidebar and the navigation rail
// (icon rail plus an inset sidebar panel) with the Statsig gate `3085093835`.
// The gate is evaluated over the network when an instance starts, so a freshly
// launched reference clone can land on the rail while the user's running app
// still shows the legacy sidebar. Capturing styles from the wrong layout gives
// wrong answers: the rail's sidebar panel stacks `surface` at 65% over the
// shell, while the legacy sidebar keeps a single `surface-tertiary` at 70%.
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
const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');

const NAVIGATION_RAIL_GATE = '3085093835';
const argument = process.argv.find(value => value.startsWith('--layout='));
const layout = argument ? argument.slice('--layout='.length) : null;
if (layout != null && !['legacy', 'rail', 'network'].includes(layout)) {
  throw Error(`unknown --layout=${layout}; use legacy, rail, or network`);
}

const list = await (await fetch(endpoint + '/json/list')).json();
const page = list.find(target => target.url === 'app://-/index.html');
if (!page) throw Error('ChatGPT main window not found');
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
const send = (method, params) => new Promise((resolve, reject) => {
  const request = ++id;
  pending.set(request, { resolve, reject });
  socket.send(JSON.stringify({ id: request, method, params: params || {} }));
});
// A covered or minimized window still answers Runtime.evaluate, but give up
// rather than hang if the renderer stops responding.
const watchdog = setTimeout(() => { console.error('reference instance did not respond'); process.exit(2); }, 20000);

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
  return {
    networkValue: network.value,
    networkReason: network.details && network.details.reason,
    effectiveValue: client.getFeatureGate(gate, { disableExposureLog: true }).value,
    renderedLayout: document.querySelector('nav[aria-label="App navigation"]') ? 'rail' : 'legacy',
    visibility: document.visibilityState,
  };
})()`;
const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
clearTimeout(watchdog);
if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
const state = result.result.value;
console.log(JSON.stringify(state, null, 2));
if (state.error) process.exit(2);
if (layout === 'legacy' && state.renderedLayout !== 'legacy') process.exit(1);
if (layout === 'rail' && state.renderedLayout !== 'rail') process.exit(1);
socket.close();
process.exit(0);
