#!/usr/bin/env python3
"""Adversarial E2E lab for EQProxy (audit 2026-11-09).

Starts the *real* release binary and *real* TCP origins on loopback, then
drives raw HTTP/1.1, h2c via curl (S10/S11) and raw HTTP/2 frames (S12) from a
client that controls every byte. No mocks, no in-process shortcuts: every
result below is observed on a socket.

Writes lab/results.json and prints a PASS/FAIL table. Exit code is 1 while
any scenario fails, so this file doubles as a regression gate for the two
confirmed findings of this audit.
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
BINARY = ROOT / "target" / "release" / "eqproxy"
LAB = Path(__file__).resolve().parent

results: list[dict] = []
lock = threading.Lock()


def record(name, ok, detail, evidence=None, critical=False):
    with lock:
        results.append(
            {
                "name": name,
                "ok": bool(ok),
                "critical": critical,
                "detail": detail,
                "evidence": evidence or {},
            }
        )
    print(("PASS " if ok else "FAIL ") + name + " :: " + detail)


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class Origin(threading.Thread):
    """Threaded raw TCP origin. One handler per connection, keep-alive."""

    def __init__(self, port: int, desync: bool = False):
        super().__init__(daemon=True)
        self.port = port
        self.desync = desync
        self.requests: list[dict] = []
        self._lock = threading.Lock()
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(("127.0.0.1", port))
        self.sock.listen(64)

    def run(self):
        while True:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            threading.Thread(target=self.handle, args=(conn,), daemon=True).start()

    def handle(self, conn: socket.socket):
        conn.settimeout(5.0)
        buf = b""
        try:
            while True:
                while b"\r\n\r\n" not in buf:
                    chunk = conn.recv(65536)
                    if not chunk:
                        return
                    buf += chunk
                head, buf = buf.split(b"\r\n\r\n", 1)
                lines = head.split(b"\r\n")
                request_line = lines[0].decode("latin1")
                parts = request_line.split(" ")
                method = parts[0] if parts else ""
                path = parts[1] if len(parts) > 1 else ""
                headers = {}
                for line in lines[1:]:
                    if b":" in line:
                        k, v = line.split(b":", 1)
                        headers[k.decode("latin1").strip().lower()] = v.decode(
                            "latin1"
                        ).strip()
                with self._lock:
                    self.requests.append(
                        {
                            "method": method,
                            "path": path,
                            "conditional": "if-none-match" in headers
                            or "if-modified-since" in headers,
                            "headers": headers,
                        }
                    )
                self.respond(conn, method, path, headers, buf)
                # consume any request body declared by content-length (for the
                # scenarios that reach the origin); chunked bodies are not
                # used by this lab.
                try:
                    length = int(headers.get("content-length", "0"))
                except ValueError:
                    length = 0
                while len(buf) < length:
                    chunk = conn.recv(65536)
                    if not chunk:
                        return
                    buf += chunk
                buf = buf[length:]
        except (ConnectionError, socket.timeout, OSError):
            return
        finally:
            try:
                conn.close()
            except OSError:
                pass

    def reply(self, conn, status, headers, body=b""):
        out = [f"HTTP/1.1 {status}\r\n".encode()]
        for k, v in headers:
            out.append(f"{k}: {v}\r\n".encode())
        out.append(b"\r\n")
        out.append(body)
        conn.sendall(b"".join(out))

    def respond(self, conn, method, path, headers, buf):
        if self.desync:
            if path.startswith("/desync"):
                # Head first, then a *complete forged response* arrives after
                # the proxy has already checked the connection for extra bytes
                # and returned it to the pool. The next client on that pooled
                # connection must not receive it.
                conn.sendall(b"HTTP/1.1 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n")
                time.sleep(1.2)
                forged = b"SMUGGLED-RESPONSE"
                conn.sendall(
                    b"HTTP/1.1 200 OK\r\nContent-Length: "
                    + str(len(forged)).encode()
                    + b"\r\n\r\n"
                    + forged
                )
                return
            self.reply(conn, "200 OK", [("Content-Length", "7")], b"NORMAL2")
            return

        if path.startswith("/aud01"):
            if "if-none-match" in headers:
                self.reply(
                    conn,
                    "304 Not Modified",
                    [
                        ("ETag", '"v1"'),
                        ("Set-Cookie", "sid=evil; HttpOnly"),
                        ("X-Injected", "yes"),
                    ],
                )
                return
            with self._lock:
                count = sum(1 for r in self.requests if r["path"].startswith("/aud01"))
            if count <= 1:
                self.reply(
                    conn,
                    "200 OK",
                    [
                        ("Content-Length", str(len(b"STALE-BODY-v1"))),
                        ("ETag", '"v1"'),
                        ("Cache-Control", "max-age=1"),
                    ],
                    b"STALE-BODY-v1",
                )
            else:
                self.reply(
                    conn,
                    "200 OK",
                    [
                        ("Content-Length", str(len(b"FRESH-BODY-v2"))),
                        ("ETag", '"v2"'),
                        ("Cache-Control", "max-age=1"),
                    ],
                    b"FRESH-BODY-v2",
                )
            return
        if path.startswith("/maxage"):
            body = b"MAXAGE-BODY"
            self.reply(
                conn,
                "200 OK",
                [
                    ("Content-Length", str(len(body))),
                    ("ETag", '"m1"'),
                    ("Cache-Control", "max-age=60"),
                ],
                body,
            )
            return
        if path.startswith("/vary"):
            ae = headers.get("accept-encoding", "none")
            body = f"VARY-{ae}".encode()
            self.reply(
                conn,
                "200 OK",
                [
                    ("Content-Length", str(len(body))),
                    ("Vary", "Accept-Encoding"),
                    ("Cache-Control", "max-age=60"),
                ],
                body,
            )
            return
        if path.startswith("/head"):
            body = b"HELLO-WORLD"
            self.reply(
                conn,
                "200 OK",
                [
                    ("Content-Length", str(len(body))),
                    ("Cache-Control", "max-age=60"),
                ],
                body,
            )
            return
        if path.startswith("/echo"):
            body = ("|".join(f"{k}={v}" for k, v in sorted(headers.items()))).encode()
            self.reply(
                conn,
                "200 OK",
                [("Content-Length", str(len(body))), ("Content-Type", "text/plain")],
                body,
            )
            return
        if path.startswith("/hop"):
            # RFC 9113 §8.2.2: a header nominated by Connection is
            # hop-by-hop and must not cross an h2 translation.
            self.reply(
                conn,
                "200 OK",
                [
                    ("Content-Length", "2"),
                    ("Connection", "X-Secret"),
                    ("X-Secret", "leaked"),
                ],
                b"ok",
            )
            return
        self.reply(conn, "200 OK", [("Content-Length", "9")], b"NORMAL-OK")


def connect(port: int, timeout=5.0) -> socket.socket:
    s = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    s.settimeout(timeout)
    return s


def read_response(s: socket.socket) -> dict:
    """Reads one response (Content-Length only; chunked decoded minimally)."""
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = s.recv(65536)
        if not chunk:
            break
        buf += chunk
    if b"\r\n\r\n" not in buf:
        return {"raw": buf, "status": 0, "headers": {}, "body": b""}
    head, rest = buf.split(b"\r\n\r\n", 1)
    lines = head.split(b"\r\n")
    status = int(lines[0].split(b" ")[1]) if len(lines[0].split(b" ")) > 1 else 0
    headers = {}
    for line in lines[1:]:
        if b":" in line:
            k, v = line.split(b":", 1)
            headers[k.decode("latin1").strip().lower()] = v.decode("latin1").strip()
    body = rest
    if "content-length" in headers:
        want = int(headers["content-length"])
        while len(body) < want:
            chunk = s.recv(65536)
            if not chunk:
                break
            body += chunk
        body = body[:want]
    elif headers.get("transfer-encoding", "").lower() == "chunked":
        decoded = b""
        while True:
            while b"\r\n" not in body:
                body += s.recv(65536)
            line, body = body.split(b"\r\n", 1)
            size = int(line.split(b";")[0], 16)
            if size == 0:
                break
            while len(body) < size + 2:
                body += s.recv(65536)
            decoded += body[:size]
            body = body[size + 2 :]
        body = decoded
    return {"status": status, "headers": headers, "body": body, "raw": head}


def raw_exchange(port: int, payload: bytes, wait=1.0) -> bytes:
    s = connect(port, timeout=max(2.0, wait + 1))
    s.sendall(payload)
    s.settimeout(wait)
    out = b""
    try:
        while True:
            chunk = s.recv(65536)
            if not chunk:
                break
            out += chunk
    except socket.timeout:
        pass
    s.close()
    return out


H2_PREFACE = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"


def hpack_literal(name: str, value: str) -> bytes:
    """HPACK literal header field without indexing, new name, no Huffman.

    This is deliberately the simplest legal representation: the lab must not
