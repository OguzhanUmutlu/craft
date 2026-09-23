#!/usr/bin/env python3
"""
Craft Studio DevTools Automation & Screenshot Harness
Connects to the Chrome DevTools Protocol (CDP) WebSocket endpoint exposed
by the desktop studio webview to enable headless window capture, dynamic JavaScript
evaluation, console log streaming, and performance profiling.

Zero external dependencies - implemented using Python standard library (RFC 6455).
Strictly compliant with workspace Zero-Emoji policy.
"""

import sys
import os
import time
import json
import base64
import socket
import struct
import hashlib
import argparse
import subprocess
import urllib.request
import urllib.error
from pathlib import Path

# Base paths
PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
SCREENSHOTS_DIR = PROJECT_ROOT / "screenshots"
PID_FILE = PROJECT_ROOT / "tools" / "devtools" / ".devtools_app.pid"
DEFAULT_CDP_PORT = 9333


class SimpleWebSocketClient:
    """Minimal RFC 6455 WebSocket client using standard library sockets."""

    def __init__(self, ws_url: str, timeout: float = 10.0):
        # ws://host:port/path
        if not ws_url.startswith("ws://"):
            raise ValueError(f"Only ws:// scheme is supported: {ws_url}")

        url_part = ws_url[5:]
        host_port, _, self.path = url_part.partition("/")
        self.path = "/" + self.path

        if ":" in host_port:
            self.host, port_str = host_port.split(":")
            self.port = int(port_str)
        else:
            self.host = host_port
            self.port = 80

        self.timeout = timeout
        self.sock = None
        self._connect()

    def _connect(self):
        self.sock = socket.create_connection((self.host, self.port), timeout=self.timeout)
        key = base64.b64encode(os.urandom(16)).decode("ascii")

        req = (
            f"GET {self.path} HTTP/1.1\r\n"
            f"Host: {self.host}:{self.port}\r\n"
            f"Upgrade: websocket\r\n"
            f"Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            f"Sec-WebSocket-Version: 13\r\n\r\n"
        )
        self.sock.sendall(req.encode("ascii"))

        # Read handshake response
        response = b""
        while b"\r\n\r\n" not in response:
            chunk = self.sock.recv(1024)
            if not chunk:
                raise ConnectionError("Server closed connection during WebSocket handshake.")
            response += chunk

        status_line = response.split(b"\r\n")[0].decode("ascii", errors="replace")
        if "101" not in status_line:
            raise ConnectionError(f"WebSocket handshake failed: {status_line}")

    def send_text(self, text: str):
        payload = text.encode("utf-8")
        mask_key = os.urandom(4)
        length = len(payload)

        # Fin + Text Opcode (0x81)
        header = bytearray([0x81])

        # Masked bit (0x80) | length
        if length <= 125:
            header.append(0x80 | length)
        elif length <= 65535:
            header.append(0x80 | 126)
            header.extend(struct.pack("!H", length))
        else:
            header.append(0x80 | 127)
            header.extend(struct.pack("!Q", length))

        header.extend(mask_key)

        # Mask payload
        masked_payload = bytearray(length)
        for i in range(length):
            masked_payload[i] = payload[i] ^ mask_key[i % 4]

        self.sock.sendall(header + masked_payload)

    def recv_text(self) -> str:
        # Read frame header
        header = self._recv_exact(2)
        b1, b2 = header[0], header[1]

        opcode = b1 & 0x0F
        masked = bool(b2 & 0x80)
        length = b2 & 0x7F

        if length == 126:
            ext_len = self._recv_exact(2)
            length = struct.unpack("!H", ext_len)[0]
        elif length == 127:
            ext_len = self._recv_exact(8)
            length = struct.unpack("!Q", ext_len)[0]

        mask = None
        if masked:
            mask = self._recv_exact(4)

        payload = self._recv_exact(length)
        if masked and mask:
            unmasked = bytearray(length)
            for i in range(length):
                unmasked[i] = payload[i] ^ mask[i % 4]
            payload = unmasked

        if opcode == 0x8:  # Connection Close
            raise ConnectionResetError("Remote endpoint closed WebSocket connection.")
        if opcode == 0x9:  # Ping
            # Echo as Pong
            pong_header = bytearray([0x8A, 0x00])
            self.sock.sendall(pong_header)
            return self.recv_text()

        return payload.decode("utf-8", errors="replace")

    def _recv_exact(self, count: int) -> bytes:
        data = bytearray()
        while len(data) < count:
            chunk = self.sock.recv(count - len(data))
            if not chunk:
                raise ConnectionResetError("Socket closed prematurely.")
            data.extend(chunk)
        return bytes(data)

    def close(self):
        if self.sock:
            try:
                # Send close frame
                self.sock.sendall(bytearray([0x88, 0x00]))
                self.sock.close()
            except Exception:
                pass
            self.sock = None


