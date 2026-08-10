"""Narrow managed sidecar for Pipeline's full PaddleOCR-VL parser.

The Rust host owns the llama.cpp service and passes an authenticated loopback
URL to this process.  This script owns only PaddleOCR's client-side layout,
recognition orchestration, page restructuring, and normalization into a small
versioned JSON contract.  It intentionally has no package-management logic.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 2
MAX_RAW_BLOCK_CHARS = 200_000
SUSPECT_BASELINE_MIN_CHARS = 200


def _bool(value: str) -> bool:
    normalized = value.strip().lower()
    if normalized in {"1", "true", "yes", "on"}:
        return True
    if normalized in {"0", "false", "no", "off"}:
        return False
    raise argparse.ArgumentTypeError(f"invalid boolean: {value}")


def _jsonable(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, dict):
        return {str(key): _jsonable(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_jsonable(item) for item in value]
    if hasattr(value, "tolist"):
        return _jsonable(value.tolist())
    if hasattr(value, "item"):
        try:
            return _jsonable(value.item())
        except (TypeError, ValueError):
            pass
    return str(value)


def _property(value: Any, name: str, default: Any) -> Any:
    candidate = getattr(value, name, default)
    if callable(candidate):
        candidate = candidate()
    return candidate


def _result_json(result: Any) -> dict[str, Any]:
    value = _jsonable(_property(result, "json", {}))
    if not isinstance(value, dict):
        return {}
    nested = value.get("res")
    return nested if isinstance(nested, dict) else value


def _bbox(value: Any) -> list[float]:
    value = _jsonable(value)
    if not isinstance(value, list):
        return []
    if len(value) >= 4 and all(isinstance(item, (int, float)) for item in value[:4]):
        return [float(item) for item in value[:4]]
    points = [
        point
        for point in value
        if isinstance(point, list)
        and len(point) >= 2
        and isinstance(point[0], (int, float))
        and isinstance(point[1], (int, float))
    ]
    if not points:
        return []
    xs = [float(point[0]) for point in points]
    ys = [float(point[1]) for point in points]
    return [min(xs), min(ys), max(xs), max(ys)]


def _polygon(value: Any) -> list[list[float]]:
    value = _jsonable(value)
    if not isinstance(value, list):
        return []
    points = []
    for point in value:
        if (
            isinstance(point, list)
            and len(point) >= 2
            and isinstance(point[0], (int, float))
            and isinstance(point[1], (int, float))
        ):
            points.append([float(point[0]), float(point[1])])
    return points


def _iou(left: list[float], right: list[float]) -> float:
    if len(left) != 4 or len(right) != 4:
        return 0.0
    x1 = max(left[0], right[0])
    y1 = max(left[1], right[1])
    x2 = min(left[2], right[2])
    y2 = min(left[3], right[3])
    intersection = max(0.0, x2 - x1) * max(0.0, y2 - y1)
    left_area = max(0.0, left[2] - left[0]) * max(0.0, left[3] - left[1])
    right_area = max(0.0, right[2] - right[0]) * max(0.0, right[3] - right[1])
    union = left_area + right_area - intersection
    return intersection / union if union > 0 else 0.0


def _layout_confidence(
    label: str, bbox: list[float], layout_boxes: list[dict[str, Any]]
) -> float | None:
    best_score: float | None = None
    best_overlap = -1.0
    for box in layout_boxes:
        box_label = str(box.get("label", ""))
        if label and box_label and label != box_label:
            continue
        overlap = _iou(bbox, _bbox(box.get("coordinate", box.get("bbox", []))))
        if overlap < best_overlap:
            continue
        score = box.get("score")
        if isinstance(score, (int, float)):
            best_overlap = overlap
            best_score = float(score)
    return best_score


def _plain_text(markdown: str) -> str:
    text = re.sub(r"(?s)<!--.*?-->", " ", markdown)
    text = re.sub(r"(?s)<[^>]+>", " ", text)
    text = re.sub(r"!?\[([^]]*)\]\([^)]+\)", r"\1", text)
    text = re.sub(r"(?m)^\s{0,3}(?:#{1,6}\s+|>\s*|[-+]\s+)", "", text)
    text = text.replace("*", "").replace("_", "").replace("`", "")
    return " ".join(text.split())


def _substantive_chars(text: str) -> int:
    return sum(1 for char in text if not char.isspace())


def _result_text_chars(result: Any) -> int:
    """Measure recognition before cross-page restructuring can move blocks."""
    info = _jsonable(_property(result, "markdown", {}))
    markdown = ""
    if isinstance(info, dict):
        candidate = info.get("markdown_texts", info.get("text", ""))
        if isinstance(candidate, list):
            markdown = "\n\n".join(str(item) for item in candidate)
        elif candidate is not None:
            markdown = str(candidate)

    raw = _result_json(result)
    parsing = raw.get("parsing_res_list", [])
    block_text = ""
    if isinstance(parsing, list):
        contents: list[str] = []
        for block in parsing:
            if not isinstance(block, dict):
                continue
            content = block.get("block_content", block.get("content", ""))
            if isinstance(content, (dict, list)):
                contents.append(json.dumps(_jsonable(content), ensure_ascii=False))
            elif content is not None:
                contents.append(str(content))
        block_text = "\n\n".join(contents)

    return max(
        _substantive_chars(_plain_text(markdown)),
        _substantive_chars(_plain_text(block_text)),
    )


def _is_suspicious(char_count: int, baseline_chars: int | None) -> bool:
    return (
        baseline_chars is not None
        and baseline_chars >= SUSPECT_BASELINE_MIN_CHARS
        and char_count < baseline_chars // 10
    )


def _page_image(result: Any) -> Any | None:
    """Recover the page image retained by Paddle's prediction result."""
    try:
        preprocessor = result.get("doc_preprocessor_res")
    except (AttributeError, TypeError):
        preprocessor = None
    if preprocessor is not None:
        try:
            image = preprocessor.get("output_img")
        except (AttributeError, TypeError):
            image = None
        if image is not None:
            return image
    try:
        layout = result.get("layout_det_res")
    except (AttributeError, TypeError):
        layout = None
    if layout is not None:
        try:
            return layout.get("input_img")
        except (AttributeError, TypeError):
            return None
    return None


