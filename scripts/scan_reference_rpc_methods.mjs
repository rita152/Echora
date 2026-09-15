#!/usr/bin/env node

// Read-only scan of the installed ChatGPT desktop app bundle for app-server
// JSON-RPC method-name string literals. Writes a per-method match report to
// the requested output directory; never opens or drives the app itself.

import fs from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const asarPath = process.argv[2] ?? "/Applications/ChatGPT.app/Contents/Resources/app.asar";
const outputDir = process.argv[3];
const schemaRoot = process.argv[4] ?? "artifacts/app-server-schema/experimental";

if (!outputDir) {
  process.stderr.write("usage: node scripts/scan_reference_rpc_methods.mjs <asar> <output-dir> [schema-root]\n");
  process.exit(2);
}

function readAsarIndex(filePath) {
  const fd = fs.openSync(filePath, "r");
  const prefix = Buffer.alloc(16);
  fs.readSync(fd, prefix, 0, prefix.length, 0);
  const headerLength = prefix.readUInt32LE(12);
  const header = Buffer.alloc(headerLength);
  fs.readSync(fd, header, 0, header.length, 16);
  return {
    contentOffset: 16 + headerLength,
    fd,
    header: JSON.parse(header.toString("utf8")),
  };
}

function visitFiles(node, parts, result) {
  if (node.files) {
    for (const [name, child] of Object.entries(node.files)) {
      visitFiles(child, [...parts, name], result);
    }
    return;
  }
  if (!node.unpacked && Number.isFinite(node.size) && node.size > 0) {
    result.push({ path: parts.join("/"), ...node });
  }
}

const methods = {};
for (const [kind, file] of [
  ["clientRequest", "ClientRequest.json"],
  ["serverRequest", "ServerRequest.json"],
  ["clientNotification", "ClientNotification.json"],
  ["serverNotification", "ServerNotification.json"],
]) {
  const schema = JSON.parse(fs.readFileSync(path.join(schemaRoot, file), "utf8"));
  methods[kind] = schema.oneOf.map((variant) => variant.properties.method.enum[0]);
}

const allMethods = Object.values(methods).flat();
const matches = new Map(allMethods.map((method) => [method, { files: {} }]));

const index = readAsarIndex(asarPath);
try {
  const files = [];
  visitFiles(index.header, [], files);
  for (const file of files) {
    if (!/\.(?:js|css|json|html|map)$/.test(file.path)) continue;
    const buffer = Buffer.alloc(file.size);
    fs.readSync(index.fd, buffer, 0, buffer.length, index.contentOffset + Number(file.offset));
    const source = buffer.toString("utf8");
    for (const method of allMethods) {
      let position = source.indexOf(method);
      if (position < 0) continue;
      const entry = matches.get(method);
      entry.files[file.path] = (entry.files[file.path] ?? 0) + 1;
      let count = 1;
      position = source.indexOf(method, position + method.length);
      while (position >= 0 && count < 100) {
        count += 1;
        position = source.indexOf(method, position + method.length);
      }
      entry.count = (entry.count ?? 0) + count;
    }
  }
} finally {
  fs.closeSync(index.fd);
}

const report = {};
for (const [kind, list] of Object.entries(methods)) {
  report[kind] = {};
  for (const method of list) {
    const entry = matches.get(method);
    const files = Object.entries(entry.files ?? {})
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3)
      .map(([file, count]) => ({ file, count }));
    report[kind][method] = { count: entry.count ?? 0, files };
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  path.join(outputDir, "reference-method-scan.json"),
  JSON.stringify(report, null, 2) + "\n",
);

const lines = [];
for (const [kind, list] of Object.entries(methods)) {
  const used = list.filter((method) => (matches.get(method).count ?? 0) > 0);
  lines.push(kind + ": " + used.length + "/" + list.length);
  for (const method of used) {
    lines.push("  " + method + " (" + matches.get(method).count + ")");
  }
}
process.stdout.write(lines.join("\n") + "\n");
