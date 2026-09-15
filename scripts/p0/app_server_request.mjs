#!/usr/bin/env node
// One-shot app-server request used by the P0 verification scripts.
//
// Usage:
//   node scripts/p0/app_server_request.mjs --method=project/create \
//     --params='{"idempotencyKey":"p0","name":"P0 Scratch","roots":[{"path":"/private/tmp/p0-scratch"}]}'
//   node scripts/p0/app_server_request.mjs --method=turn/start --wait-turn \
//     --params='{"threadId":"...","input":[{"type":"text","text":"hi"}]}'
//
// The helper starts a private codex app-server, performs the initialize
// handshake, issues exactly one request, prints the JSON response, and exits.
// It never reuses another instance's connection.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import process from 'node:process';

const option = (name, fallback = null) => {
  const prefix = '--' + name + '=';
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) ?? fallback;
};

const method = option('method');
const params = JSON.parse(option('params', '{}'));
const binary = option('codex', process.env.P0_CODEX_BIN ?? 'codex');
const waitTurn = process.argv.includes('--wait-turn');
const startThreadTurn = process.argv.includes('--start-thread-turn');
const logFile = option('log');
const turnTimeoutMs = Number(option('timeout-ms', '300000'));
if (!method) throw new Error('pass --method=');

const child = spawn(binary, ['app-server', '--stdio'], { stdio: ['pipe', 'pipe', 'inherit'] });
let buffer = '';
let stage = 'initialize';
const notifications = [];
const send = (message) => child.stdin.write(JSON.stringify(message) + '\n');

const timeout = setTimeout(() => {
  console.error('app-server request timed out');
  child.kill();
  process.exit(1);
}, 60_000);

function finish(response) {
  if (logFile) {
    fs.writeFileSync(
      logFile,
      JSON.stringify({ method, params, request: response, notifications }, null, 2),
    );
  }
  console.log(JSON.stringify(response, null, 2));
  child.kill();
  process.exit(response.error ? 1 : 0);
}

function watchTurn(turnStartedId) {
  const deadline = setTimeout(() => {
    console.error('turn did not complete in time');
    child.kill();
    process.exit(1);
  }, turnTimeoutMs);
  const handler = (message) => {
    if (message.method) notifications.push(message);
    if (message.method === 'turn/completed' && message.params?.turn?.id === turnStartedId) {
      clearTimeout(deadline);
      finish({ result: { turnId: turnStartedId, completed: true }, notifications });
    }
  };
  pendingWake = handler;
}

let pendingWake = null;
child.stdout.on('data', (chunk) => {
  buffer += chunk.toString('utf8');
  let index;
  while ((index = buffer.indexOf('\n')) !== -1) {
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    if (!line.trim()) continue;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      continue;
    }
    if (pendingWake) {
      pendingWake(message);
      continue;
    }
    if (stage === 'initialize' && message.id === '__p0_initialize__') {
      stage = 'request';
      send({ jsonrpc: '2.0', id: '__p0_request__', method, params });
      continue;
    }
    if (stage === 'request' && message.id === '__p0_request__') {
      clearTimeout(timeout);
      if (waitTurn && message.result?.turn?.id) {
        stage = 'wait-turn';
        watchTurn(message.result.turn.id);
        continue;
      }
      finish(message);
    }
  }
});

send({
  jsonrpc: '2.0',
  id: '__p0_initialize__',
  method: 'initialize',
  params: {
    clientInfo: { name: 'p0_verification', title: 'P0 verification', version: '0.0.0' },
    capabilities: { experimentalApi: true, requestAttestation: false },
  },
});
send({ jsonrpc: '2.0', method: 'initialized', params: {} });