depend on a Python h2/hpack package. Lengths are one byte because every
field used here is far below 127 bytes.
    """
    name_bytes = name.encode("ascii")
    value_bytes = value.encode("ascii")
    if len(name_bytes) > 127 or len(value_bytes) > 127:
        raise ValueError("lab HPACK literal too long")
    return (
        b"\x00"
        + bytes((len(name_bytes),))
        + name_bytes
        + bytes((len(value_bytes),))
        + value_bytes
    )


def h2_frame(frame_type: int, flags: int, stream_id: int, payload: bytes) -> bytes:
    return (
        len(payload).to_bytes(3, "big")
        + bytes((frame_type, flags))
        + (stream_id & 0x7FFFFFFF).to_bytes(4, "big")
        + payload
    )


def parse_h2_frames(raw: bytes) -> list[dict]:
    """Decodes server frames without needing an h2 library."""
    names = {
        0: "DATA",
        1: "HEADERS",
        2: "PRIORITY",
        3: "RST_STREAM",
        4: "SETTINGS",
        5: "PUSH_PROMISE",
        6: "PING",
        7: "GOAWAY",
        8: "WINDOW_UPDATE",
        9: "CONTINUATION",
    }
    frames = []
    offset = 0
    while offset + 9 <= len(raw):
        length = int.from_bytes(raw[offset : offset + 3], "big")
        frame_type = raw[offset + 3]
        flags = raw[offset + 4]
        stream = int.from_bytes(raw[offset + 5 : offset + 9], "big") & 0x7FFFFFFF
        payload = raw[offset + 9 : offset + 9 + length]
        item = {
            "type": names.get(frame_type, str(frame_type)),
            "flags": flags,
            "stream": stream,
            "payload_hex": payload.hex(),
        }
        if frame_type == 3 and length >= 4:
            item["error_code"] = int.from_bytes(payload[:4], "big")
        if frame_type == 7 and length >= 8:
            item["last_stream"] = int.from_bytes(payload[:4], "big") & 0x7FFFFFFF
            item["error_code"] = int.from_bytes(payload[4:8], "big")
            item["debug"] = payload[8:].decode("utf-8", "replace")
        frames.append(item)
        offset += 9 + length
    return frames


def raw_h2_request(
    port: int,
    path: str,
    authority: str,
    extra_headers: list[tuple[str, str]],
    origin: Origin,
    timeout: float = 3.0,
) -> tuple[bytes, dict | None]:
    """Sends one h2c HEADERS frame and waits for the real origin to see it.

    The client pretends to be a permissive HTTP/2 peer: it sends the bytes a
    conforming curl/h2 client refuses to emit (Connection-nominated names),
    but uses only legal HPACK and frame syntax. The verdict comes from the
    origin's recorded request, not from a synthesized response.
    """
    headers = [
        (":method", "GET"),
        (":scheme", "http"),
        (":path", path),
        (":authority", authority),
        ("user-agent", "raw-h2-lab"),
        *extra_headers,
    ]
    block = b"".join(hpack_literal(name, value) for name, value in headers)
    payload = (
        H2_PREFACE
        + h2_frame(0x4, 0, 0, b"")  # SETTINGS
        + h2_frame(0x1, 0x5, 1, block)  # HEADERS | END_STREAM | END_HEADERS
    )
    raw = b""
    seen = None
    s = connect(port, timeout=timeout)
    try:
        s.sendall(payload)
        s.settimeout(0.1)
        deadline = time.time() + timeout
        while time.time() < deadline:
            if seen is None:
                for request in list(origin.requests):
                    if request["path"] == path:
                        seen = request
                        break
            if seen is not None:
                break
            try:
                chunk = s.recv(65536)
            except socket.timeout:
                chunk = b""
            if chunk:
                raw += chunk
        return raw, seen
    finally:
        s.close()


def wait_port(port: int, timeout=15.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            s = socket.create_connection(("127.0.0.1", port), timeout=0.2)
            s.close()
            return True
        except OSError:
            time.sleep(0.05)
    return False


def main() -> int:
    main_up = free_port()
    desync_up = free_port()
    main_port = free_port()
    desync_port = free_port()

    origin = Origin(main_up)
    origin.start()
    desync_origin = Origin(desync_up, desync=True)
    desync_origin.start()

    config = LAB / "eqproxy.toml"
    config.write_text(
        f"""schema_version = 1