def _copy_page_identity(source: Any, target: Any) -> None:
    for key in ("input_path", "page_index", "page_count"):
        try:
            target[key] = source[key]
        except (KeyError, TypeError):
            continue


def _retry_page(
    pipeline: Any,
    source: Any,
    image: Any,
    args: argparse.Namespace,
    *,
    use_layout_detection: bool,
) -> Any | None:
    candidates = list(
        pipeline.predict(
            input=image,
            use_layout_detection=use_layout_detection,
            prompt_label=None if use_layout_detection else "ocr",
            layout_threshold=args.layout_threshold,
            layout_nms=args.layout_nms,
            layout_merge_bboxes_mode=args.layout_merge_bboxes_mode,
            merge_layout_blocks=args.merge_layout_blocks,
            use_ocr_for_image_block=args.ocr_image_blocks,
            format_block_content=args.format_block_content,
            max_new_tokens=args.max_new_tokens,
            temperature=0.0,
            use_queues=False,
        )
    )
    if not candidates:
        return None
    candidate = candidates[0]
    _copy_page_identity(source, candidate)
    return candidate


def _recover_suspicious_pages(
    pipeline: Any,
    pages: list[Any],
    baselines: list[int],
    args: argparse.Namespace,
    quality_notes: list[str],
) -> tuple[list[Any], list[int]]:
    recovered_pages = list(pages)
    source_text_chars: list[int] = []
    for index, original in enumerate(pages):
        page_number = index + 1
        baseline = baselines[index] if index < len(baselines) else None
        best = original
        best_chars = _result_text_chars(best)
        if _is_suspicious(best_chars, baseline):
            image = _page_image(original)
            if image is None:
                quality_notes.append(
                    f"Page {page_number}: could not access Paddle's retained page image for recognition recovery."
                )
            else:
                retry_mode = (
                    "layout recognition"
                    if args.layout_detection
                    else "whole-page recognition"
                )
                for attempt in range(args.page_retries):
                    print(
                        f"Page {page_number}: retrying incomplete {retry_mode} "
                        f"({best_chars}/{baseline} substantive characters, attempt {attempt + 1}/{args.page_retries}).",
                        file=sys.stderr,
                    )
                    try:
                        candidate = _retry_page(
                            pipeline,
                            original,
                            image,
                            args,
                            use_layout_detection=args.layout_detection,
                        )
                    except Exception as error:
                        print(
                            f"Page {page_number}: layout recognition retry failed: {error}",
                            file=sys.stderr,
                        )
                        candidate = None
                    if candidate is not None:
                        candidate_chars = _result_text_chars(candidate)
                        if candidate_chars > best_chars:
                            best = candidate
                            best_chars = candidate_chars
                    if not _is_suspicious(best_chars, baseline):
                        quality_notes.append(
                            f"Page {page_number}: recovered incomplete recognition with a {retry_mode} retry."
                        )
                        break

                if _is_suspicious(best_chars, baseline) and args.layout_detection:
                    print(
                        f"Page {page_number}: layout retries remained incomplete; "
                        "recovering the page with PaddleOCR-VL whole-page recognition.",
                        file=sys.stderr,
                    )
                    try:
                        candidate = _retry_page(
                            pipeline,
                            original,
                            image,
                            args,
                            use_layout_detection=False,
                        )
                    except Exception as error:
                        print(
                            f"Page {page_number}: whole-page recognition recovery failed: {error}",
                            file=sys.stderr,
                        )
                        candidate = None
                    if candidate is not None:
                        candidate_chars = _result_text_chars(candidate)
                        if candidate_chars > best_chars:
                            best = candidate
                            best_chars = candidate_chars
                    if not _is_suspicious(best_chars, baseline):
                        quality_notes.append(
                            f"Page {page_number}: recovered incomplete layout recognition with "
                            "PaddleOCR-VL whole-page recognition; consult the rendered page when "
                            "block-level layout matters."
                        )

        recovered_pages[index] = best
        source_text_chars.append(best_chars)
    return recovered_pages, source_text_chars


