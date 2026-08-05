import argparse
import json
import os
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

# FunASR 1.2.7 invokes ffmpeg by name. The desktop project already ships the
# binaries under src-tauri, so expose that directory to the Python runtime.
PROJECT_ROOT = Path(__file__).resolve().parent.parent
FFMPEG_DIR = PROJECT_ROOT / "src-tauri"
if (FFMPEG_DIR / "ffmpeg.exe").is_file():
    os.environ["PATH"] = str(FFMPEG_DIR) + os.pathsep + os.environ.get("PATH", "")

import torch
from funasr import AutoModel

# FunASR registers model components by walking its Python package at runtime.
# Frozen executables do not expose package directories to pkgutil, so import
# the exact offline-model components explicitly before AutoModel is created.
import funasr.frontends.wav_frontend  # noqa: F401,E402
import funasr.models.bicif_paraformer.cif_predictor  # noqa: F401,E402
import funasr.models.e_paraformer.decoder  # noqa: F401,E402
import funasr.models.fsmn_vad_streaming.encoder  # noqa: F401,E402
import funasr.models.fsmn_vad_streaming.model  # noqa: F401,E402
import funasr.models.sanm.encoder  # noqa: F401,E402
import funasr.models.seaco_paraformer.model  # noqa: F401,E402
import funasr.models.specaug.specaug  # noqa: F401,E402
import funasr.tokenizer.char_tokenizer  # noqa: F401,E402


MODEL = None
MODEL_LOAD_SECONDS = 0.0
INFERENCE_LOCK = threading.Lock()
MIN_SEGMENT_MS = 5_000
MAX_SEGMENT_MS = 15_000

DEFAULT_HOTWORDS = (
    "佳能 尼康 索尼 富士 松下 适马 腾龙 蔡司 小白兔 爱死小白兔 "
    "70-200 24-70 24-105 24-240 R5 R6 R62 R8 R7 5D2 5D3 5D4 "
    "RF卡口 EF卡口 E卡口 Z卡口 X卡口 全画幅 半画幅 APS-C "
    "99新 95新 9成新 在仓现货 前盖 后盖 遮光罩 脚架环 UV镜 镜片 卡口 "
    "小黄车 置顶链接 号链接 到手价 优惠完价 赠品 发货 备注"
)


def resolved_model_dir(path: Path) -> Path | None:
    candidates = (path, path / "snapshots" / "master")
    for candidate in candidates:
        if (candidate / "config.yaml").is_file() or (candidate / "configuration.json").is_file():
            return candidate
    return None


def model_source(env_name: str, cache_name: str, fallback: str) -> str:
    configured = os.environ.get(env_name, "").strip()
    if configured:
        resolved = resolved_model_dir(Path(configured))
        if resolved:
            return str(resolved)
    cached = Path.home() / ".cache" / "modelscope" / "models" / cache_name
    resolved = resolved_model_dir(cached)
    return str(resolved) if resolved else fallback


def load_model() -> None:
    global MODEL, MODEL_LOAD_SECONDS
    started = time.perf_counter()
    torch.set_num_threads(4)
    MODEL = AutoModel(
        model=model_source(
            "BSR_FUNASR_ASR_MODEL",
            "iic--speech_seaco_paraformer_large_asr_nat-zh-cn-16k-common-vocab8404-pytorch",
            "paraformer-zh",
        ),
        vad_model=model_source(
            "BSR_FUNASR_VAD_MODEL",
            "iic--speech_fsmn_vad_zh-cn-16k-common-pytorch",
            "fsmn-vad",
        ),
        punc_model=None,
        device="cpu",
        disable_update=True,
        vad_kwargs={"max_single_segment_time": MAX_SEGMENT_MS},
    )
    MODEL_LOAD_SECONDS = time.perf_counter() - started


def split_sentences(text: str) -> list[str]:
    sentences = [part.strip() for part in re.findall(r".+?[。！？!?；;]|.+$", text)]
    return [part for part in sentences if part]