[listener.main]
listen = "127.0.0.1:{main_port}"
backend = "main"
mode = "http"
idle_timeout = "30s"

[listener.main.cache]
default_ttl = "60s"
max_memory_bytes = 8388608
max_entry_bytes = 1048576

[listener.desync]
listen = "127.0.0.1:{desync_port}"
backend = "desync"
mode = "http"
idle_timeout = "30s"

[backend.main]
server = "127.0.0.1:{main_up}"

[backend.desync]
server = "127.0.0.1:{desync_up}"
"""
    )

    log = open(LAB / "eqproxy.log", "wb")
    proc = subprocess.Popen(
        [str(BINARY), "run", "--config", str(config)],
        stdout=log,
        stderr=subprocess.STDOUT,
        cwd=str(LAB),
    )
    try:
        if not wait_port(main_port) or not wait_port(desync_port):
            record("startup", False, "eqproxy did not bind both listeners; see eqproxy.log")
            return 1
        record("startup", True, f"listeners {main_port}/{desync_port}, origins {main_up}/{desync_up}")

        # --- S1: AUD-01 regression, real 304 that revokes storability --------
        s = connect(main_port)
        s.sendall(b"GET /aud01 HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        first = read_response(s)
        s.close()
        time.sleep(1.3)  # entry (max-age=1) is now stale
        s = connect(main_port)
        s.sendall(
            b"GET /aud01 HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n"
        )
        second = read_response(s)
        s.close()
        aud_requests = [r for r in origin.requests if r["path"].startswith("/aud01")]
        detail = (
            f"first={first['body']!r} second={second['body']!r} "
            f"set-cookie={'set-cookie' in second['headers']} "
            f"x-injected={second['headers'].get('x-injected')} "
            f"origin_calls={[(r['conditional'], r['path']) for r in aud_requests]}"
        )
        ok = (
            first["body"] == b"STALE-BODY-v1"
            and second["body"] == b"FRESH-BODY-v2"
            and "set-cookie" not in second["headers"]
            and "x-injected" not in second["headers"]
            and any(not r["conditional"] for r in aud_requests[2:])
        )
        record(
            "S1_aud01_revoking_304_invalidates",
            ok,
            detail,
            {
                "first": {k: v.decode("latin1") if isinstance(v, bytes) else v for k, v in first.items() if k != "raw"},
                "second": {k: v.decode("latin1") if isinstance(v, bytes) else v for k, v in second.items() if k != "raw"},
                "origin_requests": aud_requests,
            },
            critical=True,
        )

        # --- S2: client max-age=0 is response-scoped (AUD-02) ---------------
        s = connect(main_port)
        s.sendall(b"GET /maxage HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        a = read_response(s)
        s.close()
        s = connect(main_port)
        s.sendall(b"GET /maxage HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        b = read_response(s)
        s.close()
        s = connect(main_port)
        s.sendall(
            b"GET /maxage HTTP/1.1\r\nHost: example.test\r\n"
            b"Cache-Control: max-age=0\r\nConnection: close\r\n\r\n"
        )
        c = read_response(s)
        s.close()
        s = connect(main_port)
        s.sendall(b"GET /maxage HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        d = read_response(s)
        s.close()
        maxage_requests = [r for r in origin.requests if r["path"].startswith("/maxage")]
        ok = (
            a["headers"].get("x-cache") == "MISS"
            and b["headers"].get("x-cache") == "HIT"
            and c["headers"].get("x-cache") in ("MISS", "REVALIDATED")
            and c["body"] == b"MAXAGE-BODY"
            and d["headers"].get("x-cache") == "HIT"
            and len(maxage_requests) == 2
        )
        record(
            "S2_client_max_age_zero_scoped",
            ok,
            f"x-cache={[a['headers'].get('x-cache'), b['headers'].get('x-cache'), c['headers'].get('x-cache'), d['headers'].get('x-cache')]} "
            f"origin_calls={len(maxage_requests)} (expected 2: initial + revalidation)",
            {"origin_requests": maxage_requests},
            critical=True,
        )

        # --- S3: request smuggling CL.TE / duplicate CL / bad TE -------------
        smuggled = b"GET /smuggled HTTP/1.1\r\nHost: example.test\r\n\r\n"
        payloads = {
            "cl_te": b"POST /x HTTP/1.1\r\nHost: example.test\r\nContent-Length: 4\r\n"
            b"Transfer-Encoding: chunked\r\n\r\n0\r\n\r\n" + smuggled,
            "dup_cl": b"POST /x HTTP/1.1\r\nHost: example.test\r\nContent-Length: 4\r\n"
            b"Content-Length: 5\r\n\r\nAAAA" + smuggled,
            "te_not_chunked": b"POST /x HTTP/1.1\r\nHost: example.test\r\n"
            b"Transfer-Encoding: gzip\r\n\r\n" + smuggled,
            "space_before_colon": b"GET /x HTTP/1.1\r\nHost: example.test\r\n"
            b"X-Bad : y\r\n\r\n" + smuggled,
        }
        before = len(origin.requests)
        statuses = {}
        for name, payload in payloads.items():
            raw = raw_exchange(main_port, payload, wait=0.5)
            statuses[name] = raw.split(b"\r\n", 1)[0].decode("latin1") if raw else "<no response>"
        after = origin.requests[before:]
        leaked = [r for r in after if r["path"] == "/smuggled"]
        ok = all(st.startswith("HTTP/1.1 400") for st in statuses.values()) and not leaked
        record(
            "S3_request_smuggling_rejected",
            ok,
            f"statuses={statuses} smuggled_reached_origin={len(leaked)}",
            {"statuses": statuses, "origin_saw": after},
            critical=True,
        )

        # --- S4: header injection / obs-fold --------------------------------
        inject = (
            b"GET /x HTTP/1.1\r\nHost: example.test\r\n"
            b"X-Inject: a\rInjected: yes\r\n\r\n"
        )
        fold = b"GET /x HTTP/1.1\r\nHost: example.test\r\nX-Fold: a\r\n b\r\n\r\n"
        before = len(origin.requests)
        r1 = raw_exchange(main_port, inject, wait=0.5)
        r2 = raw_exchange(main_port, fold, wait=0.5)
        seen = origin.requests[before:]
        injected = [r for r in seen if "injected" in r["headers"]]
        status_inject = r1.split(b"\r\n", 1)[0] if r1 else b"<no response>"
        status_fold = r2.split(b"\r\n", 1)[0] if r2 else b"<no response>"
        ok = (
            status_inject.startswith(b"HTTP/1.1 400")
            and status_fold.startswith(b"HTTP/1.1 400")
            and not injected
        )
        record(
            "S4_header_injection_and_obs_fold_rejected",
            ok,
            f"inject_status={status_inject!r} fold_status={status_fold!r} "
            f"injected_reached_origin={len(injected)}",
            {"injected_reached_origin": injected},
            critical=True,
        )

        # --- S5: path normalization / traversal -----------------------------
        before = len(origin.requests)
        s = connect(main_port)
        s.sendall(
            b"GET /public/../admin HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n"
        )
        norm = read_response(s)
        s.close()
        traversal = raw_exchange(
            main_port,
            b"GET /../etc HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n",
            wait=0.5,
        )
        encoded = raw_exchange(
            main_port,
            b"GET /%2e%2e/admin HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n",
            wait=0.5,
        )
        paths = [r["path"] for r in origin.requests[before:]]
        ok = (
            "/admin" in paths
            and traversal.split(b"\r\n", 1)[0].startswith(b"HTTP/1.1 400")
            and encoded.split(b"\r\n", 1)[0].startswith(b"HTTP/1.1 400")
        )
        record(
            "S5_path_normalization_and_traversal",
            ok,
            f"origin_paths={paths} traversal={traversal.split(b'\r\n',1)[0]!r} encoded={encoded.split(b'\r\n',1)[0]!r}",
            {"origin_paths": paths},
        )

        # --- S6: Host enforcement -------------------------------------------
        missing = raw_exchange(
            main_port, b"GET /x HTTP/1.1\r\nConnection: close\r\n\r\n", wait=0.5
        )
        duplicate = raw_exchange(
            main_port,
            b"GET /x HTTP/1.1\r\nHost: a.test\r\nHost: b.test\r\nConnection: close\r\n\r\n",
            wait=0.5,
        )
        ok = missing.split(b"\r\n", 1)[0].startswith(b"HTTP/1.1 400") and duplicate.split(
            b"\r\n", 1
        )[0].startswith(b"HTTP/1.1 400")
        record(
            "S6_host_header_enforcement",
            ok,
            f"missing={missing.split(b'\r\n',1)[0]!r} duplicate={duplicate.split(b'\r\n',1)[0]!r}",
        )

        # --- S7: Vary isolation ---------------------------------------------
        def get_vary(accept):
            s = connect(main_port)
            s.sendall(
                b"GET /vary HTTP/1.1\r\nHost: example.test\r\nAccept-Encoding: "
                + accept
                + b"\r\nConnection: close\r\n\r\n"
            )
            r = read_response(s)
            s.close()
            return r

        g1 = get_vary(b"gzip")
        i1 = get_vary(b"identity")
        g2 = get_vary(b"gzip")
        i2 = get_vary(b"identity")
        ok = (
            g1["body"] == b"VARY-gzip"
            and i1["body"] == b"VARY-identity"
            and g2["body"] == b"VARY-gzip"
            and i2["body"] == b"VARY-identity"
            and g2["headers"].get("x-cache") == "HIT"
            and i2["headers"].get("x-cache") == "HIT"
        )
        record(
            "S7_vary_variants_isolated",
            ok,
            f"gzip={g1['body']!r}/{g2['body']!r} identity={i1['body']!r}/{i2['body']!r} "
            f"x-cache={[g1['headers'].get('x-cache'), i1['headers'].get('x-cache'), g2['headers'].get('x-cache'), i2['headers'].get('x-cache')]}",
        )

        # --- S8: HEAD served from a GET entry --------------------------------
        s = connect(main_port)
        s.sendall(b"GET /head HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        g = read_response(s)
        s.close()
        s = connect(main_port)
        s.sendall(b"HEAD /head HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        h = read_response(s)
        s.close()
        ok = (
            g["body"] == b"HELLO-WORLD"
            and h["status"] == 200
            and h["body"] == b""
            and h["headers"].get("content-length") == "11"
            and h["headers"].get("x-cache") == "HIT"
        )
        record(
            "S8_head_served_from_get_entry",
            ok,
            f"GET={g['body']!r} HEAD status={h['status']} body={h['body']!r} "
            f"CL={h['headers'].get('content-length')} x-cache={h['headers'].get('x-cache')}",
        )

        # --- S9: backend 204 + Transfer-Encoding desync (candidate) ----------
        first_raw = raw_exchange(
            desync_port,
            b"GET /desync HTTP/1.1\r\nHost: example.test\r\nConnection: keep-alive\r\n\r\n",
            wait=0.5,
        )
        time.sleep(1.6)  # let the illegal chunk terminator reach the proxy
        s = connect(desync_port)
        s.sendall(b"GET /normal HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
        second = read_response(s)
        s.close()
        first_status = (
            int(first_raw.split(b" ")[1]) if first_raw.startswith(b"HTTP/1.1 ") else 0
        )
        ok = second["status"] == 200 and second["body"] == b"NORMAL2"
        record(
            "S9_backend_204_with_transfer_encoding_does_not_desync",
            ok,
            f"first_status={first_status} second_status={second['status']} "
            f"second_body={second['body']!r} "
            f"forged_served={second['body'] == b'SMUGGLED-RESPONSE'}",
            {
                "first_raw": first_raw.decode("latin1", "replace"),
                "second": {"status": second["status"], "headers": second["headers"],
                           "body": second["body"].decode("latin1", "replace")},
            },
        )

        # --- S10: h2c hop-by-hop leak (RFC 9113 §8.2.2) ----------------------
        try:
            proc_h2 = subprocess.run(
                [
                    "curl",
                    "--http2-prior-knowledge",
                    "-s",
                    "-D",
                    "-",
                    "-o",
                    "/dev/null",
                    "--max-time",
                    "8",
                    f"http://127.0.0.1:{main_port}/hop",
                ],
                capture_output=True,
                text=True,
                timeout=12,
            )
            h2_ok_support = proc_h2.returncode == 0
            headers_text = proc_h2.stdout.lower()
        except (subprocess.TimeoutExpired, FileNotFoundError) as exc:
            h2_ok_support = False
            headers_text = f"<curl failed: {exc}>"

        if not h2_ok_support:
            record("S10_h2_connection_nominated_header_stripped", False, "BLOCKED: curl h2c unavailable")
        else:
            leaked = "x-secret: leaked" in headers_text
            record(
                "S10_h2_connection_nominated_header_stripped",
                not leaked,
                f"h2 response headers leak Connection-nominated X-Secret={leaked}",
                {"curl_headers": headers_text},
            )

        # --- S11: basic h2c translation correctness --------------------------
        if h2_ok_support:
            proc_h2b = subprocess.run(
                [
                    "curl",
                    "--http2-prior-knowledge",
                    "-s",
                    "--max-time",
                    "8",
                    f"http://127.0.0.1:{main_port}/normal",
                ],
                capture_output=True,
                text=True,
                timeout=12,
            )
            record(
                "S11_h2c_basic_translation",
                proc_h2b.returncode == 0 and proc_h2b.stdout == "NORMAL-OK",
                f"rc={proc_h2b.returncode} body={proc_h2b.stdout!r}",
            )
        else:
            record("S11_h2c_basic_translation", False, "BLOCKED: curl h2c unavailable")

        # --- S12: raw h2c request frames, including Connection nomination ----
        # A conforming h2 client removes Connection before sending (RFC 7540
        # §8.1.2.2), so curl cannot exercise this path. These frames are legal
        # HPACK/frame syntax with a permissive peer's headers; the origin is
        # the witness. Case 1 nominates x-evil; case 2 also nominates
        # content-length. Each path is unique so the proxy cache cannot hide
        # the forwarding.
        control_path = "/echo?s12=control"
        try:
            control_raw, control_seen = raw_h2_request(
                main_port,
                control_path,
                f"127.0.0.1:{main_port}",
                [("x-evil", "control-visible"), ("x-keep", "yes")],
                origin,
                timeout=3.0,
            )
            control_headers = control_seen["headers"] if control_seen else {}
            control_ok = (
                control_seen is not None
                and control_headers.get("x-evil") == "control-visible"
                and control_headers.get("x-keep") == "yes"
            )
            control_error = None
        except OSError as exc:
            control_raw = b""
            control_headers = {}
            control_ok = False
            control_error = str(exc)

        s12_cases = [
            (
                "connection: x-evil",
                "/echo?s12=nomination",
                [
                    ("connection", "x-evil"),
                    ("x-evil", "smuggled-h2"),
                    ("x-keep", "yes"),
                ],
                ["connection", "x-evil"],
            ),
            (
                "connection: x-evil, content-length",
                "/echo?s12=nomination-cl",
                [
                    ("connection", "x-evil, content-length"),
                    ("x-evil", "smuggled-h2"),
                    ("content-length", "0"),
                    ("x-keep", "yes"),
                ],
                ["connection", "x-evil", "content-length"],
            ),
        ]
        s12_evidence = []
        filter_verified = True
        protocol_rejected = 0
        for case_name, path, extra, forbidden in s12_cases:
            try:
                raw, seen = raw_h2_request(
                    main_port,
                    path,
                    f"127.0.0.1:{main_port}",
                    extra,
                    origin,
                    timeout=3.0,
                )
                headers = seen["headers"] if seen is not None else {}
                leaked = [name for name in forbidden if name in headers]
                kept = headers.get("x-keep") == "yes"
                frames = parse_h2_frames(raw)
                rst = [
                    frame
                    for frame in frames
                    if frame.get("type") == "RST_STREAM"
                    and frame.get("stream") == 1
                    and frame.get("error_code") == 1
                ]
                if seen is not None:
                    verdict = "FILTER_VERIFIED" if not leaked and kept else "LEAK"
                    case_ok = not leaked and kept
                    filter_verified = filter_verified and case_ok
                elif rst:
                    verdict = "PROTOCOL_REJECTED"
                    case_ok = not leaked
                    protocol_rejected += 1
                    filter_verified = False
                else:
                    verdict = "NOT_FORWARDED"
                    case_ok = False
                    filter_verified = False
                error = None
            except OSError as exc:
                raw = b""
                seen = None
                headers = {}
                leaked = list(forbidden)
                kept = False
                frames = []
                verdict = "PROBE_ERROR"
                case_ok = False
                filter_verified = False
                error = str(exc)
            s12_evidence.append(
                {
                    "case": case_name,
                    "path": path,
                    "ok": case_ok,
                    "verdict": verdict,
                    "origin_saw_request": seen is not None,
                    "origin_headers": headers,
                    "forbidden_present": leaked,
                    "normal_header_kept": kept,
                    "server_frames": frames,
                    "raw_server_bytes_hex": raw[:4096].hex(),
                    "error": error,
                }
            )
        if not control_ok:
            verdict = "PROBE_INVALID"
        elif filter_verified:
            verdict = "FILTER_VERIFIED"
        elif protocol_rejected == len(s12_cases):
            verdict = "PROTOCOL_REJECTED"
        else:
            verdict = "MIXED"
        s12_ok = control_ok and all(item["ok"] for item in s12_evidence)
        detail_cases = [
            {
                "case": item["case"],
                "ok": item["ok"],
                "verdict": item["verdict"],
                "origin_saw_request": item["origin_saw_request"],
                "forbidden_present": item["forbidden_present"],
                "normal_header_kept": item["normal_header_kept"],
            }
            for item in s12_evidence
        ]
        record(
            "S12_h2_request_connection_nominated_header",
            s12_ok,
            f"raw h2c verdict={verdict} control={control_ok} cases={detail_cases}",
            {
                "verdict": verdict,
                "control": {
                    "path": control_path,
                    "ok": control_ok,
                    "origin_headers": control_headers,
                    "raw_server_bytes_hex": control_raw[:4096].hex(),
                    "error": control_error,
                },
                "cases": s12_evidence,
            },
        )

        # --- S12 h1: legal Connection nomination on the h1c listener --------
        # HTTP/1.1 is an accepted codec and Connection is legal there. The
        # h1 parser has its own nominated-header sanitizer; this is the path a
        # conforming client can really produce. The second case names a
        # framing header, which the h1 parser deliberately rejects with 400.
        def h1_payload(path: str, extra_headers: list[tuple[str, str]]) -> bytes:
            lines = [f"GET {path} HTTP/1.1", f"Host: 127.0.0.1:{main_port}"]
            lines.extend(f"{name}: {value}" for name, value in extra_headers)
            return ("\r\n".join(lines) + "\r\n\r\n").encode()

        def origin_request(path: str):
            for request in list(origin.requests):
                if request["path"] == path:
                    return request
            return None

        # Regression for the h2c → h1 pool hand-off: the first HTTP/1.1
        # request after an h2c exchange must not inherit a backend connection
        # the origin already closed. A 502 here is the bug, so this is an
        # assertion, not a throwaway warm-up.
        h1_preflight_path = "/s12-h1-preflight"
        h1_preflight_raw = raw_exchange(
            main_port,
            h1_payload(h1_preflight_path, [("x-keep", "preflight")]),
            wait=0.5,
        )
        h1_preflight_seen = origin_request(h1_preflight_path)
        h1_preflight_headers = (
            h1_preflight_seen["headers"] if h1_preflight_seen else {}
        )
        h1_preflight_status = (
            int(h1_preflight_raw.split(b" ")[1])
            if h1_preflight_raw.startswith(b"HTTP/1.1 ")
            and len(h1_preflight_raw.split(b" ")) > 1
            else 0
        )
        h1_preflight_ok = (
            h1_preflight_status == 200
            and h1_preflight_headers.get("x-keep") == "preflight"
        )

        h1_control_path = "/s12-h1-control"
        h1_control_raw = raw_exchange(
            main_port,
            h1_payload(h1_control_path, [("x-keep", "yes")]),
            wait=0.5,
        )
        h1_control_seen = origin_request(h1_control_path)
        h1_control_headers = h1_control_seen["headers"] if h1_control_seen else {}
        h1_control_status = (
            int(h1_control_raw.split(b" ")[1])
            if h1_control_raw.startswith(b"HTTP/1.1 ")
            and len(h1_control_raw.split(b" ")) > 1
            else 0
        )
        h1_control_ok = (
            h1_control_seen is not None
            and h1_control_headers.get("x-keep") == "yes"
            and h1_control_status == 200
        )

        h1_cases = [
            (
                "h1 connection: x-evil",
                "/echo?s12=h1-nomination",
                [
                    ("Connection", "x-evil"),
                    ("x-evil", "smuggled-h1"),
                    ("x-keep", "yes"),
                ],
                ["connection", "x-evil"],
                "filter",
            ),
            (
                "h1 connection: x-evil, content-length",
                "/echo?s12=h1-nomination-cl",
                [
                    ("Connection", "x-evil, content-length"),
                    ("x-evil", "smuggled-h1"),
                    ("Content-Length", "0"),
                    ("x-keep", "yes"),
                ],
                ["connection", "x-evil", "content-length"],
                "reject",
            ),
        ]
        h1_evidence = []
        h1_ok = h1_preflight_ok and h1_control_ok
        h1_filter_verified = False
        h1_rejected = 0
        for case_name, path, extra, forbidden, expectation in h1_cases:
            raw = raw_exchange(main_port, h1_payload(path, extra), wait=0.5)
            seen = origin_request(path)
            headers = seen["headers"] if seen else {}
            leaked = [name for name in forbidden if name in headers]
            kept = headers.get("x-keep") == "yes"
            status = (
                int(raw.split(b" ")[1])
                if raw.startswith(b"HTTP/1.1 ") and len(raw.split(b" ")) > 1
                else 0
            )
            if seen is not None:
                verdict = "FILTER_VERIFIED" if not leaked and kept else "LEAK"
                case_ok = not leaked and kept
                h1_filter_verified = h1_filter_verified or case_ok
            elif expectation == "reject" and status == 400:
                verdict = "PROTOCOL_REJECTED"
                case_ok = True
                h1_rejected += 1
            else:
                verdict = "NOT_FORWARDED"
                case_ok = False
            h1_ok = h1_ok and case_ok
            h1_evidence.append(
                {
                    "case": case_name,
                    "path": path,
                    "ok": case_ok,
                    "verdict": verdict,
                    "origin_saw_request": seen is not None,
                    "origin_headers": headers,
                    "forbidden_present": leaked,
                    "normal_header_kept": kept,
                    "http_status": status,
                    "raw_server_bytes_hex": raw[:4096].hex(),
                }
            )
        if not h1_control_ok:
            h1_verdict = "PROBE_INVALID"
        elif not h1_preflight_ok:
            h1_verdict = "POOL_REUSE_REGRESSION"
        elif h1_filter_verified and h1_rejected:
            h1_verdict = "FILTER_VERIFIED_H1_REJECTS_FRAMING"
        elif h1_filter_verified:
            h1_verdict = "FILTER_VERIFIED"
        elif h1_rejected == len(h1_cases):
            h1_verdict = "PROTOCOL_REJECTED"
        else:
            h1_verdict = "MIXED"
        record(
            "S12_h1_request_connection_nominated_header",
            h1_ok,
            f"h1 verdict={h1_verdict} preflight={h1_preflight_ok} control={h1_control_ok} cases={h1_evidence}",
            {
                "verdict": h1_verdict,
                "preflight": {
                    "path": h1_preflight_path,
                    "ok": h1_preflight_ok,
                    "http_status": h1_preflight_status,
                    "origin_headers": h1_preflight_headers,
                    "raw_server_bytes_hex": h1_preflight_raw[:4096].hex(),
                },
                "control": {
                    "path": h1_control_path,
                    "ok": h1_control_ok,
                    "http_status": h1_control_status,
                    "origin_headers": h1_control_headers,
                    "raw_server_bytes_hex": h1_control_raw[:4096].hex(),
                },
                "cases": h1_evidence,
            },
        )
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        log.close()

    (LAB / "results.json").write_text(json.dumps(results, indent=2))
    failed = [r for r in results if not r["ok"]]
    print()
    print(f"{len(results) - len(failed)}/{len(results)} scenarios passed")
    for r in failed:
        print(("CRITICAL " if r["critical"] else "         ") + r["name"] + ": " + r["detail"])
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
