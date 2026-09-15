#!/usr/bin/env node
// Seed a disposable thread for the P0 verification run.
//
// One private app-server process creates the thread and runs the requested
// prompts inside it, so the thread stays persisted after the process exits.
//
// Usage:
//   node scripts/p0/app_server_seed_thread.mjs --cwd=/private/tmp/p0-scratch \
//     --prompt="Reply with exactly: p0 stage one" --prompt="Reply with exactly: p0 stage two" \
//     --log=artifacts/p0-stage/wire/setup/seed.json
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import process from 'node:process';

const option = (name, fallback = null) => {
  const prefix = '--' + name + '=';
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) ?? fallback;
};
const prompts = process.argv.filter((argument) => argument.startsWith('--prompt=')).map((argument) => argument.slice('--prompt='.length));
const cwd = option('cwd');
const logFile = option('log');
const binary = option('codex', process.env.P0_CODEX_BIN ?? 'codex');
if (!cwd || prompts.length === 0) throw new Error('pass --cwd= and at least one --prompt=');

const child = spawn(binary, ['app-server', '--stdio'], { stdio: ['pipe', 'pipe', 'inherit'] });
const send = (message) => child.stdin.write(JSON.stringify(message) + '\n');
const record = { cwd, prompts, threadId: null, turns: [], notifications: [] };

let buffer = '';
let pendingInitialize = null;
const responses = new Map();

const finish = (code) => {
  if (logFile) fs.writeFileSync(logFile, JSON.stringify(record, null, 2));
  console.log(JSON.stringify({ threadId: record.threadId, turns: record.turns }, null, 2));
  child.kill();
  process.exit(code);
};

const deadline = setTimeout(() => {
  console.error('seed timed out');
  finish(1);
}, 900_000);

let onNotification = null;
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
    if (message.method) {
      record.notifications.push(message.method);
      onNotification?.(message);
      continue;
    }
    const resolve = responses.get(message.id);
    if (resolve) {
      responses.delete(message.id);
      resolve(message);
    }
  }
});

const request = (id, method, params) =>
  new Promise((resolve, reject) => {
    responses.set(id, resolve);
    send({ jsonrpc: '2.0', id, method, params });
    setTimeout(() => {
      if (responses.has(id)) {
        responses.delete(id);
        reject(new Error('no response for ' + method));
      }
    }, 120_000);
  });

const runTurn = (params) =>
  new Promise((resolve, reject) => {
    onNotification = (message) => {
      if (message.method === 'turn/completed') {
        onNotification = null;
        resolve(message.params);
      }
      if (message.method === 'error') {
        onNotification = null;
        reject(new Error(JSON.stringify(message.params)));
      }
    };
    request('__p0_turn__', 'turn/start', params).then((response) => {
      if (response.error) {
        onNotification = null;
        reject(new Error(JSON.stringify(response.error)));
      }
    }, reject);
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

const started = await request('__p0_thread__', 'thread/start', { cwd });
if (started.error) throw new Error(JSON.stringify(started.error));
record.threadId = started.result.thread.id;

for (const prompt of prompts) {
  const completed = await runTurn({
    threadId: record.threadId,
    input: [{ type: 'text', text: prompt }],
  });
  record.turns.push({ prompt, turnId: completed?.turn?.id ?? null, status: completed?.turn?.status ?? null });
}

clearTimeout(deadline);
finish(0);

