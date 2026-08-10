#!/usr/bin/env python3
"""Fast contract test for the release-owned Paddle full-parser sidecar."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
import types
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SIDECAR = ROOT / "gui" / "src-tauri" / "src" / "paddle_parser_sidecar.py"


class StubResult(dict):
    def __init__(self) -> None:
        raw = {
            "page_index": 0,
            "width": 640,
            "height": 480,
            "layout_det_res": {
                "boxes": [
                    {
                        "label": "text",
                        "coordinate": [10, 20, 300, 80],
                        "score": 0.99,
                    }
                ]
            },
            "parsing_res_list": [
                {
                    "block_id": 1,
                    "block_label": "text",
                    "block_content": "Hello from the parser contract fixture.",
                    "block_bbox": [10, 20, 300, 80],
                    "block_order": 1,
                }
            ],
        }
        super().__init__(raw)
        self.json = {"res": raw}
        self.markdown = {
            "markdown_texts": (
                "# Fixture\n\nHello from the parser contract fixture.\n\n"
                "![Fixture figure](figure.png)"
            ),
            "markdown_images": {"figure.png": b"fixture image bytes"},
        }

    def save_to_markdown(self, save_path: Path, **_: object) -> None:
        Path(save_path, "page.md").write_text(
            self.markdown["markdown_texts"], encoding="utf-8"
        )


class StubPaddleOCRVL:
    def __init__(self, **kwargs: object) -> None:
        model_dir = Path(str(kwargs["layout_detection_model_dir"]))
        assert model_dir.name == "PP-DocLayoutV3_infer"
        assert kwargs["vl_rec_backend"] == "llama-cpp-server"
        assert kwargs["vl_rec_server_url"].startswith("http://127.0.0.1:")
        assert kwargs["vl_rec_api_key"] == "contract-secret"
        assert kwargs["device"] == "cpu"

    def predict(self, **_: object):
        yield StubResult()

    def restructure_pages(self, pages, **_: object):
        return pages


def invoke(sidecar, arguments: list[str]) -> tuple[int, str, str]:
    previous = sys.argv
    stdout = io.StringIO()
    stderr = io.StringIO()
    try:
        sys.argv = [str(SIDECAR), *arguments]
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = sidecar.main()
    finally:
        sys.argv = previous
    return code, stdout.getvalue(), stderr.getvalue()


def main() -> int:
    paddleocr = types.ModuleType("paddleocr")
    paddleocr.__version__ = "3.7.0"
    paddleocr.PaddleOCRVL = StubPaddleOCRVL
    sys.modules["paddleocr"] = paddleocr

    spec = importlib.util.spec_from_file_location("pipeline_paddle_sidecar", SIDECAR)
    assert spec and spec.loader
    sidecar = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(sidecar)

    code, stdout, stderr = invoke(sidecar, ["--probe"])
    assert code == 0 and not stderr
    assert json.loads(stdout)["paddleocr"] == "3.7.0"

    with tempfile.TemporaryDirectory(prefix="pipeline-parser-contract-") as temp:
        root = Path(temp)
        model = root / "PP-DocLayoutV3_infer"
        model.mkdir()
        for name in ("inference.json", "inference.yml", "inference.pdiparams"):
            (model / name).write_bytes(b"contract fixture")

        common = [
            "--layout-model-dir",
            str(model),
            "--server-url",
            "http://127.0.0.1:49199/v1",
            "--api-key",
            "contract-secret",
        ]
        code, stdout, stderr = invoke(sidecar, ["--warm-layout", *common])
        assert code == 0 and not stderr
        assert json.loads(stdout)["layout_ready"] is True

        input_path = root / "fixture.png"
        input_path.write_bytes(b"contract fixture")
        output = root / "structure.json"
        assets = root / "assets"
        code, stdout, stderr = invoke(
            sidecar,
            [
                "--input",
                str(input_path),
                "--output",
                str(output),
                "--assets-dir",
                str(assets),
                "--baseline-chars",
                "[0]",
                *common,
            ],
        )
        assert code == 0 and not stderr, stderr
        assert json.loads(stdout)["pages"] == 1
        document = json.loads(output.read_text(encoding="utf-8"))
        assert document["schema_version"] == 2
        assert document["parser"] == "paddleocr-vl-full"
        assert len(document["pages"]) == 1
        assert document["pages"][0]["number"] == 1
        assert document["pages"][0]["blocks"][0]["role"] == "text"
        assert "Hello from the parser contract fixture" in document["pages"][0]["markdown"]
        assert "assets/page-0001-0001-figure.png" in document["pages"][0]["markdown"]
        assert (assets / "page-0001-0001-figure.png").read_bytes() == b"fixture image bytes"

        incomplete_model = root / "incomplete-model"
        incomplete_model.mkdir()
        code, _stdout, stderr = invoke(
            sidecar,
            ["--warm-layout", "--layout-model-dir", str(incomplete_model)],
        )
        assert code == 2 and "model is incomplete" in stderr

    print("Paddle full-parser sidecar contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