class DevToolsController:
    """Coordinates Chrome DevTools Protocol operations for Craft Desktop Studio."""

    def __init__(self, port: int = DEFAULT_CDP_PORT):
        self.port = port
        self.msg_id = 0

    def _next_id(self) -> int:
        self.msg_id += 1
        return self.msg_id

    def get_target_ws_url(self) -> str:
        """Retrieves the active webview target WebSocket debugger URL."""
        endpoint = f"http://127.0.0.1:{self.port}/json/list"
        try:
            req = urllib.request.Request(endpoint, headers={"User-Agent": "Craft-DevTools/1.0"})
            with urllib.request.urlopen(req, timeout=3.0) as resp:
                targets = json.loads(resp.read().decode("utf-8"))
        except Exception as e:
            raise RuntimeError(
                f"[ERROR] Unable to reach DevTools HTTP endpoint at 127.0.0.1:{self.port}. "
                f"Ensure the app is running with --remote-debugging-port={self.port}. ({e})"
            )

        if not targets:
            raise RuntimeError(f"[ERROR] No active webview targets found at 127.0.0.1:{self.port}.")

        # Select first page or studio target
        for target in targets:
            ws_url = target.get("webSocketDebuggerUrl")
            if ws_url and target.get("type") in ("page", "webview", "iframe"):
                return ws_url

        if "webSocketDebuggerUrl" in targets[0]:
            return targets[0]["webSocketDebuggerUrl"]

        raise RuntimeError(f"[ERROR] No webSocketDebuggerUrl present in target: {targets[0]}")

    def capture_screenshot(self, output_name: str = "studio_capture.png") -> Path:
        """Captures a full window screenshot via CDP even if the window is in the background."""
        SCREENSHOTS_DIR.mkdir(parents=True, exist_ok=True)
        if not output_name.endswith(".png"):
            output_name += ".png"
        output_path = SCREENSHOTS_DIR / output_name

        ws_url = self.get_target_ws_url()
        client = SimpleWebSocketClient(ws_url, timeout=15.0)

        try:
            req_id = self._next_id()
            cmd = json.dumps({
                "id": req_id,
                "method": "Page.captureScreenshot",
                "params": {"format": "png", "captureBeyondViewport": True}
            })
            client.send_text(cmd)

            # Wait for response with matching id
            while True:
                msg = client.recv_text()
                data = json.loads(msg)
                if data.get("id") == req_id:
                    if "error" in data:
                        raise RuntimeError(f"[ERROR] CDP captureScreenshot failed: {data['error']}")
                    img_b64 = data["result"]["data"]
                    img_bytes = base64.b64decode(img_b64)
                    output_path.write_bytes(img_bytes)
                    print(f"[OK] Screenshot captured successfully ({len(img_bytes):,} bytes): {output_path}")
                    return output_path
        finally:
            client.close()

    def evaluate_javascript(self, expression: str) -> any:
        """Executes arbitrary JavaScript in the desktop studio frontend."""
        ws_url = self.get_target_ws_url()
        client = SimpleWebSocketClient(ws_url, timeout=10.0)

        try:
            req_id = self._next_id()
            cmd = json.dumps({
                "id": req_id,
                "method": "Runtime.evaluate",
                "params": {
                    "expression": expression,
                    "returnByValue": True,
                    "awaitPromise": True
                }
            })
            client.send_text(cmd)

            while True:
                msg = client.recv_text()
                data = json.loads(msg)
                if data.get("id") == req_id:
                    if "error" in data:
                        raise RuntimeError(f"[ERROR] Evaluation error: {data['error']}")
                    result = data.get("result", {}).get("result", {})
                    val = result.get("value")
                    desc = result.get("description", str(val))
                    print(f"[OK] JS Evaluation Result: {desc}")
                    return val
        finally:
            client.close()

    def stream_console_logs(self, duration_secs: float = 5.0):
        """Streams live console logs from the desktop studio webview."""
        ws_url = self.get_target_ws_url()
        client = SimpleWebSocketClient(ws_url, timeout=duration_secs + 2.0)

        try:
            # Enable Runtime and Log domains
            client.send_text(json.dumps({"id": self._next_id(), "method": "Runtime.enable"}))
            client.send_text(json.dumps({"id": self._next_id(), "method": "Log.enable"}))

            print(f"[INFO] Streaming webview console logs for {duration_secs:.1f}s...")
            start_time = time.time()
            log_count = 0

            while time.time() - start_time < duration_secs:
                try:
                    msg = client.recv_text()
                    data = json.loads(msg)
                    method = data.get("method")
                    if method == "Runtime.consoleAPICalled":
                        params = data.get("params", {})
                        log_type = params.get("type", "log").upper()
                        args = [str(a.get("value", a.get("description", ""))) for a in params.get("args", [])]
                        print(f"[{log_type}] {' '.join(args)}")
                        log_count += 1
                    elif method == "Runtime.exceptionThrown":
                        details = data.get("params", {}).get("exceptionDetails", {})
                        text = details.get("text", "Unknown Exception")
                        print(f"[EXCEPTION] {text}")
                        log_count += 1
                except (socket.timeout, TimeoutError):
                    break

            print(f"[OK] Console stream completed. Captured {log_count} log frame(s).")
        finally:
            client.close()

    def profile_performance(self):
        """Extracts frontend layout, heap, and frame timing metrics via CDP."""
        ws_url = self.get_target_ws_url()
        client = SimpleWebSocketClient(ws_url, timeout=10.0)

        try:
            client.send_text(json.dumps({"id": self._next_id(), "method": "Performance.enable"}))
            req_id = self._next_id()
            client.send_text(json.dumps({"id": req_id, "method": "Performance.getMetrics"}))

            while True:
                msg = client.recv_text()
                data = json.loads(msg)
                if data.get("id") == req_id:
                    metrics = data.get("result", {}).get("metrics", [])
                    print("\n=== Frontend Performance Metrics ===")
                    for m in metrics:
                        name = m.get("name")
                        val = m.get("value")
                        if "Duration" in name:
                            print(f"  {name:30}: {val * 1000.0:10.2f} ms")
                        elif "Bytes" in name or "Size" in name:
                            print(f"  {name:30}: {val / (1024 * 1024):10.2f} MB")
                        else:
                            print(f"  {name:30}: {val:10.2f}")
                    print("====================================\n")
                    return metrics
        finally:
            client.close()

    def set_viewport(self, width: int, height: int):
        """Sets the browser viewport dimensions via Emulation.setDeviceMetricsOverride."""
        ws_url = self.get_target_ws_url()
        client = SimpleWebSocketClient(ws_url, timeout=10.0)
        try:
            req_id = self._next_id()
            cmd = json.dumps({
                "id": req_id,
                "method": "Emulation.setDeviceMetricsOverride",
                "params": {
                    "width": width,
                    "height": height,
                    "deviceScaleFactor": 1,
                    "mobile": False,
                }
            })
            client.send_text(cmd)
            while True:
                msg = client.recv_text()
                data = json.loads(msg)
                if data.get("id") == req_id:
                    print(f"[OK] Viewport set to {width}x{height}.")
                    return data
        finally:
            client.close()

    def audit_zero_emoji(self) -> list:
        """Audits the rendered DOM to strictly verify zero emojis exist anywhere."""
        script = """
        (() => {
            const emojiRegex = /(\\p{Extended_Pictographic}|\\p{Emoji_Presentation})/u;
            const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
            const violations = [];
            let node;
            while (node = walker.nextNode()) {
                const text = (node.nodeValue || '').trim();
                if (text && emojiRegex.test(text)) {
                    violations.push({
                        text: text.slice(0, 50),
                        tag: node.parentElement ? node.parentElement.tagName.toLowerCase() : 'unknown'
                    });
                }
            }
            return violations;
        })()
        """
        return self.evaluate_javascript(script) or []

    def get_dom_node_count(self) -> int:
        """Returns the total number of DOM elements in the document."""
        res = self.evaluate_javascript("document.querySelectorAll('*').length")
        return int(res) if res is not None else 0

    def wait_for_selector(self, selector: str, timeout_secs: float = 5.0) -> bool:
        """Polls until a DOM element matching selector exists or timeout expires."""
        start = time.time()
        while time.time() - start < timeout_secs:
            res = self.evaluate_javascript(f"!!document.querySelector('{selector}')")
            if res:
                return True
            time.sleep(0.2)
        return False