def _safe_asset_name(page: int, name: str, ordinal: int) -> str:
    base = Path(name).name or f"asset-{ordinal:04}.png"
    safe = re.sub(r"[^A-Za-z0-9._-]+", "-", base).strip(".-")
    if not safe:
        safe = f"asset-{ordinal:04}.png"
    return f"page-{page:04}-{ordinal:04}-{safe}"


def _save_markdown_images(
    result: Any,
    page: int,
    assets_dir: Path,
    markdown: str,
    exported_names: set[str] | None = None,
) -> tuple[str, list[str]]:
    info = _jsonable(_property(result, "markdown", {}))
    if not isinstance(info, dict):
        return markdown, []
    images = info.get("markdown_images", info.get("images", {}))
    if not isinstance(images, dict):
        return markdown, []
    saved: list[str] = []
    for ordinal, (original_name, original_image) in enumerate(images.items(), start=1):
        original_path = Path(str(original_name))
        if exported_names and (
            str(original_name) in exported_names or original_path.name in exported_names
        ):
            continue
        image = original_image
        # _jsonable stringifies PIL objects, so retrieve them from the original
        # property when possible.
        raw_info = _property(result, "markdown", {})
        if isinstance(raw_info, dict):
            raw_images = raw_info.get("markdown_images", raw_info.get("images", {}))
            if isinstance(raw_images, dict):
                image = raw_images.get(original_name, original_image)
        target_name = _safe_asset_name(page, str(original_name), ordinal)
        target = assets_dir / target_name
        try:
            if hasattr(image, "save"):
                image.save(target)
            elif isinstance(image, (bytes, bytearray)):
                target.write_bytes(bytes(image))
            else:
                continue
        except OSError:
            continue
        relative = f"assets/{target_name}"
        markdown = markdown.replace(str(original_name), relative)
        saved.append(relative)
    return markdown, saved


