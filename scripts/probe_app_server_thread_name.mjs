// Asks the app-server itself whether it truncates a long `thread/name/set`
// title: the answer decides whether the truncation seen in the sidebar comes
// from the reference UI or from the protocol.
//
//   node scripts/probe_app_server_thread_name.mjs <threadId>
import { spawn } from 'node:child_process';
import readline from 'node:readline';

const threadId = process.argv[2];
if (!threadId) throw Error('pass the thread id');

const child = spawn('codex', ['app-server', '--stdio'], { stdio: ['pipe', 'pipe', 'pipe'] });
const rl = readline.createInterface({ input: child.stdout });
const pending = new Map();
let nextId = 0;

rl.on('line', (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  if (message.id !== undefined && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});

function request(method, params) {
  const id = ++nextId;
  return new Promise((resolve, reject) => {
    pending.set(id, (message) => (message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result)));
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    setTimeout(() => reject(new Error(`timeout for ${method}`)), 30000);
  });
}

await request('initialize', {
  clientInfo: { name: 'thread-name-probe', version: '0.0.0' },
  capabilities: { experimentalApi: true },
});
child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'initialized', params: {} })}\n`);

const long = 'y'.repeat(200);
const set = await request('thread/name/set', { threadId, name: long });
console.log('set result:', JSON.stringify(set).slice(0, 300));

const read = await request('thread/read', { threadId, includeTurns: false });
const name = read?.thread?.name ?? read?.thread?.threadName ?? null;
console.log('server name length:', name ? name.length : null, 'tail:', name ? JSON.stringify(name.slice(-6)) : null);

const restore = await request('thread/name/set', { threadId, name: '94' });
console.log('restore result:', JSON.stringify(restore).slice(0, 200));

child.kill();