def cmd_start(args):
    """Starts Vite dev server and desktop studio application with DevTools remote debugging."""
    ui_dir = PROJECT_ROOT / "crates" / "ui"
    if not ui_dir.exists():
        print(f"[ERROR] UI directory not found at: {ui_dir}")
        return 1

    print(f"[INFO] Launching Craft Studio Dev Server from {ui_dir}...")
    log_file = PROJECT_ROOT / "tools" / "devtools" / "dev_server.log"
    out = open(log_file, "w")

    vite_proc = subprocess.Popen(
        ["npm", "run", "dev", "--", "--port", "5173", "--strictPort"],
        cwd=str(ui_dir),
        stdout=out,
        stderr=out,
        start_new_session=True
    )
    pids = [vite_proc.pid]
    print(f"[OK] Dev server process launched with PID {vite_proc.pid}.")
    time.sleep(2)

    # Launch browser/webview pointing to the dev server with remote debugging port
    chrome_bins = ["/usr/bin/google-chrome", "google-chrome", "chromium", "chromium-browser"]
    chrome_bin = None
    for b in chrome_bins:
        if subprocess.call(["which", b], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) == 0:
            chrome_bin = b
            break

    if chrome_bin:
        print(f"[INFO] Launching webview instance using {chrome_bin} with CDP on port {args.port}...")
        chrome_proc = subprocess.Popen(
            [
                chrome_bin,
                "--headless=new",
                "--no-sandbox",
                "--disable-gpu",
                f"--user-data-dir=/tmp/craft_devtools_{args.port}",
                f"--remote-debugging-port={args.port}",
                "--window-size=1280,800",
                "http://localhost:5173"
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True
        )
        pids.append(chrome_proc.pid)
        print(f"[OK] Webview process launched with PID {chrome_proc.pid}.")
        time.sleep(2)

    PID_FILE.write_text(json.dumps(pids))
    print(f"[OK] Studio environment running. DevTools available at http://127.0.0.1:{args.port}/json")
    return 0


def cmd_stop(args):
    """Stops all running dev server and desktop studio processes tracked by PID file."""
    if not PID_FILE.exists():
        print("[INFO] No active app PID file found.")
        return 0

    try:
        content = PID_FILE.read_text().strip()
        try:
            pids = json.loads(content)
            if not isinstance(pids, list):
                pids = [int(content)]
        except Exception:
            pids = [int(content)]

        for pid in pids:
            print(f"[INFO] Terminating process PID {pid}...")
            try:
                os.kill(pid, 15)  # SIGTERM
            except ProcessLookupError:
                pass
        time.sleep(1)
        PID_FILE.unlink(missing_ok=True)
        print("[OK] Stopped successfully.")
    except Exception as e:
        print(f"[WARN] Error stopping processes: {e}")
    return 0


def cmd_status(args):
    """Checks the status of the DevTools CDP endpoint and target webview."""
    controller = DevToolsController(port=args.port)
    try:
        ws_url = controller.get_target_ws_url()
        print(f"[ONLINE] DevTools CDP endpoint active on port {args.port}")
        print(f"[TARGET] {ws_url}")
        return 0
    except Exception as e:
        print(f"[OFFLINE] DevTools CDP endpoint unreachable: {e}")
        return 1


def cmd_screenshot(args):
    controller = DevToolsController(port=args.port)
    try:
        controller.capture_screenshot(output_name=args.output)
        return 0
    except Exception as e:
        print(f"[ERROR] Screenshot failed: {e}")
        return 1


def cmd_eval(args):
    controller = DevToolsController(port=args.port)
    try:
        controller.evaluate_javascript(args.expression)
        return 0
    except Exception as e:
        print(f"[ERROR] Evaluation failed: {e}")
        return 1


def cmd_logs(args):
    controller = DevToolsController(port=args.port)
    try:
        controller.stream_console_logs(duration_secs=args.duration)
        return 0
    except Exception as e:
        print(f"[ERROR] Log stream failed: {e}")
        return 1


def cmd_profile(args):
    controller = DevToolsController(port=args.port)
    try:
        controller.profile_performance()
        return 0
    except Exception as e:
        print(f"[ERROR] Profiling failed: {e}")
        return 1


def main():
    parser = argparse.ArgumentParser(description="Craft Desktop Studio DevTools Automation Harness")
    parser.add_argument("--port", type=int, default=DEFAULT_CDP_PORT, help="CDP remote debugging port (default 9222)")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # Subcommands
    sub_start = subparsers.add_parser("start", help="Start the UI dev server")
    sub_start.set_defaults(func=cmd_start)

    sub_stop = subparsers.add_parser("stop", help="Stop the UI dev server")
    sub_stop.set_defaults(func=cmd_stop)

    sub_status = subparsers.add_parser("status", help="Check DevTools connection status")
    sub_status.set_defaults(func=cmd_status)

    sub_snap = subparsers.add_parser("screenshot", help="Capture window screenshot to screenshots/")
    sub_snap.add_argument("--output", "-o", default="studio_capture.png", help="Output filename in screenshots/")
    sub_snap.set_defaults(func=cmd_screenshot)

    sub_eval = subparsers.add_parser("eval", help="Evaluate JavaScript in webview via CDP")
    sub_eval.add_argument("expression", help="JavaScript expression to execute")
    sub_eval.set_defaults(func=cmd_eval)

    sub_logs = subparsers.add_parser("logs", help="Stream webview console logs")
    sub_logs.add_argument("--duration", "-d", type=float, default=5.0, help="Stream duration in seconds")
    sub_logs.set_defaults(func=cmd_logs)

    sub_prof = subparsers.add_parser("profile", help="Extract performance metrics via CDP")
    sub_prof.set_defaults(func=cmd_profile)

    args = parser.parse_args()
    ret = args.func(args)
    sys.exit(ret)


if __name__ == "__main__":
    main()