def _saved_markdown(
    result: Any,
    page: int,
    assets_dir: Path,
    show_formula_numbers: bool,
    quality_notes: list[str],
) -> tuple[str, list[str]]:
    info = _jsonable(_property(result, "markdown", {}))
    fallback = ""
    if isinstance(info, dict):
        candidate = info.get("markdown_texts", info.get("text", ""))
        if isinstance(candidate, list):
            fallback = "\n\n".join(str(item) for item in candidate)
        elif candidate is not None:
            fallback = str(candidate)

    with tempfile.TemporaryDirectory(prefix=f"pipeline-paddle-page-{page:04}-") as temp:
        temp_path = Path(temp)
        try:
            result.save_to_markdown(
                save_path=temp_path,
                pretty=False,
                show_formula_number=show_formula_numbers,
            )
            markdown_files = sorted(temp_path.rglob("*.md"))
            if markdown_files:
                fallback = markdown_files[0].read_text(encoding="utf-8")
            saved: list[str] = []
            exported_names: set[str] = set()
            raw_info = _property(result, "markdown", {})
            raw_images = (
                raw_info.get("markdown_images", raw_info.get("images", {}))
                if isinstance(raw_info, dict)
                else {}
            )
            original_image_names = (
                [str(name) for name in raw_images]
                if isinstance(raw_images, dict)
                else []
            )
            for ordinal, source in enumerate(
                sorted(
                    path
                    for path in temp_path.rglob("*")
                    if path.is_file()
                    and path.suffix.lower() in {".png", ".jpg", ".jpeg", ".gif", ".webp"}
                ),
                start=1,
            ):
                target_name = _safe_asset_name(page, source.name, ordinal)
                target = assets_dir / target_name
                shutil.copy2(source, target)
                try:
                    relative_source = source.relative_to(temp_path).as_posix()
                except ValueError:
                    relative_source = source.name
                aliases = {relative_source, source.name}
                aliases.update(
                    name for name in original_image_names if Path(name).name == source.name
                )
                exported_names.update(aliases)
                relative = f"assets/{target_name}"
                matched_reference = False
                for alias in sorted(aliases, key=len, reverse=True):
                    if alias in fallback:
                        fallback = fallback.replace(alias, relative)
                        matched_reference = True
                        break
                if not matched_reference:
                    fallback = fallback.replace(source.name, relative)
                saved.append(relative)
            fallback, property_assets = _save_markdown_images(
                result,
                page,
                assets_dir,
                fallback,
                exported_names,
            )
            return fallback.strip(), sorted(set(saved + property_assets))
        except Exception as error:  # Paddle result writers vary by patch release.
            quality_notes.append(
                f"Page {page}: PaddleOCR markdown export fell back to the in-memory result ({error})."
            )
            fallback, saved = _save_markdown_images(result, page, assets_dir, fallback)
            return fallback.strip(), saved


