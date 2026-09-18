#!/usr/bin/env node

// Read-only check that docs/APP_SERVER_INTEGRATION.md still matches the
// app-server schema it claims as its baseline. The method tables are edited by
// hand, and a bad merge once dropped whole rows without changing any code, so
// this script re-derives every structural invariant from the schema and from
// the repository instead of trusting the document's own numbers.
//
// It verifies:
//   * each direction's method set equals the schema union (default +
//     experimental), with the heading count and no duplicate method names,
//   * the default/experimental column follows the schema,
//   * the status column uses known values, every integrated row names an
//     entry point, and the summary counts match the rows,
//   * the "共 N 个方法" line matches the tables,
//   * the `兼容退订` rows equal OPT_OUT_NOTIFICATION_METHODS in
//     src/agent/codex/runtime.rs.
//
// Usage:
//   node scripts/verify_integration_table.mjs
//   node scripts/verify_integration_table.mjs --schema-root <dir> --doc <file>

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const DIRECTIONS = [
  { heading: "客户端请求", direction: "clientRequest", schema: "ClientRequest.json" },
  { heading: "服务端请求", direction: "serverRequest", schema: "ServerRequest.json" },
  { heading: "客户端通知", direction: "clientNotification", schema: "ClientNotification.json" },
  { heading: "服务端通知", direction: "serverNotification", schema: "ServerNotification.json" },
];

const STATUSES = ["已接入", "后端已接入", "部分接入", "兼容退订", "未接入"];
const OPT_OUT_STATUS = "兼容退订";
const UNINTEGRATED_STATUS = "未接入";
const EMPTY_CELL = "—";
const OPT_OUT_SOURCE = "src/agent/codex/runtime.rs";

function parseArgs(argv) {
  const options = {
    doc: path.join(repoRoot, "docs/APP_SERVER_INTEGRATION.md"),
    schemaRoot: path.join(repoRoot, "artifacts/app-server-schema"),
  };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--doc" || flag === "--schema-root") {
      const value = argv[index + 1];
      if (!value) {
        process.stderr.write(`${flag} 需要路径参数\n`);
        process.exit(2);
      }
      options[flag === "--doc" ? "doc" : "schemaRoot"] = path.resolve(value);
      index += 1;
      continue;
    }
    process.stderr.write(`未知参数 ${flag}\n`);
    process.exit(2);
  }
  return options;
}

function schemaPath(root, variant, file) {
  return path.join(root, variant, file);
}

/// The checked-in table is generated on demand: `artifacts/` is gitignored, so
/// a fresh clone has no schema export. Generate one into a temp directory with
/// the CLI the document names instead of failing.
function resolveSchemaRoot(root) {
  const required = [];
  for (const variant of ["default", "experimental"]) {
    for (const { schema } of DIRECTIONS) {
      required.push(schemaPath(root, variant, schema));
    }
  }
  if (required.every((file) => fs.existsSync(file))) return { root, generated: false };

  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "app-server-schema-"));
  try {
    execFileSync(
      "codex",
      ["app-server", "generate-json-schema", "--out", path.join(temp, "default")],
      { stdio: ["ignore", "ignore", "pipe"] },
    );
    execFileSync(
      "codex",
      [
        "app-server",
        "generate-json-schema",
        "--experimental",
        "--out",
        path.join(temp, "experimental"),
      ],
      { stdio: ["ignore", "ignore", "pipe"] },
    );
  } catch (error) {
    process.stderr.write(
      `${root} 下没有可用的 schema，也无法运行 \`codex app-server generate-json-schema\`：` +
        `${error?.message ?? error}\n` +
        "请先执行：\n" +
        "  codex app-server generate-json-schema --out artifacts/app-server-schema/default\n" +
        "  codex app-server generate-json-schema --experimental --out artifacts/app-server-schema/experimental\n",
    );
    process.exit(2);
  }
  return { root: temp, generated: true };
}