def build_segments(text: str, timestamps) -> list[dict]:
    if not text:
        return []
    if not timestamps:
        return [{"start_ms": 0, "end_ms": 1_000, "text": text}]

    sentences = split_sentences(text) or [text]
    weights = [max(1, len(re.sub(r"\s|[，。！？!?；;]", "", part))) for part in sentences]
    total_weight = sum(weights)
    total_stamps = len(timestamps)
    provisional = []
    consumed = 0

    for index, (sentence, weight) in enumerate(zip(sentences, weights)):
        start_index = min(consumed, total_stamps - 1)
        if index == len(sentences) - 1:
            end_index = total_stamps - 1
        else:
            consumed += max(1, round(total_stamps * weight / total_weight))
            end_index = min(max(start_index, consumed - 1), total_stamps - 1)
        start_ms = int(timestamps[start_index][0])
        end_ms = int(timestamps[end_index][1])
        duration = max(1, end_ms - start_ms)
        pieces = max(1, (duration + MAX_SEGMENT_MS - 1) // MAX_SEGMENT_MS)
        for piece in range(pieces):
            left = round(len(sentence) * piece / pieces)
            right = round(len(sentence) * (piece + 1) / pieces)
            piece_text = sentence[left:right].strip()
            if not piece_text:
                continue
            piece_start = start_ms + round(duration * piece / pieces)
            piece_end = start_ms + round(duration * (piece + 1) / pieces)
            provisional.append(
                {"start_ms": piece_start, "end_ms": piece_end, "text": piece_text}
            )

    segments = []
    for segment in provisional:
        duration = segment["end_ms"] - segment["start_ms"]
        if segments and duration < MIN_SEGMENT_MS:
            previous = segments[-1]
            if segment["end_ms"] - previous["start_ms"] <= MAX_SEGMENT_MS:
                previous["end_ms"] = segment["end_ms"]
                previous["text"] += segment["text"]
                continue
        segments.append(segment)
    return segments


def transcribe(payload: dict) -> dict:
    audio_path = Path(payload["audio_path"]).resolve()
    if not audio_path.is_file():
        raise ValueError(f"Audio file does not exist: {audio_path}")
    started = time.perf_counter()
    with INFERENCE_LOCK:
        result = MODEL.generate(
            input=str(audio_path),
            batch_size_s=120,
            use_itn=True,
            disable_pbar=True,
        )
    item = result[0] if result else {}
    text = str(item.get("text", "")).strip()
    # Recent FunASR versions may emit a space between every Chinese character.
    # Removing only Han-to-Han spaces preserves normal Latin/model-name spacing.
    text = re.sub(r"(?<=[\u4e00-\u9fff])\s+(?=[\u4e00-\u9fff])", "", text)
    timestamps = item.get("timestamp") or item.get("timestamps") or []
    segments = build_segments(text, timestamps)
    return {
        "engine": "funasr-paraformer-vad-fast",
        "raw_text": text,
        "corrected_text": text,
        "changes": [],
        "raw_segments": segments,
        "segments": segments,
        "review_items": [],
        "model_load_seconds": round(MODEL_LOAD_SECONDS, 3),
        "inference_seconds": round(time.perf_counter() - started, 3),
    }


class Handler(BaseHTTPRequestHandler):
    def send_json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        if self.path == "/health":
            self.send_json(200, {"ready": MODEL is not None, "engine": "funasr"})
        else:
            self.send_json(404, {"error": "not found"})

    def do_POST(self) -> None:
        if self.path != "/transcribe":
            self.send_json(404, {"error": "not found"})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            payload = json.loads(self.rfile.read(length).decode("utf-8"))
            self.send_json(200, transcribe(payload))
        except Exception as error:
            self.send_json(500, {"error": str(error)})

    def log_message(self, _format, *_args) -> None:
        return


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=18765)
    args = parser.parse_args()
    load_model()
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(
        json.dumps(
            {"ready": True, "port": args.port, "model_load_seconds": MODEL_LOAD_SECONDS}
        ),
        flush=True,
    )
    server.serve_forever()


if __name__ == "__main__":
    main()
