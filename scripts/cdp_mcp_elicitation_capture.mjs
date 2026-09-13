#!/usr/bin/env node
// Real-CDP driver for the ChatGPT desktop app elicitation card.
//
// Every interaction goes through Input.dispatchMouseEvent /
// Input.dispatchKeyEvent / Input.insertText on the dedicated instance started
// with --remote-debugging-port=9334; the DOM is only read to locate geometry
// and computed styles. Run with a command from the list below.
import fs from "node:fs";
import path from "node:path";

const endpoint = process.env.CDP_ENDPOINT ?? "http://127.0.0.1:9334";
const outputDir = path.resolve(
  process.env.CDP_OUTPUT ?? "artifacts/mcp-elicitation-cdp-20260913",
);
const [command, ...args] = process.argv.slice(2);
const CARD_TEXT =
  process.env.CARD_ANCHOR_TEXT ?? "部署前需要确认以下信息";

fs.mkdirSync(path.join(outputDir, "raw"), { recursive: true });

const targets = await (await fetch(endpoint + "/json/list")).json();
const target = targets.find(
  (candidate) => candidate.type === "page" && candidate.url === "app://-/index.html",
);
if (!target) throw new Error("ChatGPT app://-/index.html target not found on " + endpoint);

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let nextId = 0;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id == null) return;
  const callback = pending.get(message.id);
  if (!callback) return;
  pending.delete(message.id);
  if (message.error) callback.reject(new Error(JSON.stringify(message.error)));
  else callback.resolve(message.result);
};

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const response = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (response.exceptionDetails) {
    throw new Error(
      response.exceptionDetails.exception?.description ?? response.exceptionDetails.text,
    );
  }
  return response.result.value;
}

const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function clickPoint(x, y) {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x,
    y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x,
    y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function typeText(text) {
  for (const character of text) {
    await send("Input.dispatchKeyEvent", {
      type: "keyDown",
      text: character,
      unmodifiedText: character,
      key: character,
    });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: character });
    await sleep(12);
  }
}

async function pressKey(key, modifiers = 0) {
  const base = { key, modifiers, windowsVirtualKeyCode: modifiers === 8 ? 0 : undefined };
  await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}

async function screenshot(name) {
  const image = await send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  const file = path.join(outputDir, "raw", name + ".png");
  fs.writeFileSync(file, Buffer.from(image.data, "base64"));
  return file;
}

/** Innermost title node, then walk up to the rounded card container. */
const CARD_EXPRESSION =
  "(() => { const all = Array.from(document.querySelectorAll('div,span,p,h1,h2,h3,section,form'));" +
  " const matches = all.filter((node) => (node.textContent ?? '').trim() ===" +
  " " +
  JSON.stringify(CARD_TEXT) +
  "); const anchor = matches[matches.length - 1]; if (!anchor) return null; let node = anchor;" +
  " for (let step = 0; step < 12 && node && node.tagName !== 'BODY'; step += 1) {" +
  " const style = getComputedStyle(node); const rect = node.getBoundingClientRect();" +
  " if (parseFloat(style.borderRadius) >= 20 && rect.width >= 600) return node;" +
  " node = node.parentElement; } return anchor; })()";

async function rectOf(expression) {
  return evaluate(
    "(() => { const element = (" +
      expression +
      "); if (!element) return null; const rect = element.getBoundingClientRect();" +
      " if (rect.width <= 0 || rect.height <= 0) return null;" +
      " return { x: rect.x, y: rect.y, width: rect.width, height: rect.height }; })()",
  );
}

async function clickExpression(expression, description) {
  const rect = await rectOf(expression);
  if (!rect) throw new Error((description ?? "element") + " not found");
  await clickPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
  return rect;
}

async function cardRect() {
  return rectOf(CARD_EXPRESSION);
}

async function composerRect() {
  const selectors = [
    "[contenteditable='true'][role='textbox']",
    "div[contenteditable='true']",
    "textarea",
  ];
  for (const selector of selectors) {
    const rect = await rectOf("document.querySelector(" + JSON.stringify(selector) + ")");
    if (rect) return { selector, rect };
  }
  return null;
}

async function waitForCard(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await cardRect()) return true;
    await sleep(500);
  }
  return false;
}

async function snapshotStyles(name) {
  const data = await evaluate(
    "(() => { const element = (" +
      CARD_EXPRESSION +
      "); if (!element) return null;" +
      " const describe = (node, label) => { const style = getComputedStyle(node);" +
      " const rect = node.getBoundingClientRect(); const properties = {};" +
      " for (const property of style) properties[property] = style.getPropertyValue(property);" +
      " return { label, tag: node.tagName.toLowerCase()," +
      " role: node.getAttribute?.('role') ?? null," +
      " ariaLabel: node.getAttribute?.('aria-label') ?? null," +
      " className: typeof node.className === 'string' ? node.className : null," +
      " text: (node.innerText ?? '').slice(0, 300)," +
      " rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height }," +
      " properties }; };" +
      " const collect = (node, prefix) => Array.from(node.children ?? []).flatMap((child, index) => {" +
      " const label = prefix + '.' + index + ':' + child.tagName.toLowerCase();" +
      " return [describe(child, label), ...collect(child, label)]; });" +
      " return { capturedAt: new Date().toISOString(), devicePixelRatio: window.devicePixelRatio," +
      " viewport: { width: window.innerWidth, height: window.innerHeight }," +
      " theme: document.documentElement.className," +
      " nodes: [describe(element, 'card'), ...collect(element, 'card').slice(0, 500)] }; })()",
  );
  if (data) {
    fs.writeFileSync(
      path.join(outputDir, "raw", name + ".styles.json"),
      JSON.stringify(data, null, 2),
    );
  }
  return data ? { nodes: data.nodes.length, rect: data.nodes[0].rect } : null;
}

