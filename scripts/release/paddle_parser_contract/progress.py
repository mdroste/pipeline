"""Progress reporting contract for the standalone Paddle parser."""

from __future__ import annotations

import concurrent.futures
import contextlib
import io
import types


class ProgressClient:
    def __init__(self) -> None:
        self.requests: list[concurrent.futures.Future] = []

    def create_chat_completion(self, _messages: object, **_: object):
        self.requests.append(concurrent.futures.Future())
        return self.requests[-1]


def recognition_pipeline(recognizer: types.SimpleNamespace) -> types.SimpleNamespace:
    """Paddle's private parts, with each page given as its count of regions."""

    def aggregate(page_results: list[int]) -> tuple[None, None, None, dict, None]:
        regions = [
            (page, region)
            for page, count in enumerate(page_results)
            for region in range(count)
        ]
        # Regions of one page can land in different pixel groups.
        groups = {"small": regions[::2], "large": regions[1::2]}
        return None, None, None, {
            key: {"vlm_block_ids": ids} for key, ids in groups.items()
        }, None

    inner = types.SimpleNamespace(
        vl_rec_model=recognizer, _paddleocr_vl_aggregate_vlm_batches=aggregate
    )
    return types.SimpleNamespace(paddlex_pipeline=types.SimpleNamespace(_pipeline=inner))


def progress_lines(action) -> list[str]:
    """Run `action` and return its progress lines without their time estimates."""
    stdout = io.StringIO()
    with contextlib.redirect_stdout(stdout):
        action()
    return [
        line.removeprefix("pipeline-progress: ").split(", about")[0].split(", less")[0]
        for line in stdout.getvalue().splitlines()
    ]


def check_page_progress(sidecar) -> None:
    assert sidecar._time_left(20) == "less than a minute left"
    assert sidecar._time_left(60) == "about 1 minute left"
    assert sidecar._time_left(600) == "about 10 minutes left"

    # Requests finish out of order, and a page counts once its last region does.
    client = ProgressClient()
    pipeline = recognition_pipeline(types.SimpleNamespace(genai_client=client))
    inner = pipeline.paddlex_pipeline._pipeline
    progress = sidecar._PageProgress(5)
    sidecar._watch_recognition(pipeline, progress)
    send = client.create_chat_completion
    assert progress_lines(
        lambda: inner._paddleocr_vl_aggregate_vlm_batches([2, 0, 1])
    ) == ["recognizing pages 1-3 of 5, 3 regions", "1 of 5 pages extracted"]
    requests = [send(None) for _ in range(3)]
    assert progress_lines(lambda: requests[2].set_result(None)) == []
    assert progress_lines(lambda: requests[1].set_result(None)) == [
        "2 of 5 pages extracted"
    ]
    assert progress_lines(lambda: requests[0].set_result(None)) == [
        "3 of 5 pages extracted"
    ]
    # Paddle returning the same pages afterwards repeats nothing.
    assert progress_lines(lambda: progress.pages_finished(3)) == []

    assert progress_lines(
        lambda: inner._paddleocr_vl_aggregate_vlm_batches([1, 1])
    ) == ["recognizing pages 4-5 of 5, 2 regions"]
    requests = [send(None) for _ in range(2)]
    assert progress_lines(lambda: requests[0].set_result(None)) == [
        "4 of 5 pages extracted"
    ]
    assert progress_lines(lambda: requests[1].set_result(None)) == [
        "5 of 5 pages extracted"
    ]

    # Retries go through the same hooks and must not count as new pages.
    progress.stop_watching()
    assert progress_lines(
        lambda: inner._paddleocr_vl_aggregate_vlm_batches([1])
    ) == []
    assert progress_lines(lambda: send(None).set_result(None)) == []

    # Built-in recognition returns regions one at a time.
    recognizer = types.SimpleNamespace(predict=lambda inputs, **_: iter(inputs))
    pipeline = recognition_pipeline(recognizer)
    inner = pipeline.paddlex_pipeline._pipeline
    progress = sidecar._PageProgress(2)
    sidecar._watch_recognition(pipeline, progress)
    assert progress_lines(
        lambda: inner._paddleocr_vl_aggregate_vlm_batches([1, 1])
    ) == ["recognizing pages 1-2 of 2, 2 regions"]
    regions = recognizer.predict(["first", "second"])
    assert progress_lines(lambda: next(regions)) == ["1 of 2 pages extracted"]
    assert progress_lines(lambda: next(regions)) == ["2 of 2 pages extracted"]

    # Without those private parts, only the pages Paddle returns are counted.
    progress = sidecar._PageProgress(0)
    sidecar._watch_recognition(types.SimpleNamespace(), progress)
    assert progress_lines(lambda: progress.pages_finished(2)) == ["2 pages extracted"]
