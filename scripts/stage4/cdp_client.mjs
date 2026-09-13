#!/usr/bin/env node
// Shared minimal CDP client for the isolated ChatGPT reference instance.
import fs from "node:fs";
import http from "node:http";
import path from "node:path";

export const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

export function option(name, fallback = null) {
  const prefix = `--${name}=`;
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) ?? fallback;
}

function getJson(url) {
  return new Promise((resolve, reject) => {
    http
      .get(url, (response) => {
        let body = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => (body += chunk));
        response.on("end", () => {
          try {
            resolve(JSON.parse(body));
          } catch (error) {
            reject(new Error(`invalid JSON from ${url}: ${error.message}`));
          }
        });
      })
      .on("error", reject);
  });
}

export class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 0;
    this.pending = new Map();
    this.sessionId = null;
    this.socket.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.id == null) return;
      const callback = this.pending.get(message.id);
      if (!callback) return;
      this.pending.delete(message.id);
      if (message.error) callback.reject(new Error(`${message.method}: ${message.error.message}`));
      else callback.resolve(message.result);
    };
  }

  static async connect(endpoint = "http://127.0.0.1:9335", urlMatch = "app://-/index.html") {
    const targets = await getJson(`${endpoint}/json/list`);
    // Prefer the exact main window; other page targets (avatar overlay,
    // detached windows) share the same URL prefix but render different content.
    const target =
      targets.find((candidate) => candidate.type === "page" && candidate.url === urlMatch) ??
      targets.find(
        (candidate) => candidate.type === "page" && candidate.url.startsWith(urlMatch),
      );
    if (!target) {
      throw new Error(
        `no CDP page target matching ${urlMatch} on ${endpoint}; found ${targets
          .map((candidate) => candidate.url)
          .join(", ")}`,
      );
    }
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.onopen = resolve;
      socket.onerror = reject;
    });
    const cdp = new Cdp(socket);
    cdp.targetId = target.id;
    await cdp.send("Runtime.enable");
    await cdp.send("Page.enable");
    return cdp;
  }

  /// Browser level connection: required for window bounds, which are not
  /// addressable from a page target session.
  static async connectBrowser(endpoint = "http://127.0.0.1:9335") {
    const version = await getJson(`${endpoint}/json/version`);
    const socket = new WebSocket(version.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.onopen = resolve;
      socket.onerror = reject;
    });
    return new Cdp(socket);
  }

  async setWindowSizeForTarget(targetId, width, height) {
    const { windowId } = await this.send("Browser.getWindowForTarget", { targetId });
    const { bounds } = await this.send("Browser.getWindowBounds", { windowId });
    await this.send("Browser.setWindowBounds", {
      windowId,
      bounds: {
        left: bounds.left,
        top: bounds.top,
        width,
        height,
        windowState: "normal",
      },
    });
    return { windowId, previous: bounds };
  }

  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.nextId;
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP command timed out: ${method}`));
      }, 60_000);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timeout);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timeout);
          reject(error);
        },
      });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression, { awaitPromise = true } = {}) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(
        result.exceptionDetails.exception?.description ?? result.exceptionDetails.text,
      );
    }
    return result.result.value;
  }

  async key(key, modifiers = 0, code = null) {
    const params = { type: "keyDown", key, modifiers, code: code ?? key };
    await this.send("Input.dispatchKeyEvent", { ...params, type: "rawKeyDown" });
    await this.send("Input.dispatchKeyEvent", { ...params, type: "keyUp" });
  }

  async click(x, y, { clickCount = 1 } = {}) {
    await this.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await this.send("Input.dispatchMouseEvent", {
      type: "mousePressed",
      x,
      y,
      button: "left",
      buttons: 1,
      clickCount,
    });
    await this.send("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x,
      y,
      button: "left",
      buttons: 0,
      clickCount,
    });
  }

  async hover(x, y) {
    await this.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  }

  async scroll(x, y, deltaY) {
    await this.send("Input.dispatchMouseEvent", {
      type: "mouseWheel",
      x,
      y,
      deltaX: 0,
      deltaY,
    });
  }

  async windowBounds() {
    const { windowId } = await this.send("Browser.getWindowForTarget", { targetId: this.targetId });
    const { bounds } = await this.send("Browser.getWindowBounds", { windowId });
    return { windowId, bounds };
  }

  async setWindowBounds(bounds) {
    const { windowId } = await this.send("Browser.getWindowForTarget", { targetId: this.targetId });
    await this.send("Browser.setWindowBounds", { windowId, bounds });
    return windowId;
  }

  async screenshot(file, { clip = null } = {}) {
    const result = await this.send("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: false,
      ...(clip ? { clip: { ...clip, scale: 1 } } : {}),
    });
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, Buffer.from(result.data, "base64"));
    return file;
  }

  close() {
    this.socket.close();
  }
}

export function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}
