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
    merge_mode: object = None
    native = False

    def __init__(self, **kwargs: object) -> None:
        model_dir = Path(str(kwargs["layout_detection_model_dir"]))
        assert model_dir.name == "PP-DocLayoutV3_infer"
        StubPaddleOCRVL.native = "vl_rec_model_dir" in kwargs
        if StubPaddleOCRVL.native:
            assert Path(str(kwargs["vl_rec_model_dir"])).is_dir()
            assert "vl_rec_backend" not in kwargs
            assert "vl_rec_server_url" not in kwargs
        else:
            assert kwargs["vl_rec_backend"] == "llama-cpp-server"
            assert kwargs["vl_rec_server_url"].startswith("http://127.0.0.1:")
            assert kwargs["vl_rec_api_key"] == "contract-secret"
        assert kwargs["device"] == "cpu"
        StubPaddleOCRVL.merge_mode = kwargs["layout_merge_bboxes_mode"]

    def predict(self, **kwargs: object):
        assert kwargs["temperature"] == (None if StubPaddleOCRVL.native else 0.0)
        yield StubResult()

    def restructure_pages(self, pages, **_: object):
        return pages


def page_result(blocks: list[tuple[str, list[float], str]]) -> types.SimpleNamespace:
    """Build a 144 DPI letter page from (label, bbox, content) layout blocks."""
    return types.SimpleNamespace(
        json={
            "res": {
                "width": 1224,
                "height": 1584,
                "parsing_res_list": [
                    {"block_label": label, "block_bbox": bbox, "block_content": content}
                    for label, bbox, content in blocks
                ],
                "layout_det_res": {
                    "boxes": [
                        {"label": label, "coordinate": bbox} for label, bbox, _ in blocks
                    ]
                },
            }
        }
    )


def recovery_notes(sidecar, page: types.SimpleNamespace, baseline: int) -> list[str]:
    notes: list[str] = []
    sidecar._recover_suspicious_pages(
        None, [page], [baseline], [baseline], types.SimpleNamespace(), notes
    )
    return notes


class TitleRetryPipeline:
    def __init__(self, candidate: types.SimpleNamespace) -> None:
        self.candidate = candidate
        self.calls: list[dict[str, object]] = []

    def predict(self, **kwargs: object) -> list[types.SimpleNamespace]:
        self.calls.append(kwargs)
        return [self.candidate]


def title_retry(sidecar, page, candidate, layout_threshold: float = 0.5):
    pipeline = TitleRetryPipeline(candidate)
    page.get = lambda key: {"input_img": "page image"} if key == "layout_det_res" else None
    arguments = types.SimpleNamespace(
        native_model_dir="",
        layout_threshold=layout_threshold,
        layout_nms=True,
        resolved_layout_merge_mode="large",
        merge_layout_blocks=True,
        ocr_image_blocks=True,
        format_block_content=True,
        max_new_tokens=4096,
    )
    notes: list[str] = []
    pages, chars = sidecar._recover_suspicious_pages(
        pipeline, [page], [883], [883], arguments, notes
    )
    assert notes == []
    return pipeline, pages[0], chars[0]