def _normalize_page(
    result: Any,
    ordinal: int,
    assets_dir: Path,
    show_formula_numbers: bool,
    quality_notes: list[str],
) -> dict[str, Any]:
    raw = _result_json(result)
    raw_page = raw.get("page_index")
    page = int(raw_page) + 1 if isinstance(raw_page, int) else ordinal
    markdown, page_assets = _saved_markdown(
        result, page, assets_dir, show_formula_numbers, quality_notes
    )
    layout = raw.get("layout_det_res", {})
    layout_boxes = layout.get("boxes", []) if isinstance(layout, dict) else []
    if not isinstance(layout_boxes, list):
        layout_boxes = []
    parsing = raw.get("parsing_res_list", [])
    if not isinstance(parsing, list):
        parsing = []

    blocks: list[dict[str, Any]] = []
    for fallback_order, item in enumerate(parsing, start=1):
        if not isinstance(item, dict):
            continue
        label = str(item.get("block_label", item.get("label", "text")))
        content_value = item.get("block_content", item.get("content", ""))
        if isinstance(content_value, (dict, list)):
            content = json.dumps(_jsonable(content_value), ensure_ascii=False)
        else:
            content = str(content_value or "")
        raw_bbox = item.get("block_bbox", item.get("bbox", []))
        bbox = _bbox(raw_bbox)
        polygon = _polygon(raw_bbox)
        order_value = item.get("block_order", item.get("order", fallback_order))
        order = int(order_value) if isinstance(order_value, (int, float)) else fallback_order
        block_id = item.get("block_id", fallback_order)
        confidence = _layout_confidence(label, bbox, layout_boxes)
        block_assets = [
            asset
            for asset in page_assets
            if Path(asset).name in content or asset in content
        ]
        raw_block = _jsonable(item)
        encoded_raw = json.dumps(raw_block, ensure_ascii=False)
        if len(encoded_raw) > MAX_RAW_BLOCK_CHARS:
            raw_block = {"truncated": True, "block_label": label, "block_order": order}
        blocks.append(
            {
                "block_id": f"paddle-full-page-{page:04}-block-{str(block_id)}",
                "role": label,
                "block_label": label,
                "markdown": content,
                "text": _plain_text(content),
                "boundary": None,
                "note_marker": None,
                "order": order,
                "bbox": bbox,
                "polygon": polygon,
                "confidence": confidence,
                "asset_files": block_assets,
                "raw": raw_block,
            }
        )
    blocks.sort(key=lambda block: (block["order"], block["block_id"]))
    # Markdown exports do not expose a stable image-to-block identifier in
    # every PaddleOCR patch release. Preserve deterministic associations for
    # image-like regions even when the original filename was rewritten by the
    # result writer.
    image_blocks = [
        block
        for block in blocks
        if block["block_label"].lower()
        in {"image", "figure", "chart", "image_body", "chart_body"}
    ]
    for index, block in enumerate(image_blocks):
        if not block["asset_files"] and index < len(page_assets):
            block["asset_files"] = [page_assets[index]]
    if not markdown:
        markdown = "\n\n".join(
            block["markdown"] for block in blocks if block["markdown"].strip()
        )
    width = raw.get("width")
    height = raw.get("height")
    return {
        "number": page,
        "markdown": markdown,
        "width": int(width) if isinstance(width, (int, float)) and width > 0 else None,
        "height": int(height) if isinstance(height, (int, float)) and height > 0 else None,
        "blocks": blocks,
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--probe", action="store_true")
    parser.add_argument("--warm-layout", action="store_true")
    parser.add_argument("--input")
    parser.add_argument("--output")
    parser.add_argument("--assets-dir")
    parser.add_argument("--layout-model-dir")
    parser.add_argument("--server-url", default="http://127.0.0.1:9/v1")
    parser.add_argument("--server-model", default="paddleocr-vl-1.6")
    parser.add_argument("--api-key", default=os.environ.get("PIPELINE_PADDLE_API_KEY", ""))
    parser.add_argument("--concurrency", type=int, default=1)
    parser.add_argument("--max-new-tokens", type=int, default=4096)
    parser.add_argument("--page-retries", type=int, default=1)
    parser.add_argument("--baseline-chars", default="[]")
    parser.add_argument("--layout-detection", type=_bool, default=True)
    parser.add_argument("--layout-threshold", type=float, default=0.5)
    parser.add_argument("--layout-nms", type=_bool, default=True)
    parser.add_argument("--layout-merge-bboxes-mode", choices=("large", "small", "union"), default="large")
    parser.add_argument("--merge-layout-blocks", type=_bool, default=True)
    parser.add_argument("--ocr-image-blocks", type=_bool, default=True)
    parser.add_argument("--format-block-content", type=_bool, default=True)
    parser.add_argument("--merge-tables", type=_bool, default=True)
    parser.add_argument("--relevel-titles", type=_bool, default=True)
    parser.add_argument("--show-formula-numbers", type=_bool, default=True)
    return parser


def main() -> int:
    args = _parser().parse_args()
    try:
        import paddleocr
        from paddleocr import PaddleOCRVL
    except Exception as error:
        print(f"PaddleOCR import failed: {error}", file=sys.stderr)
        return 2

    version = str(getattr(paddleocr, "__version__", "unknown"))
    if args.probe and not args.warm_layout:
        print(json.dumps({"ok": True, "paddleocr": version}))
        return 0
    if not args.layout_model_dir:
        print("--layout-model-dir is required", file=sys.stderr)
        return 2
    layout_model_dir = Path(args.layout_model_dir)
    required_model_files = ("inference.json", "inference.yml", "inference.pdiparams")
    if not layout_model_dir.is_dir() or any(
        not (layout_model_dir / name).is_file() for name in required_model_files
    ):
        print("The managed PP-DocLayoutV3 model is incomplete", file=sys.stderr)
        return 2

    pipeline = PaddleOCRVL(
        pipeline_version="v1.6",
        device="cpu",
        vl_rec_backend="llama-cpp-server",
        vl_rec_server_url=args.server_url,
        vl_rec_max_concurrency=args.concurrency,
        vl_rec_api_model_name=args.server_model,
        vl_rec_api_key=args.api_key or None,
        layout_detection_model_dir=str(layout_model_dir),
        use_layout_detection=args.layout_detection,
        layout_threshold=args.layout_threshold,
        layout_nms=args.layout_nms,
        layout_merge_bboxes_mode=args.layout_merge_bboxes_mode,
        merge_layout_blocks=args.merge_layout_blocks,
        use_ocr_for_image_block=args.ocr_image_blocks,
        format_block_content=args.format_block_content,
        # Preserve semantic text while excluding repeated visual margins from
        # the compatibility Markdown view. The structured JSON retains them.
        markdown_ignore_labels=["number", "header", "header_image", "footer", "footer_image"],
        use_queues=True,
    )
    if args.warm_layout:
        print(json.dumps({"ok": True, "paddleocr": version, "layout_ready": True}))
        return 0
    if not args.input or not args.output or not args.assets_dir:
        print("--input, --output, and --assets-dir are required", file=sys.stderr)
        return 2

    output_path = Path(args.output)
    assets_dir = Path(args.assets_dir)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    assets_dir.mkdir(parents=True, exist_ok=True)
    pages = list(
        pipeline.predict(
            input=args.input,
            max_new_tokens=args.max_new_tokens,
            temperature=0.0,
        )
    )
    try:
        parsed_baselines = json.loads(args.baseline_chars)
        baselines = (
            [int(value) for value in parsed_baselines]
            if isinstance(parsed_baselines, list)
            else []
        )
    except (TypeError, ValueError, json.JSONDecodeError):
        baselines = []
    quality_notes: list[str] = []
    pages, source_text_chars = _recover_suspicious_pages(
        pipeline,
        pages,
        baselines,
        args,
        quality_notes,
    )
    pages = list(
        pipeline.restructure_pages(
            pages,
            merge_tables=args.merge_tables,
            relevel_titles=args.relevel_titles,
            concatenate_pages=False,
        )
    )
    normalized = [
        _normalize_page(
            result,
            ordinal,
            assets_dir,
            args.show_formula_numbers,
            quality_notes,
        )
        for ordinal, result in enumerate(pages, start=1)
    ]
    normalized.sort(key=lambda page: page["number"])
    for page in normalized:
        page_number = page["number"]
        if 1 <= page_number <= len(source_text_chars):
            page["source_text_chars"] = source_text_chars[page_number - 1]
    if not normalized:
        raise RuntimeError("PaddleOCR returned no pages")
    document = {
        "schema_version": SCHEMA_VERSION,
        "parser": "paddleocr-vl-full",
        "parser_version": version,
        "settings": {
            "layout_detection": args.layout_detection,
            "layout_threshold": args.layout_threshold,
            "layout_nms": args.layout_nms,
            "layout_merge_bboxes_mode": args.layout_merge_bboxes_mode,
            "merge_layout_blocks": args.merge_layout_blocks,
            "ocr_image_blocks": args.ocr_image_blocks,
            "format_block_content": args.format_block_content,
            "merge_tables": args.merge_tables,
            "relevel_titles": args.relevel_titles,
            "show_formula_numbers": args.show_formula_numbers,
        },
        "quality_notes": quality_notes,
        "pages": normalized,
    }
    output_path.write_text(
        json.dumps(document, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(json.dumps({"ok": True, "pages": len(normalized), "paddleocr": version}))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
    except Exception as error:
        print(f"PaddleOCR full parser failed: {error}", file=sys.stderr)
        raise SystemExit(1)
