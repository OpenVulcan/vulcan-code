"""Minimal loopback fixture server for the W02 Browser Use verification.

为 W02 Browser Use 验证提供的最小回环夹具服务器。
"""

from __future__ import annotations

import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Tuple

FIXTURE_DIRECTORY = Path(__file__).resolve().parent


class _FixtureHandler(BaseHTTPRequestHandler):
    """Serves only files inside the fixture directory."""

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        """Serves one fixture file or a 404."""
        relative = self.path.split("?")[0].lstrip("/") or "w02-page.html"
        target = (FIXTURE_DIRECTORY / relative).resolve()
        if not str(target).startswith(str(FIXTURE_DIRECTORY)) or not target.is_file():
            self.send_error(404)
            return
        body = target.read_bytes()
        content_type = (
            "text/html; charset=utf-8" if target.suffix == ".html" else "text/plain; charset=utf-8"
        )
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:  # noqa: A002 - base signature
        """Suppresses default request logging."""


def start_fixture_server() -> Tuple[ThreadingHTTPServer, str]:
    """Starts the fixture server on an ephemeral loopback port."""
    server = ThreadingHTTPServer(("127.0.0.1", 0), _FixtureHandler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}"