def check_page_quality_rules(sidecar) -> None:
    # Two inequalities must not read as a single HTML tag.
    math = r"If $ \rho < 0 $, the spread widens. If $ \rho > 0 $, it narrows."
    assert "the spread widens" in sidecar._plain_text(math)
    table = "<div style='a'><table border=1><tr><td rowspan=\"2\">Cell</td></tr></table></div>"
    assert sidecar._plain_text(table) == "Cell"

    # Only `large` removes the entries inside a reference list.
    labels = ["text", "reference", "reference_content", "chart"]
    assert sidecar._layout_merge_mode("large", labels) == {
        0: "large",
        1: "union",
        2: "union",
        3: "large",
    }
    assert sidecar._layout_merge_mode("union", labels) == "union"
    assert sidecar._layout_merge_mode("small", labels) == "small"
    assert sidecar._layout_merge_mode("large", []) == "large"

    # Figure C11 has only panel titles and two charts covering 41% of the page.
    title = ("figure_title", [375, 204, 848, 232], "Panel A: Quarterly Refunding Statements")
    charts = [
        ("chart", [245, 252, 988, 788], ""),
        ("chart", [242, 863, 982, 1397], ""),
    ]
    figure_page = page_result([title, *charts])
    assert sidecar._result_is_figure_only(figure_page)
    assert recovery_notes(sidecar, figure_page, 883) == []

    # A second look may add the figure's title, but never at the cost of its crops.
    figure_title = ("figure_title", [159, 161, 1065, 189], "Figure C11: Distribution")
    titled = page_result([figure_title, title, *charts])
    pipeline, kept, chars = title_retry(sidecar, figure_page, titled)
    assert kept is titled and chars > 30
    assert pipeline.calls[0]["layout_threshold"] == 0.3
    assert pipeline.calls[0]["use_layout_detection"] is True
    not_figure_only = page_result([figure_title, ("text", [100, 300, 1100, 1300], "x" * 90)])
    _pipeline, kept, _chars = title_retry(sidecar, figure_page, not_figure_only)
    assert kept is figure_page
    no_gain = page_result([title, *charts])
    _pipeline, kept, _chars = title_retry(sidecar, figure_page, no_gain)
    assert kept is figure_page
    pipeline, kept, _chars = title_retry(sidecar, figure_page, titled, layout_threshold=0.3)
    assert kept is figure_page and pipeline.calls == []

    # Body text, or a chart too small to explain the text layer, keeps the check.
    with_body = page_result([title, *charts, ("text", [100, 1400, 1100, 1500], "")])
    assert not sidecar._result_is_figure_only(with_body)
    assert len(recovery_notes(sidecar, with_body, 883)) == 1
    small_chart = page_result([title, ("chart", [300, 300, 700, 600], "")])
    assert not sidecar._result_is_figure_only(small_chart)
    assert not sidecar._result_is_figure_only(
        types.SimpleNamespace(
            json={
                "res": {
                    "width": 1224,
                    "height": 1584,
                    "parsing_res_list": [
                        {"block_label": "chart", "block_bbox": [100, 100, 1100, 1400]}
                    ],
                    "layout_det_res": {
                        "boxes": [
                            {"label": "chart", "coordinate": [100, 100, 1100, 1400]},
                            {"label": "reference", "coordinate": [100, 100, 1100, 1400]},
                        ]
                    },
                }
            }
        )
    )


def image_block(text: str, name: str = "plot") -> str:
    """Format a recognized image the way Paddle does with block formatting on."""
    return (
        f'<div style="text-align: center;"><img src="imgs/{name}.jpg" alt="Image" '
        f'width="47%" />\n\n{text}\n\n</div>\n'
    )


def exported_page(
    blocks: list[tuple[str, list[float], str]], markdown: str
) -> types.SimpleNamespace:
    page = page_result(blocks)
    page.json["res"]["page_index"] = 0
    page.markdown = {"markdown_texts": markdown}
    page.save_to_markdown = lambda **_: None
    return page


def normalized(sidecar, page: types.SimpleNamespace) -> dict[str, object]:
    notes: list[str] = []
    with tempfile.TemporaryDirectory() as assets:
        result = sidecar._normalize_page(page, 1, Path(assets), True, notes)
    assert notes == []
    return result


