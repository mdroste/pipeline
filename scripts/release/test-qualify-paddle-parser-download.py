#!/usr/bin/env python3
"""Regression tests for the checksum-verified parser artifact downloader."""

from __future__ import annotations

import hashlib
import http.server
import importlib.util
import tempfile
import threading
import unittest
import unittest.mock
from pathlib import Path


SCRIPT = Path(__file__).with_name("qualify-paddle-parser.py")
SPEC = importlib.util.spec_from_file_location("qualify_paddle_parser", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"Could not load {SCRIPT}")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RetryHandler(http.server.BaseHTTPRequestHandler):
    payload = b"checksum-locked parser artifact"
    requests_seen = 0
    headers_seen: list[dict[str, str]] = []

    def do_GET(self) -> None:  # noqa: N802 - inherited HTTP handler API
        type(self).requests_seen += 1
        type(self).headers_seen.append(dict(self.headers))
        if type(self).requests_seen == 1:
            self.send_error(403)
            return
        self.send_response(200)
        self.send_header("Content-Length", str(len(self.payload)))
        self.end_headers()
        self.wfile.write(self.payload)

    def log_message(self, _format: str, *_args: object) -> None:
        pass


class MirrorHandler(http.server.BaseHTTPRequestHandler):
    files = {
        "inference.json": b"locked json",
        "inference.yml": b"locked yaml",
        "inference.pdiparams": b"locked parameters",
    }
    primary_requests = 0

    def do_GET(self) -> None:  # noqa: N802 - inherited HTTP handler API
        if self.path == "/primary.tar":
            type(self).primary_requests += 1
            self.send_error(403)
            return
        payload = self.files.get(Path(self.path).name)
        if payload is None:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, _format: str, *_args: object) -> None:
        pass


class DownloadVerifiedTests(unittest.TestCase):
    def test_retries_rejected_artifact_request_and_verifies_checksum(self) -> None:
        RetryHandler.requests_seen = 0
        RetryHandler.headers_seen = []
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), RetryHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory(prefix="pipeline-download-test-") as temp:
                destination = Path(temp) / "artifact.tar"
                digest = hashlib.sha256(RetryHandler.payload).hexdigest()
                MODULE.download_verified(
                    f"http://127.0.0.1:{server.server_port}/artifact.tar",
                    destination,
                    digest,
                )
                self.assertEqual(destination.read_bytes(), RetryHandler.payload)
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

        self.assertEqual(RetryHandler.requests_seen, 2)
        self.assertIn("application/octet-stream", RetryHandler.headers_seen[0]["Accept"])
        self.assertIn("Pipeline/0.9", RetryHandler.headers_seen[0]["User-Agent"])
        self.assertEqual(RetryHandler.headers_seen[0]["Referer"], "https://www.paddleocr.ai/")

    def test_provisions_byte_locked_official_mirror_after_primary_403(self) -> None:
        MirrorHandler.primary_requests = 0
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), MirrorHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base_url = f"http://127.0.0.1:{server.server_port}"
        model_lock = {
            "url": f"{base_url}/primary.tar",
            "sha256": "0" * 64,
            "qualificationFallback": {
                "baseUrl": f"{base_url}/mirror",
                "revision": "immutable-revision",
                "files": {
                    name: hashlib.sha256(payload).hexdigest()
                    for name, payload in MirrorHandler.files.items()
                },
            },
        }
        try:
            with tempfile.TemporaryDirectory(prefix="pipeline-mirror-test-") as temp:
                with unittest.mock.patch.object(MODULE.time, "sleep", return_value=None):
                    model = MODULE.provision_layout_model(model_lock, Path(temp))
                self.assertEqual(
                    {path.name: path.read_bytes() for path in model.iterdir()},
                    MirrorHandler.files,
                )
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

        self.assertEqual(MirrorHandler.primary_requests, 3)


if __name__ == "__main__":
    unittest.main()
