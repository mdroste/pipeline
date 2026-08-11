#!/usr/bin/env python3
"""Provision and exercise the real checksum-locked Paddle parser stack."""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import platform
import shutil
import ssl
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
RESOURCES = ROOT / "gui" / "src-tauri" / "resources" / "paddle-parser"
SIDECAR = ROOT / "gui" / "src-tauri" / "src" / "paddle_parser_sidecar.py"


def platform_key() -> str:
    os_name = {"darwin": "macos", "win32": "windows", "linux": "linux"}.get(
        sys.platform
    )
    machine = platform.machine().lower()
    if not os_name:
        raise RuntimeError(f"Unsupported operating system: {sys.platform}")
    arch = (
        "arm64"
        if os_name == "macos" and machine in {"arm64", "aarch64"}
        else "aarch64" if machine in {"arm64", "aarch64"} else "x86_64"
    )
    return f"{os_name}-{arch}"


def download_verified(url: str, destination: Path, expected: str) -> None:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/octet-stream,*/*;q=0.8",
            "Referer": "https://www.paddleocr.ai/",
            "User-Agent": "Pipeline/0.9 release-qualification (+https://github.com/mdroste/pipeline)",
        },
    )
    certificate_file = os.environ.get("SSL_CERT_FILE")
    if not certificate_file and Path("/etc/ssl/cert.pem").is_file():
        certificate_file = "/etc/ssl/cert.pem"
    context = ssl.create_default_context(cafile=certificate_file)
    retryable_statuses = {403, 408, 429, 500, 502, 503, 504}
    for attempt in range(3):
        digest = hashlib.sha256()
        try:
            with urllib.request.urlopen(
                request, timeout=60, context=context
            ) as response, destination.open("wb") as output:
                while chunk := response.read(1024 * 1024):
                    digest.update(chunk)
                    output.write(chunk)
            break
        except urllib.error.HTTPError as error:
            destination.unlink(missing_ok=True)
            if error.code not in retryable_statuses or attempt == 2:
                raise
            time.sleep(2**attempt)
    actual = digest.hexdigest()
    if actual != expected:
        raise RuntimeError(
            f"Checksum mismatch for {url}: expected {expected}, received {actual}"
        )


def validate_tar_members(archive: tarfile.TarFile) -> None:
    for member in archive.getmembers():
        path = PurePosixPath(member.name)
        if path.is_absolute() or ".." in path.parts:
            raise RuntimeError(f"Unsafe archive path: {member.name}")
        if member.issym() or member.islnk():
            target = PurePosixPath(member.linkname)
            if target.is_absolute():
                raise RuntimeError(f"Unsafe archive link: {member.linkname}")
            depth = len(path.parent.parts)
            for part in target.parts:
                if part == "..":
                    depth -= 1
                    if depth < 0:
                        raise RuntimeError(f"Escaping archive link: {member.linkname}")
                elif part not in {"", "."}:
                    depth += 1


def extract_tar(archive_path: Path, destination: Path, mode: str) -> None:
    with tarfile.open(archive_path, mode) as archive:
        validate_tar_members(archive)
        if sys.version_info >= (3, 12):
            archive.extractall(destination, filter="fully_trusted")
        else:
            archive.extractall(destination)


def extract_uv(archive_path: Path, destination: Path, windows: bool) -> Path:
    name = "uv.exe" if windows else "uv"
    if windows:
        with zipfile.ZipFile(archive_path) as archive:
            candidates = [item for item in archive.infolist() if Path(item.filename).name == name]
            if len(candidates) != 1:
                raise RuntimeError("uv archive did not contain one uv executable")
            with archive.open(candidates[0]) as source, destination.open("wb") as output:
                shutil.copyfileobj(source, output)
    else:
        with tarfile.open(archive_path, "r:gz") as archive:
            candidates = [item for item in archive.getmembers() if Path(item.name).name == name]
            if len(candidates) != 1:
                raise RuntimeError("uv archive did not contain one uv executable")
            source = archive.extractfile(candidates[0])
            if source is None:
                raise RuntimeError("uv executable could not be read")
            with source, destination.open("wb") as output:
                shutil.copyfileobj(source, output)
        destination.chmod(0o755)
    return destination


class RecognitionHandler(http.server.BaseHTTPRequestHandler):
    requests_seen = 0

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if self.path.rstrip("/").endswith("/models"):
            self.respond(
                {"object": "list", "data": [{"id": "paddleocr-vl-1.6", "object": "model"}]}
            )
        else:
            self.send_error(404)

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if not self.path.rstrip("/").endswith("/chat/completions"):
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", "0"))
        json.loads(self.rfile.read(length))
        type(self).requests_seen += 1
        self.respond(
            {
                "id": "qualification",
                "object": "chat.completion",
                "created": int(time.time()),
                "model": "paddleocr-vl-1.6",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Full parser qualification text.",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {"prompt_tokens": 1, "completion_tokens": 5, "total_tokens": 6},
            }
        )

    def respond(self, value: object) -> None:
        body = json.dumps(value).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format: str, *_args: object) -> None:
        pass


def run(arguments: list[str], environment: dict[str, str], timeout: int = 1200) -> None:
    subprocess.run(arguments, check=True, env=environment, timeout=timeout)


def qualify(workspace: Path) -> None:
    runtime_lock = json.loads((RESOURCES / "runtime-lock.json").read_text(encoding="utf-8"))
    key = platform_key()
    python_lock = runtime_lock["python"]["artifacts"].get(key)
    uv_lock = runtime_lock["uv"]["artifacts"].get(key)
    if not python_lock or not uv_lock:
        raise RuntimeError(f"The full parser is not qualified for {key}")

    workspace.mkdir(parents=True, exist_ok=True)
    python_archive = workspace / "python.tar.gz"
    python_url = (
        "https://releases.astral.sh/github/python-build-standalone/releases/download/"
        f"{runtime_lock['python']['buildRelease']}/cpython-{runtime_lock['python']['version']}%2B"
        f"{runtime_lock['python']['buildRelease']}-{python_lock['target']}-install_only_stripped.tar.gz"
    )
    download_verified(python_url, python_archive, python_lock["sha256"])
    python_root = workspace / "cpython"
    python_root.mkdir()
    extract_tar(python_archive, python_root, "r:gz")
    base_python = (
        python_root / "python" / "python.exe"
        if os.name == "nt"
        else python_root / "python" / "bin" / "python3"
    )

    windows = os.name == "nt"
    uv_suffix = ".zip" if windows else ".tar.gz"
    uv_archive = workspace / f"uv{uv_suffix}"
    uv_url = (
        f"https://github.com/astral-sh/uv/releases/download/{runtime_lock['uv']['version']}/"
        f"uv-{uv_lock['target']}{uv_suffix}"
    )
    download_verified(uv_url, uv_archive, uv_lock["sha256"])
    uv = extract_uv(uv_archive, workspace / ("uv.exe" if windows else "uv"), windows)

    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("PYTHON", "PIP_", "UV_", "CONDA", "VIRTUAL_ENV"))
    }
    environment.update(
        {
            "UV_NO_CONFIG": "1",
            "UV_NO_PROJECT": "1",
            "UV_NO_SOURCES": "1",
            "UV_NO_PROGRESS": "1",
            "UV_PYTHON_DOWNLOADS": "never",
            "UV_DEFAULT_INDEX": "https://pypi.org/simple",
            "UV_INDEX_STRATEGY": "first-index",
            "UV_KEYRING_PROVIDER": "disabled",
            "UV_CACHE_DIR": str(workspace / "uv-cache"),
        }
    )
    venv = workspace / "venv"
    run(
        [
            str(uv),
            "venv",
            str(venv),
            "--python",
            str(base_python),
            "--no-python-downloads",
            "--no-managed-python",
            "--no-project",
        ],
        environment,
    )
    python = venv / ("Scripts/python.exe" if windows else "bin/python")
    package_lock = RESOURCES / python_lock["lock"]
    if hashlib.sha256(package_lock.read_bytes()).hexdigest() != python_lock["lockSha256"]:
        raise RuntimeError("Platform package lock checksum drifted")
    run(
        [
            str(uv),
            "pip",
            "sync",
            "--python",
            str(python),
            "--strict",
            "--require-hashes",
            "--no-build",
            "--no-index",
            "--no-sources",
            "--no-python-downloads",
            "--no-managed-python",
            str(package_lock),
        ],
        environment,
    )

    model_archive = workspace / "PP-DocLayoutV3_infer.tar"
    model_lock = runtime_lock["layoutModel"]
    download_verified(model_lock["url"], model_archive, model_lock["sha256"])
    models = workspace / "models"
    models.mkdir()
    extract_tar(model_archive, models, "r:")
    model = models / "PP-DocLayoutV3_infer"

    environment.update(
        {
            "PADDLE_PDX_CACHE_HOME": str(workspace / "cache"),
            "PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK": "True",
            "HF_HOME": str(workspace / "cache" / "huggingface"),
            "HF_HUB_OFFLINE": "1",
            "PYTHONNOUSERSITE": "1",
            "PYTHONUTF8": "1",
        }
    )
    run(
        [str(python), "-I", "-B", str(SIDECAR), "--probe"], environment, timeout=180
    )
    run(
        [
            str(python),
            "-I",
            "-B",
            str(SIDECAR),
            "--warm-layout",
            "--layout-model-dir",
            str(model),
        ],
        environment,
        timeout=600,
    )

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), RecognitionHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        fixture_code = (
            "from PIL import Image,ImageDraw; import sys; "
            "image=Image.new('RGB',(1200,400),'white'); "
            "ImageDraw.Draw(image).text((80,120),'FULL PARSER QUALIFICATION',fill='black',stroke_width=1); "
            "image.save(sys.argv[1],'PDF',resolution=72.0)"
        )
        fixture = workspace / "fixture.pdf"
        run([str(python), "-I", "-B", "-c", fixture_code, str(fixture)], environment)
        output = workspace / "structure.json"
        run(
            [
                str(python),
                "-I",
                "-B",
                str(SIDECAR),
                "--input",
                str(fixture),
                "--output",
                str(output),
                "--assets-dir",
                str(workspace / "assets"),
                "--layout-model-dir",
                str(model),
                "--server-url",
                f"http://127.0.0.1:{server.server_port}/v1",
                "--server-model",
                "paddleocr-vl-1.6",
                "--api-key",
                "qualification-only",
                "--baseline-chars",
                "[0]",
                "--layout-detection",
                "false",
            ],
            environment,
            timeout=900,
        )
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

    document = json.loads(output.read_text(encoding="utf-8"))
    if document.get("schema_version") != 2 or document.get("parser") != "paddleocr-vl-full":
        raise RuntimeError("Full parser returned an incompatible structure")
    if not document.get("pages") or RecognitionHandler.requests_seen < 1:
        raise RuntimeError("Full parser did not complete real layout-to-recognition extraction")
    print(
        json.dumps(
            {
                "ok": True,
                "platform": key,
                "release": runtime_lock["release"],
                "pages": len(document["pages"]),
                "recognitionRequests": RecognitionHandler.requests_seen,
            }
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", type=Path)
    args = parser.parse_args()
    if args.workspace:
        qualify(args.workspace.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="pipeline-parser-qualification-") as temp:
            qualify(Path(temp))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