function textMatchExpression(text, tags, exact) {
  return (
    "(() => { const nodes = Array.from(document.querySelectorAll(" +
    JSON.stringify(tags) +
    ")); const target = " +
    JSON.stringify(text) +
    "; const matches = nodes.filter((node) => { const value = (node.innerText ?? node.textContent ?? '').trim();" +
    (exact ? " return value === target; " : " return value.includes(target); ") +
    "}); return matches.find((node) => { const rect = node.getBoundingClientRect();" +
    " return rect.width > 0 && rect.height > 0; }) ?? null; })()"
  );
}

const actions = {
  async eval_expr() {
    return evaluate(args.join(" "));
  },
  async probe() {
    return {
      title: await evaluate("document.title"),
      url: await evaluate("location.href"),
      composer: await composerRect(),
      card: await cardRect(),
    };
  },
  async screenshot() {
    return { file: await screenshot(args[0] ?? "shot") };
  },
  async type() {
    const found = await composerRect();
    if (!found) throw new Error("composer not found");
    await clickPoint(found.rect.x + found.rect.width / 2, found.rect.y + found.rect.height / 2);
    await typeText(args.join(" "));
    return { typed: args.join(" "), composer: found };
  },
  async insert_text() {
    await send("Input.insertText", { text: args.join(" ") });
    return { inserted: args.join(" ") };
  },
  async type_focused() {
    const text = args.join(" ");
    await typeText(text);
    return { typed: text };
  },
  async enter() {
    await pressKey("Enter");
    return { ok: true };
  },
  async key() {
    const key = args[0];
    const shift = args.includes("--shift");
    await pressKey(key, shift ? 8 : 0);
    return { key, shift };
  },
  async wait_card() {
    return { found: await waitForCard(Number(args[0] ?? 90000)), rect: await cardRect() };
  },
  async card() {
    return { rect: await cardRect() };
  },
  async styles() {
    return snapshotStyles(args[0] ?? "card");
  },
  async click_text() {
    const text = args[0];
    const exact = !args.includes("--contains");
    return {
      rect: await clickExpression(
        textMatchExpression(text, "button,[role=button],div,span,label,p", exact),
        text,
      ),
    };
  },
  async hover_text() {
    const text = args[0];
    const rect = await rectOf(
      textMatchExpression(text, "button,[role=button],div,span,label,p", true),
    );
    if (!rect) throw new Error("hover target not found: " + text);
    await send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: rect.x + rect.width / 2,
      y: rect.y + rect.height / 2,
    });
    return { rect };
  },
  async click_at() {
    const x = Number(args[0]);
    const y = Number(args[1]);
    await clickPoint(x, y);
    return { x, y };
  },
  async scroll() {
    const deltaY = Number(args[0] ?? 200);
    const rect = (await cardRect()) ?? { x: 400, y: 300, width: 400, height: 300 };
    await send("Input.dispatchMouseEvent", {
      type: "mouseWheel",
      x: rect.x + rect.width / 2,
      y: rect.y + rect.height / 2,
      deltaX: 0,
      deltaY,
    });
    return { deltaY };
  },
  async card_text() {
    return evaluate(
      "(() => { const node = (" +
        CARD_EXPRESSION +
        "); if (!node) return null; return { text: node.innerText," +
        " html: node.outerHTML.slice(0, 80000) }; })()",
    );
  },
  async overlays() {
    return evaluate(
      "(() => Array.from(document.querySelectorAll('[role=dialog],[role=alertdialog],[role=menu],[role=listbox]'))" +
        ".map((node) => { const rect = node.getBoundingClientRect();" +
        " return { role: node.getAttribute('role'), text: (node.innerText ?? '').slice(0, 1200)," +
        " rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } }; }))()",
    );
  },
  async focus_probe() {
    return evaluate(
      "(() => { const active = document.activeElement;" +
        " if (!active) return null; const rect = active.getBoundingClientRect();" +
        " return { tag: active.tagName.toLowerCase(), role: active.getAttribute('role')," +
        " label: active.getAttribute('aria-label'), text: (active.innerText ?? active.value ?? '').slice(0, 200)," +
        " rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } }; })()",
    );
  },
};

const action = actions[command];
if (!action) {
  throw new Error("unknown command " + command + "; known: " + Object.keys(actions).join(", "));
}
const result = await action();
console.log(JSON.stringify(result ?? null, null, 2).slice(0, 24000));
socket.close();