def check_unverified_image_text(sidecar) -> None:
    table = "Horizon | Baseline | Jackknife\n1 | -0.1 | -0.1\n5 | -0.8 | -0.7\n9 | -1.5 | -1.3"
    # Sparse answers from real plots: rows among series names, and one runaway row.
    series = "Fed Total Holdings\nBaseline\n1 | 0.01\n5 | 0.10\n9 | 0.25"
    runaway = "Horizon h = 6 | " + " | ".join(f"2019e{month}" for month in range(8, 300))
    # Loops without a separator: a counter, a repeated pair, a repeated sentence.
    counter = "Scores for component 1\n" + "\n".join(str(-n) for n in range(300))
    pair = "Total FIU cost ($)\n" + "\n".join(["$21.8K", "$24.4K"] * 150)
    sentence = "\n\n".join(["Age 22.0% observed earnings effect"] * 220)
    for text in (table, series, runaway, counter, pair, sentence):
        assert sidecar._unverified_image_text("image", image_block(text)) == text
    # Pipes are not limited to the wrapped form, and page furniture is recognized too.
    assert sidecar._unverified_image_text("image", table) == table
    assert sidecar._unverified_image_text("header_image", image_block(table)) == table

    # Real pictures of text stay, including ones with a stray separator.
    excerpt = "By Sam Goldfarb Follow\n\nNov. 5, 2023 3:30 am ET\n\nThe Treasury said on Friday."
    press_release = "WASHINGTON — The Treasury is offering $78 billion:\n- A 3-year note\n- A 10-year note"
    masthead = "Markets | Bonds\nBy Sam Goldfarb | Follow\nNov. 5, 2023"
    # A picture of a table reads as spaced columns, and long pictures are fine.
    spaced = "Top 10% Top 1%\n" + "\n".join(f"19{n} 79.78 45.15" for n in range(35, 85))
    long_excerpt = "\n\n".join(f"Paragraph {n} of the excerpt." for n in range(200))
    for text in (excerpt, press_release, masthead, spaced, long_excerpt, ""):
        assert sidecar._unverified_image_text("image", image_block(text)) == ""
    # Only recognized image regions are in question; real tables have their own label.
    for label in ("table", "text", "chart"):
        assert sidecar._unverified_image_text(label, table) == ""

    notice = sidecar.UNVERIFIED_IMAGE_TEXT_NOTICE
    assert sidecar._plain_text(notice) == notice

    plot_box = [321.0, 157.0, 908.0, 570.0]
    second_box = [321.0, 764.0, 909.0, 1176.0]
    picture_box = [322.0, 1200.0, 831.0, 1500.0]
    body = "Body text about the response of industrial production."
    plot = image_block(table)
    picture = image_block(excerpt, "excerpt")
    blocks = [
        ("text", [100.0, 80.0, 1100.0, 140.0], body),
        ("image", plot_box, plot),
        ("image", second_box, plot),
        ("image", picture_box, picture),
    ]
    markdown = f"{body}\n\n![](imgs/plot.jpg)\n\n{table}\n\n![](imgs/plot.jpg)\n\n{table}\n\n{excerpt}"
    page = exported_page(blocks, markdown)

    # Withheld text cannot count as recognition or win a retry.
    without_table = exported_page(blocks[:1] + blocks[3:], f"{body}\n\n{excerpt}")
    assert sidecar._result_text_chars(page) == sidecar._result_text_chars(without_table)

    result = normalized(sidecar, page)
    assert table not in result["markdown"] and "-0.8" not in result["markdown"]
    assert result["markdown"].count(notice) == 2
    assert "![](imgs/plot.jpg)" in result["markdown"]
    assert excerpt in result["markdown"]
    assert result["blocks"][0]["markdown"] == body
    for block in result["blocks"][1:3]:
        assert block["block_label"] == "image"
        assert '<img src="imgs/plot.jpg"' in block["markdown"]
        assert notice in block["markdown"]
        assert block["text"] == notice
        assert block["raw"]["block_content"] == block["markdown"]
        assert block["raw"][sidecar.UNVERIFIED_IMAGE_TEXT_KEY] == table
    real = result["blocks"][3]
    assert real["markdown"] == picture and excerpt.split("\n")[0] in real["text"]
    assert sidecar.UNVERIFIED_IMAGE_TEXT_KEY not in real["raw"]

    # An export that spells the text differently must not carry it through.
    rewritten = table.replace(" | ", " \\| ")
    result = normalized(sidecar, exported_page(blocks[:2], f"{body}\n\n{rewritten}"))
    assert "-0.8" not in result["markdown"] and notice in result["markdown"]
    assert body in result["markdown"]

    # Margins never reach the page export, so missing it there changes nothing.
    margin = [("text", [100.0, 80.0, 1100.0, 140.0], body), ("header_image", plot_box, plot)]
    result = normalized(sidecar, exported_page(margin, body))
    assert result["markdown"] == body
    assert result["blocks"][1]["text"] == notice


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

    check_page_quality_rules(sidecar)
    check_unverified_image_text(sidecar)

    code, stdout, stderr = invoke(sidecar, ["--probe"])
    assert code == 0 and not stderr
    assert json.loads(stdout)["paddleocr"] == "3.7.0"

    with tempfile.TemporaryDirectory(prefix="pipeline-parser-contract-") as temp:
        root = Path(temp)
        model = root / "PP-DocLayoutV3_infer"
        model.mkdir()
        for name in ("inference.json", "inference.yml", "inference.pdiparams"):
            (model / name).write_bytes(b"contract fixture")
        (model / "inference.yml").write_text(
            "Global:\n  model_name: fixture\n"
            "label_list:\n- text\n- reference\n- reference_content\n"
            "Hpi:\n  backend_configs: {}\n",
            encoding="utf-8",
        )

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
        assert StubPaddleOCRVL.merge_mode == {0: "large", 1: "union", 2: "union"}
        assert document["settings"]["layout_merge_bboxes_mode"] == "large"

        native_model = root / "native-model"
        native_model.mkdir()
        native_arguments = [
            "--layout-model-dir",
            str(model),
            "--native-model-dir",
            str(native_model),
        ]
        code, stdout, stderr = invoke(sidecar, ["--warm-layout", *native_arguments])
        assert code == 0 and not stderr, stderr
        assert StubPaddleOCRVL.native
        code, stdout, stderr = invoke(
            sidecar,
            [
                "--input",
                str(input_path),
                "--output",
                str(root / "native.json"),
                "--assets-dir",
                str(root / "native-assets"),
                "--baseline-chars",
                "[0]",
                *native_arguments,
            ],
        )
        assert code == 0 and not stderr, stderr
        code, stdout, stderr = invoke(
            sidecar,
            ["--warm-layout", "--layout-model-dir", str(model), "--native-model-dir", str(root / "missing")],
        )
        assert code == 2 and "recognition model" in stderr
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
