#!/usr/bin/env python3
"""Deterministic MCP + OAuth 2.1 authorization server for stage-4 verification.

Implements the discovery, dynamic client registration, PKCE authorization-code
flow, and a protected streamable-HTTP MCP endpoint so `mcpServer/oauth/login`
can be exercised end to end without touching a real third-party service.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import secrets
import threading
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Store:
    def __init__(self, mode: str) -> None:
        self.mode = mode
        self.clients: dict[str, dict] = {}
        self.codes: dict[str, dict] = {}
        self.tokens: dict[str, dict] = {}
        self.events: list[dict] = []
        self.lock = threading.Lock()

    def record(self, event: str, **fields: object) -> None:
        with self.lock:
            self.events.append({"at": time.time(), "event": event, **fields})


def base_url(handler: BaseHTTPRequestHandler) -> str:
    host = handler.headers.get("Host") or f"127.0.0.1:{handler.server.server_port}"
    return f"http://{host}"


def json_response(handler: BaseHTTPRequestHandler, status: int, payload: object, extra_headers: dict[str, str] | None = None) -> None:
    body = json.dumps(payload).encode()
    handler.send_response(status)
    handler.send_header("Content-Type", "application/json")
    handler.send_header("Content-Length", str(len(body)))
    handler.send_header("Access-Control-Allow-Origin", "*")
    for key, value in (extra_headers or {}).items():
        handler.send_header(key, value)
    handler.end_headers()
    handler.wfile.write(body)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    store: Store

    def log_message(self, format: str, *args: object) -> None:  # noqa: A003 - stdlib signature
        if os.environ.get("MCP_OAUTH_VERBOSE"):
            super().log_message(format, *args)

    def _resource_metadata_url(self) -> str:
        return f"{base_url(self)}/.well-known/oauth-protected-resource"

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(length) if length else b""

    def do_GET(self) -> None:  # noqa: N802 - stdlib signature
        parsed = urllib.parse.urlparse(self.path)
        query = urllib.parse.parse_qs(parsed.query)
        path = parsed.path.rstrip("/") or "/"
        if path in ("/.well-known/oauth-protected-resource", "/.well-known/oauth-protected-resource/mcp"):
            json_response(
                self,
                200,
                {
                    "resource": f"{base_url(self)}/mcp",
                    "authorization_servers": [base_url(self)],
                    "bearer_methods_supported": ["header"],
                },
            )
            return
        if path in ("/.well-known/oauth-authorization-server", "/.well-known/openid-configuration"):
            json_response(
                self,
                200,
                {
                    "issuer": base_url(self),
                    "authorization_endpoint": f"{base_url(self)}/authorize",
                    "token_endpoint": f"{base_url(self)}/token",
                    "registration_endpoint": f"{base_url(self)}/register",
                    "response_types_supported": ["code"],
                    "grant_types_supported": ["authorization_code", "refresh_token"],
                    "code_challenge_methods_supported": ["S256"],
                    "token_endpoint_auth_methods_supported": ["none", "client_secret_post"],
                    "scopes_supported": ["notes.read", "notes.write"],
                },
            )
            return
        if path == "/authorize":
            self._authorize(query)
            return
        if path == "/events":
            json_response(self, 200, {"events": self.store.events})
            return
        if path == "/health":
            json_response(self, 200, {"status": "ok", "mode": self.store.mode})
            return
        if path == "/mcp":
            json_response(self, 405, {"error": "use POST for the MCP endpoint"})
            return
        json_response(self, 404, {"error": "not_found", "path": path})

    def _authorize(self, query: dict[str, list[str]]) -> None:
        redirect_uri = (query.get("redirect_uri") or [""])[0]
        state = (query.get("state") or [""])[0]
        code_challenge = (query.get("code_challenge") or [""])[0]
        client_id = (query.get("client_id") or [""])[0]
        scope = (query.get("scope") or [""])[0]
        mode = self.store.mode
        self.store.record("authorize", client_id=client_id, redirect_uri=redirect_uri, scope=scope, mode=mode)
        if not redirect_uri:
            json_response(self, 400, {"error": "invalid_request"})
            return
        if mode == "deny":
            location = f"{redirect_uri}?error=access_denied&error_description=user_denied&state={urllib.parse.quote(state)}"
            self.send_response(302)
            self.send_header("Location", location)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if mode == "hang":
            # Simulate a user who never finishes authorizing.
            time.sleep(600)
            return
        code = secrets.token_urlsafe(24)
        with self.store.lock:
            self.store.codes[code] = {
                "client_id": client_id,
                "redirect_uri": redirect_uri,
                "code_challenge": code_challenge,
                "scope": scope,
            }
        location = f"{redirect_uri}?code={urllib.parse.quote(code)}&state={urllib.parse.quote(state)}"
        self.send_response(302)
        self.send_header("Location", location)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self) -> None:  # noqa: N802 - stdlib signature
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path.rstrip("/") or "/"
        body = self._read_body()
        if path == "/register":
            payload = json.loads(body or b"{}")
            client_id = f"echora-client-{secrets.token_hex(6)}"
            with self.store.lock:
                self.store.clients[client_id] = payload
            self.store.record("register", client_id=client_id, redirect_uris=payload.get("redirect_uris"))
            json_response(
                self,
                201,
                {
                    "client_id": client_id,
                    "client_secret": "echora-secret",
                    "redirect_uris": payload.get("redirect_uris", []),
                    "token_endpoint_auth_method": "client_secret_post",
                    "grant_types": ["authorization_code", "refresh_token"],
                    "response_types": ["code"],
                },
            )
            return
        if path == "/token":
            form = urllib.parse.parse_qs(body.decode())
            grant = (form.get("grant_type") or [""])[0]
            code = (form.get("code") or [""])[0]
            verifier = (form.get("code_verifier") or [""])[0]
            with self.store.lock:
                record = self.store.codes.pop(code, None)
            if record is None:
                json_response(self, 400, {"error": "invalid_grant"})
                return
            if record["code_challenge"]:
                digest = hashlib.sha256(verifier.encode()).digest()
                expected = base64.urlsafe_b64encode(digest).decode().rstrip("=")
                if expected != record["code_challenge"]:
                    json_response(self, 400, {"error": "invalid_grant", "error_description": "pkce_mismatch"})
                    return
            token = f"echora-token-{secrets.token_hex(12)}"
            with self.store.lock:
                self.store.tokens[token] = {"scope": record.get("scope", "")}
            self.store.record("token", grant_type=grant)
            json_response(
                self,
                200,
                {
                    "access_token": token,
                    "refresh_token": f"echora-refresh-{secrets.token_hex(8)}",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "scope": record.get("scope", "notes.read notes.write"),
                },
            )
            return
        if path == "/mcp":
            self._mcp(body)
            return
        if path == "/reset":
            self.store.mode = (json.loads(body or b"{}") or {}).get("mode", self.store.mode)
            json_response(self, 200, {"mode": self.store.mode})
            return
        json_response(self, 404, {"error": "not_found", "path": path})

    def _mcp(self, body: bytes) -> None:
        authorization = self.headers.get("Authorization") or ""
        token = authorization.removeprefix("Bearer ").strip()
        with self.store.lock:
            known = token in self.store.tokens
        if not known:
            json_response(
                self,
                401,
                {"error": "invalid_token"},
                {"WWW-Authenticate": f'Bearer resource_metadata="{self._resource_metadata_url()}"'},
            )
            return
        try:
            message = json.loads(body)
        except json.JSONDecodeError:
            json_response(self, 400, {"error": "invalid_json"})
            return
        method = message.get("method")
        request_id = message.get("id")
        self.store.record("mcp_request", method=method)
        if method == "initialize":
            result = {
                "protocolVersion": message.get("params", {}).get("protocolVersion", "2025-06-18"),
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "notes-catalog", "version": "2.1.0", "title": "Notes catalog", "websiteUrl": "https://example.invalid/notes"},
            }
        elif method == "tools/list":
            result = {
                "tools": [
                    {"name": "list-notes", "description": "List stored notes", "inputSchema": {"type": "object", "properties": {}}},
                    {"name": "search-notes", "description": "Search notes by keyword", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}},
                ]
            }
        elif method == "tools/call":
            result = {"content": [{"type": "text", "text": "note: stage four"}]}
        else:
            json_response(self, 404, {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32601, "message": f"unsupported method {method}"}})
            return
        json_response(self, 200, {"jsonrpc": "2.0", "id": request_id, "result": result})


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8791)
    parser.add_argument("--mode", choices=["accept", "deny", "hang"], default="accept")
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    Handler.store = Store(args.mode)
    print(json.dumps({"listening": f"http://127.0.0.1:{args.port}", "mode": args.mode}), flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