function methodsOf(file) {
  const schema = JSON.parse(fs.readFileSync(file, "utf8"));
  const methods = schema.oneOf.map((variant) => {
    const values = variant?.properties?.method?.enum;
    if (!Array.isArray(values) || values.length !== 1) {
      throw new Error(`${file} 中存在没有单一 method 枚举的 schema 变体`);
    }
    return values[0];
  });
  return new Set(methods);
}

function splitRow(line) {
  const cells = line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|");
  return cells.map((cell) => cell.trim());
}

function isSeparatorRow(cells) {
  return cells.every((cell) => /^:?-{2,}:?$/.test(cell));
}

function parseDocument(text) {
  const sections = new Map(
    DIRECTIONS.map(({ heading }) => [
      heading,
      { heading, declared: null, rows: [] },
    ]),
  );
  const summary = new Map();
  let totals = null;
  let current = null;

  for (const rawLine of text.split("\n")) {
    const line = rawLine.trimEnd();
    const sectionMatch = /^### (客户端请求|服务端请求|客户端通知|服务端通知)（(\d+)）$/.exec(line);
    if (sectionMatch) {
      current = sections.get(sectionMatch[1]);
      current.declared = Number(sectionMatch[2]);
      continue;
    }
    if (line.startsWith("## ")) {
      current = null;
      continue;
    }
    if (!line.startsWith("|")) continue;
    const cells = splitRow(line);
    if (isSeparatorRow(cells)) continue;

    if (cells.length === 5 && current) {
      if (line.startsWith("| `")) {
        current.rows.push({
          method: cells[0].replace(/`/g, ""),
          api: cells[1],
          status: cells[2],
          entry: cells[4],
        });
      }
      continue;
    }
    if (cells.length === 3 && STATUSES.includes(cells[0])) {
      summary.set(cells[0], Number(cells[1]));
    }
  }

  const totalsMatch = /\*\*(\d+)\*\* 个方法：(\d+) 个客户端请求、(\d+) 个服务端请求、(\d+) 个客户端通知、(\d+) 个服务端通知/.exec(
    text,
  );
  if (totalsMatch) {
    totals = {
      total: Number(totalsMatch[1]),
      clientRequest: Number(totalsMatch[2]),
      serverRequest: Number(totalsMatch[3]),
      clientNotification: Number(totalsMatch[4]),
      serverNotification: Number(totalsMatch[5]),
    };
  }

  return { sections, summary, totals };
}

function optOutMethods() {
  const file = path.join(repoRoot, OPT_OUT_SOURCE);
  const source = fs.readFileSync(file, "utf8");
  const block = /OPT_OUT_NOTIFICATION_METHODS: &\[&str\] = &\[([\s\S]*?)\];/.exec(source);
  if (!block) throw new Error(`${OPT_OUT_SOURCE} 中找不到 OPT_OUT_NOTIFICATION_METHODS`);
  return [...block[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]);
}

function sorted(values) {
  return [...values].sort();
}

function formatList(values) {
  return values.length === 0 ? "无" : values.join("、");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const { root, generated } = resolveSchemaRoot(options.schemaRoot);
  const text = fs.readFileSync(options.doc, "utf8");
  const { sections, summary, totals } = parseDocument(text);
  const failures = [];
  const notes = [];

  process.stdout.write(`integrity check: ${path.relative(repoRoot, options.doc)}\n`);
  process.stdout.write(
    `schema: ${generated ? `${options.schemaRoot}（缺失，已用本机 codex 生成临时副本）` : options.schemaRoot}\n\n`,
  );

  // 1. Method set and default/experimental column, per direction.
  const seen = new Map();
  for (const { heading, direction, schema } of DIRECTIONS) {
    const section = sections.get(heading);
    const expected = new Set([
      ...methodsOf(schemaPath(root, "default", schema)),
      ...methodsOf(schemaPath(root, "experimental", schema)),
    ]);
    const documented = new Set(section.rows.map((row) => row.method));
    const missing = sorted([...expected].filter((method) => !documented.has(method)));
    const extra = sorted([...documented].filter((method) => !expected.has(method)));
    if (missing.length || extra.length) {
      failures.push(
        `${heading}：与 schema union 不一致；缺少 ${formatList(missing)}；多余 ${formatList(extra)}`,
      );
    }

    if (section.declared !== section.rows.length) {
      failures.push(
        `${heading}：标题声明 ${section.declared ?? "缺失"} 个方法，表内实际 ${section.rows.length} 个`,
      );
    }

    const defaults = methodsOf(schemaPath(root, "default", schema));
    for (const row of section.rows) {
      const expectedApi = defaults.has(row.method) ? "默认" : "实验";
      if (!expected.has(row.method)) continue;
      if (row.api !== expectedApi) {
        failures.push(`${row.method}：API 列是 ${row.api}，schema 归属为 ${expectedApi}`);
      }
      if (!STATUSES.includes(row.status)) {
        failures.push(`${row.method}：状态 \`${row.status}\` 不在 ${STATUSES.join("／")} 之内`);
      }
      if (row.status !== UNINTEGRATED_STATUS && row.entry === EMPTY_CELL) {
        failures.push(`${row.method}：状态为 ${row.status}，但入口列为空`);
      }
      const previous = seen.get(row.method);
      if (previous) {
        failures.push(`${row.method}：同时在「${previous}」和「${heading}」中出现`);
      } else {
        seen.set(row.method, heading);
      }
    }
  }

  // 2. Summary table and totals line.
  const actual = new Map(STATUSES.map((status) => [status, 0]));
  for (const section of sections.values()) {
    for (const row of section.rows) {
      if (actual.has(row.status)) actual.set(row.status, actual.get(row.status) + 1);
    }
  }
  const rowTotal = [...actual.values()].reduce((sum, value) => sum + value, 0);
  for (const status of STATUSES) {
    if (summary.get(status) !== actual.get(status)) {
      failures.push(
        `口径表 ${status} 是 ${summary.get(status) ?? "缺失"}，表内实际 ${actual.get(status)}`,
      );
    }
  }
  if (totals === null) {
    failures.push("找不到「共 **N** 个方法：…」合计行");
  } else {
    const declared = DIRECTIONS.map(({ direction }) => totals[direction]);
    const actualCounts = DIRECTIONS.map(({ heading }) => sections.get(heading).rows.length);
    if (totals.total !== rowTotal || declared.some((value, index) => value !== actualCounts[index])) {
      failures.push(
        `合计行 ${totals.total}（${declared.join("/")}）与表内 ${rowTotal}（${actualCounts.join("/")}）不一致`,
      );
    }
  }

  // 3. The opt-out rows must stay the opt-out list the connection sends.
  const declaredOptOut = sorted(
    DIRECTIONS.flatMap(({ heading }) =>
      sections
        .get(heading)
        .rows.filter((row) => row.status === OPT_OUT_STATUS)
        .map((row) => row.method),
    ),
  );
  const codeOptOut = sorted(optOutMethods());
  if (declaredOptOut.join(",") !== codeOptOut.join(",")) {
    failures.push(
      `兼容退订与 ${OPT_OUT_SOURCE} 不一致；表中 ${formatList(declaredOptOut)}；代码 ${formatList(codeOptOut)}`,
    );
  }
  if (summary.get(OPT_OUT_STATUS) !== declaredOptOut.length) {
    notes.push(`兼容退订计数 ${summary.get(OPT_OUT_STATUS)} 与表中 ${declaredOptOut.length} 行不一致`);
  }

  const statusLine = STATUSES.map((status) => `${status} ${actual.get(status)}`).join("、");
  if (failures.length === 0) {
    process.stdout.write(`✓ ${rowTotal} 项口径与 schema 一致：${statusLine}\n`);
    process.stdout.write(`✓ 四个方向的 API 列、方法唯一性与标题计数一致\n`);
    process.stdout.write(`✓ 兼容退订与 ${OPT_OUT_SOURCE} 一致\n`);
    for (const note of notes) process.stdout.write(`! ${note}\n`);
    return 0;
  }
  process.stdout.write(`✗ 发现 ${failures.length} 处不一致：\n`);
  for (const failure of failures) process.stdout.write(`  - ${failure}\n`);
  return 1;
}

process.exit(main());
