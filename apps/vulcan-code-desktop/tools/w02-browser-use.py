# W02 Browser Use deterministic verification.
# W02 Browser Use 确定性验证。
#
# This script runs one isolated Electron browser host, opens the host control channel exactly as
# Core does, and then exercises Browser Use's deterministic connection/session API over CDP with
# no model involved. It records success, failure, or unverified per case and never fabricates a
# pass. Model-backed agent tasks are deliberately out of scope here.
# 本脚本运行一个隔离的 Electron 浏览器宿主，按 Core 的方式建立控制通道，随后在不涉及任何模型
# 的情况下验证 Browser Use 的确定性连接与会话 API。它逐项记录成功、失败或未验证，绝不伪造通过。
# 依赖模型的代理任务有意不在本脚本范围内。

from __future__ import annotations

import argparse
import asyncio
import json
import os
import socket
import struct
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

CONTROL_ACCEPT_TIMEOUT_SECONDS = 30.0
DISCOVERY_TIMEOUT_SECONDS = 30.0
CONTROL_FRAME_MAX_BYTES = 1024 * 1024


def encode_frame(value: object) -> bytes:
    """Encodes one JSON control frame with its big-endian length prefix."""
    body = json.dumps(value, separators=(",", ":")).encode("utf-8")
    if len(body) > CONTROL_FRAME_MAX_BYTES:
        raise ValueError("control frame exceeds the protocol limit")
    return struct.pack(">I", len(body)) + body


class ControlFrames:
    """Incremental decoder for length-prefixed control frames."""

    def __init__(self) -> None:
        self.pending = bytearray()

    def push(self, chunk: bytes) -> list[object]:
        """Appends bytes and returns every completed frame value."""
        self.pending.extend(chunk)
        values: list[object] = []
        while True:
            if len(self.pending) < 4:
                return values
            (length,) = struct.unpack(">I", bytes(self.pending[:4]))
            if length > CONTROL_FRAME_MAX_BYTES:
                raise ValueError("control frame exceeds the protocol limit")
            if len(self.pending) < 4 + length:
                return values
            body = bytes(self.pending[4 : 4 + length])
            del self.pending[: 4 + length]
            values.append(json.loads(body.decode("utf-8")))


class Results:
    """Collects per-case results."""

    def __init__(self) -> None:
        self.cases: list[dict[str, object]] = []

    def record(self, case_id: str, title: str, status: str, detail: str) -> None:
        """Records one measured case."""
        self.cases.append({"id": case_id, "title": title, "status": status, "detail": detail})
        marker = {"pass": "PASS", "fail": "FAIL"}.get(status, "UNVERIFIED")
        print(f"[w02:bu] {marker} {case_id} {title} :: {detail}", file=sys.stderr)


def wait_for_discovery(profile_directory: Path, timeout_seconds: float) -> dict[str, object]:
    """Waits for the host control discovery document."""
    path = profile_directory / "browser-host-control.json"
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        try:
            document = json.loads(path.read_text(encoding="utf-8"))
            if isinstance(document, dict) and int(document.get("controlPort", 0)) > 0:
                return document
        except (OSError, ValueError):
            pass
        time.sleep(0.05)
    raise TimeoutError("browser host did not publish its control discovery file")


def open_control_channel(
    document: dict[str, object], session_id: str, host_generation: str, secret: str
) -> tuple[socket.socket, dict[str, object]]:
    """Opens the authenticated control channel and returns the host handshake."""
    connection = socket.create_connection(
        (str(document["controlHost"]), int(document["controlPort"])), timeout=CONTROL_ACCEPT_TIMEOUT_SECONDS
    )
    connection.sendall(
        encode_frame(
            {
                "kind": "coreHello",
                "protocolVersion": int(document["protocolVersion"]),
                "connectionSecret": secret,
                "sessionId": session_id,
                "hostGeneration": host_generation,
            }
        )
    )
    decoder = ControlFrames()
    connection.settimeout(CONTROL_ACCEPT_TIMEOUT_SECONDS)
    deadline = time.monotonic() + CONTROL_ACCEPT_TIMEOUT_SECONDS
    while time.monotonic() < deadline:
        for value in decoder.push(connection.recv(65536)):
            if isinstance(value, dict) and "cdpEndpoint" in value:
                return connection, value
    raise TimeoutError("browser host did not send its handshake")


