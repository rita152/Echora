#!/usr/bin/env node
// Minimal asar reader used for reference inspection of the installed ChatGPT client.
// Usage: node scripts/asar_extract.mjs <asar> list | cat <inner-path> | grep <needle>
import fs from 'node:fs';

const [, , archivePath, command, ...rest] = process.argv;

function readHeader(path) {
  const fd = fs.openSync(path, 'r');
  try {
    const prefix = Buffer.alloc(16);
    fs.readSync(fd, prefix, 0, 16, 0);
    const size = prefix.readUInt32LE(12);
    const raw = Buffer.alloc(size);
    fs.readSync(fd, raw, 0, size, 16);
    const text = raw.toString('utf8');
    const end = text.lastIndexOf('}');
    if (end < 0) throw new Error('asar header does not contain a JSON object');
    const tree = JSON.parse(text.slice(0, end + 1));
    return { fd, tree, dataOffset: 16 + size };
  } catch (error) {
    fs.closeSync(fd);
    throw error;
  }
}

function walk(node, prefix, out) {
  for (const [name, value] of Object.entries(node.files ?? {})) {
    const path = `${prefix}/${name}`;
    if (value.files) walk(value, path, out);
    else {
      out.push({
        path,
        size: Number(value.size),
        offset: Number(value.offset),
        unpacked: Boolean(value.unpacked),
      });
    }
  }
}

function readFile(handle, entry) {
  if (entry.unpacked || !Number.isFinite(entry.offset)) {
    const unpacked = `${handle.packed.replace(/app\.asar$/, 'app.asar.unpacked')}${entry.path}`;
    return fs.readFileSync(unpacked);
  }
  const buffer = Buffer.alloc(entry.size);
  fs.readSync(handle.fd, buffer, 0, entry.size, handle.dataOffset + entry.offset);
  return buffer;
}

const handle = readHeader(archivePath);
handle.packed = archivePath;
const entries = [];
walk(handle.tree, '', entries);
const totalBytes = entries.reduce((sum, entry) => sum + entry.size, 0);

switch (command) {
  case 'list': {
    for (const entry of entries) console.log(`${entry.size}\t${entry.path}`);
    console.error(`${entries.length} files, ${totalBytes} bytes`);
    break;
  }
  case 'cat': {
    const entry = entries.find((candidate) => candidate.path === rest[0]);
    if (!entry) throw new Error(`missing entry: ${rest[0]}`);
    process.stdout.write(readFile(handle, entry));
    break;
  }
  case 'grep': {
    const needle = rest[0];
    const limit = Number(rest[1] ?? 20);
    let hits = 0;
    for (const entry of entries) {
      if (!/\.(js|mjs|cjs|json|html|css|ts)$/.test(entry.path)) continue;
      if (entry.size > 12 * 1024 * 1024) continue;
      const text = readFile(handle, entry).toString('utf8');
      if (text.includes(needle)) {
        console.log(`${entry.path}\t${entry.size}\t${text.split(needle).length - 1}`);
        hits += 1;
        if (hits >= limit) break;
      }
    }
    break;
  }
  case 'extract-glob': {
    const pattern = new RegExp(rest[0]);
    const destination = rest[1];
    for (const entry of entries) {
      if (!pattern.test(entry.path)) continue;
      const target = `${destination}${entry.path}`;
      fs.mkdirSync(target.slice(0, target.lastIndexOf('/')), { recursive: true });
      if (entry.unpacked || !Number.isFinite(entry.offset)) continue;
      fs.writeFileSync(target, readFile(handle, entry));
    }
    break;
  }
  default:
    throw new Error(`unknown command: ${command}`);
}

fs.closeSync(handle.fd);