async def verify_browser_use(cdp_url: str, fixture_base_url: str, results: Results) -> None:
    """Exercises Browser Use's deterministic CDP session API without any model."""
    from browser_use import BrowserSession

    session = BrowserSession(cdp_url=cdp_url)
    page = None
    try:
        try:
            await session.start()
            results.record(
                "BU_CONNECT",
                "Browser Use connects to the existing host over CDP",
                "pass" if session.is_cdp_connected else "fail",
                f"cdp_url={cdp_url} is_cdp_connected={session.is_cdp_connected}",
            )
        except Exception as error:  # noqa: BLE001 - the harness records real failures
            results.record(
                "BU_CONNECT",
                "Browser Use connects to the existing host over CDP",
                "fail",
                f"{type(error).__name__}: {error}",
            )
            return

        try:
            pages = await session.get_pages()
            results.record(
                "BU_PAGE_ENUMERATION",
                "Browser Use enumerates existing host pages",
                "pass" if len(pages) >= 1 else "fail",
                f"pages={len(pages)}",
            )
        except Exception as error:  # noqa: BLE001
            results.record(
                "BU_PAGE_ENUMERATION",
                "Browser Use enumerates existing host pages",
                "fail",
                f"{type(error).__name__}: {error}",
            )

        try:
            page = await session.get_current_page()
            results.record(
                "BU_CURRENT_PAGE",
                "Browser Use resolves the current page",
                "pass" if page is not None else "fail",
                f"currentPageResolved={page is not None}",
            )
        except Exception as error:  # noqa: BLE001
            results.record(
                "BU_CURRENT_PAGE",
                "Browser Use resolves the current page",
                "fail",
                f"{type(error).__name__}: {error}",
            )

        if page is None:
            results.record(
                "BU_NAVIGATE_TITLE",
                "Browser Use navigates and reads the resulting title",
                "fail",
                "no current page was available",
            )
            results.record(
                "BU_EVALUATE_DOM",
                "Browser Use evaluates DOM state on the same page",
                "fail",
                "no current page was available",
            )
            return

        try:
            await page.navigate(f"{fixture_base_url}/w02-page.html")
            title = await page.get_title()
            results.record(
                "BU_NAVIGATE_TITLE",
                "Browser Use navigates and reads the resulting title",
                "pass" if "W02" in title else "fail",
                f"title={title}",
            )
        except Exception as error:  # noqa: BLE001
            results.record(
                "BU_NAVIGATE_TITLE",
                "Browser Use navigates and reads the resulting title",
                "fail",
                f"{type(error).__name__}: {error}",
            )

        try:
            # Browser Use requires the function form for evaluated JavaScript.
            # Browser Use 要求被求值的 JavaScript 使用函数形式。
            heading = await page.evaluate(
                "() => document.getElementById('fixture-title').textContent"
            )
            results.record(
                "BU_EVALUATE_DOM",
                "Browser Use evaluates DOM state on the same page",
                "pass" if "W02" in str(heading) else "fail",
                f"heading={heading}",
            )
        except Exception as error:  # noqa: BLE001
            results.record(
                "BU_EVALUATE_DOM",
                "Browser Use evaluates DOM state on the same page",
                "fail",
                f"{type(error).__name__}: {error}",
            )
    finally:
        try:
            await session.stop()
        except Exception:  # noqa: BLE001 - disconnect failures are reported by the host state
            pass


def main() -> int:
    """Runs the Browser Use deterministic verification."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--desktop-runtime", required=True)
    parser.add_argument("--desktop-entry", required=True)
    parser.add_argument("--evidence-dir", required=True)
    arguments = parser.parse_args()

    evidence_directory = Path(arguments.evidence_dir)
    evidence_directory.mkdir(parents=True, exist_ok=True)
    results = Results()

    isolated_root = Path(tempfile.mkdtemp(prefix="vulcan-w02-bu-"))
    profile_directory = isolated_root / "browser-profile"
    profile_directory.mkdir(parents=True, exist_ok=True)

    sys.path.insert(0, str(Path(__file__).resolve().parent / "fixtures"))
    from w02_fixture_server import start_fixture_server  # noqa: PLC0415 - local test helper

    server, fixture_base_url = start_fixture_server()
    session_id = "w02-bu-session"
    host_generation = "1"
    secret = str(uuid.uuid4())
    host = subprocess.Popen(
        [
            arguments.desktop_runtime,
            arguments.desktop_entry,
            f"--session-id={session_id}",
            f"--host-generation={host_generation}",
            f"--user-data-dir={profile_directory}",
            "--cdp-port=0",
            f"--control-secret={secret}",
            f"--initial-url={fixture_base_url}/w02-page.html",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=os.environ.copy(),
    )
    connection = None
    try:
        document = wait_for_discovery(profile_directory, DISCOVERY_TIMEOUT_SECONDS)
        connection, handshake = open_control_channel(document, session_id, host_generation, secret)
        endpoint = handshake["cdpEndpoint"]
        cdp_url = f"http://{endpoint['host']}:{endpoint['port']}"
        results.record(
            "BU_HOST_CDP_ENDPOINT",
            "Host handshake reports the CDP endpoint Browser Use must use",
            "pass" if int(endpoint["port"]) > 0 else "fail",
            f"cdp_url={cdp_url} capabilities={','.join(handshake.get('capabilities', []))}",
        )
        asyncio.run(verify_browser_use(cdp_url, fixture_base_url, results))
    finally:
        if connection is not None:
            try:
                connection.close()
            except OSError:
                pass
        try:
            host.wait(timeout=10)
        except subprocess.TimeoutExpired:
            host.kill()
        server.shutdown()

    payload = {
        "runtime": arguments.desktop_runtime,
        "entry": arguments.desktop_entry,
        "cases": results.cases,
    }
    (evidence_directory / "w02-browser-use-evidence.json").write_text(
        json.dumps(payload, indent=2), encoding="utf-8"
    )
    failing = [case for case in results.cases if case["status"] != "pass"]
    print(
        f"[w02:bu] completed: {len(results.cases) - len(failing)} pass, {len(failing)} not pass",
        file=sys.stderr,
    )
    return 0 if not failing else 1


if __name__ == "__main__":
    raise SystemExit(main())
