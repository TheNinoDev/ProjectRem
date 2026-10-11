#!/usr/bin/env python3
"""ProjectRem Python sidecar.

The Rust/egui GUI starts this process and talks to it over stdin/stdout using
newline-delimited JSON:

    ->  {"id": 1, "cmd": "ping"}
    <-  {"id": 1, "ok": true, ...}

Every reply carries the `id` of the request that triggered it. Unsolicited
notifications carry an `event` field instead of an `id`.

Nothing here needs a third-party package: audio device discovery uses
`sounddevice` / `pyaudio` when they happen to be installed and degrades
gracefully when they are not.
"""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import struct
import sys
import traceback
import wave
from pathlib import Path
from typing import Any

VERSION = "0.0.0"
PROTOCOL = 1

# Recording format. 16 kHz mono is what Whisper wants, so that is what we
# capture: recording at 48 kHz would only add a resampling step later.
SAMPLE_RATE = 16000
CHANNELS = 1
SAMPLE_WIDTH = 2  # bytes, i.e. signed 16-bit PCM

# A recording longer than this is almost certainly a stuck button.
MAX_RECORD_SECONDS = 300


def log(message: str) -> None:
    """Diagnostics go to stderr so they never corrupt the JSON stream."""
    print(f"[sidecar] {message}", file=sys.stderr, flush=True)


def reply(request_id: Any, payload: dict[str, Any]) -> None:
    response = {"id": request_id}
    response.update(payload)
    sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def error(request_id: Any, message: str) -> None:
    reply(request_id, {"ok": False, "error": message})


# --------------------------------------------------------------------- audio


def _devices_via_sounddevice() -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    import sounddevice  # type: ignore

    inputs: list[dict[str, Any]] = []
    outputs: list[dict[str, Any]] = []
    for index, device in enumerate(sounddevice.query_devices()):
        name = str(device.get("name", f"Device {index}"))
        channels = int(device.get("max_input_channels", 0) or 0)
        outputs_count = int(device.get("max_output_channels", 0) or 0)
        entry = {"id": f"sd:{index}:{name}", "name": name, "channels": max(channels, outputs_count)}
        if channels > 0:
            inputs.append(dict(entry, channels=channels))
        if outputs_count > 0:
            outputs.append(dict(entry, channels=outputs_count))
    return inputs, outputs


# ---------------------------------------------------------------- recording


def _resolve_input_index(device_id: str | None) -> int | None:
    """Map an `audio_devices` id onto a backend device index.

    Device ids look like `sd:3:Realtek High Definition Audio`; the `sd` /
    `pa` prefix says which backend produced it, which matters because the two
    backends enumerate independently.
    """
    if not device_id:
        return None
    try:
        backend, raw_index, _name = device_id.split(":", 2)
        index = int(raw_index)
    except (ValueError, AttributeError):
        return None
    return index if backend in ("sd", "pa") else None


def record_audio(device_id: str | None, destination: Path, max_seconds: float) -> dict[str, Any]:
    """Record from a microphone until interrupted, writing a WAV file.

    Uses `sounddevice`'s blocking recorder, which runs on this thread. That is
    safe because the GUI drives start/stop with separate requests and the
    recorder is interrupted between blocks - but it does mean the sidecar cannot
    answer other requests while recording. The GUI knows this and shows the
    recording as a modal state.
    """
    import sounddevice  # type: ignore

    index = _resolve_input_index(device_id)
    if device_id and index is None:
        return {"ok": False, "error": f"cannot resolve input device {device_id!r}"}

    seconds = max(0.1, min(float(max_seconds), MAX_RECORD_SECONDS))
    frames = int(SAMPLE_RATE * seconds)
    destination.parent.mkdir(parents=True, exist_ok=True)

    # `InputStream` rather than `sounddevice.rec`: `rec` returns the samples
    # directly and there is no way to stop it early, so a cancelled recording
    # would still block for the full duration.
    stream = None
    try:
        stream = sounddevice.InputStream(
            samplerate=SAMPLE_RATE,
            channels=CHANNELS,
            dtype="int16",
            device=index,
            blocksize=1024,
        )
        stream.start()
        chunks: list[bytes] = []
        collected = 0
        while collected < frames:
            block, overflowed = stream.read(min(1024, frames - collected))
            if overflowed:
                # The GUI could not drain us fast enough; drop what was lost
                # rather than splicing it into the middle of the audio.
                log("input overflow, dropped a block")
            chunks.append(bytes(block))
            collected += len(block)
    except Exception as err:  # noqa: BLE001 - surfaced to the GUI verbatim
        return {"ok": False, "error": f"recording failed: {err}"}
    finally:
        if stream is not None:
            try:
                stream.stop()
                stream.close()
            except Exception:  # noqa: BLE001 - closing must never mask the real error
                pass

    data = b"".join(chunks)
    # An interrupted recording can come back short or empty; a zero-byte WAV is
    # worse than a clear error, because the next stage would have nothing to say.
    if not data:
        return {"ok": False, "error": "no audio was captured"}

    with wave.open(str(destination), "wb") as handle:
        handle.setnchannels(CHANNELS)
        handle.setsampwidth(SAMPLE_WIDTH)
        handle.setframerate(SAMPLE_RATE)
        handle.writeframes(data)

    return {
        "ok": True,
        "path": str(destination),
        "sample_rate": SAMPLE_RATE,
        "channels": CHANNELS,
        "bytes": len(data),
        "seconds": round(len(data) / (SAMPLE_RATE * CHANNELS * SAMPLE_WIDTH), 3),
    }


def read_wav_duration(path: Path) -> float:
    """Duration of a WAV file in seconds, or 0.0 if it cannot be read."""
    try:
        with wave.open(str(path), "rb") as handle:
            rate = handle.getframerate() or 1
            return handle.getnframes() / rate
    except Exception:  # noqa: BLE001 - diagnostics only
        return 0.0


def _devices_via_pyaudio() -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    import pyaudio  # type: ignore

    audio = pyaudio.PyAudio()
    inputs: list[dict[str, Any]] = []
    outputs: list[dict[str, Any]] = []
    try:
        for index in range(audio.get_device_count()):
            info = audio.get_device_info_by_index(index)
            name = str(info.get("name", f"Device {index}"))
            entry = {"id": f"pa:{index}:{name}", "name": name}
            if int(info.get("maxInputChannels", 0)) > 0:
                inputs.append(dict(entry, channels=int(info["maxInputChannels"])))
            if int(info.get("maxOutputChannels", 0)) > 0:
                outputs.append(dict(entry, channels=int(info["maxOutputChannels"])))
    finally:
        audio.terminate()
    return inputs, outputs


def audio_devices() -> dict[str, Any]:
    """Return {"inputs": [...], "outputs": [...], "backend": "..."}."""
    for backend, loader in (("sounddevice", _devices_via_sounddevice), ("pyaudio", _devices_via_pyaudio)):
        try:
            inputs, outputs = loader()
            return {"inputs": inputs, "outputs": outputs, "backend": backend}
        except Exception as err:  # noqa: BLE001 - any failure means "try the next one"
            log(f"{backend} unavailable: {err}")

    return {
        "inputs": [],
        "outputs": [],
        "backend": "none",
        "hint": "pip install sounddevice  (or pyaudio) to list audio devices",
    }


# ------------------------------------------------------------------ commands


def handle(request: dict[str, Any]) -> tuple[dict[str, Any], bool]:
    """Handle one request.

    Returns ``(payload, should_exit)``.
    """
    request_id = request.get("id")
    command = request.get("cmd")

    if command == "ping":
        return ({"ok": True, "pong": True, "sidecar_version": VERSION, "protocol": PROTOCOL}, False)

    if command == "audio_devices":
        result = audio_devices()
        return ({"ok": True, **result}, False)

    if command == "record":
        destination = Path(str(request.get("path") or ""))
        if not str(destination):
            return ({"ok": False, "error": "record needs a destination path"}, False)
        result = record_audio(
            request.get("device"),
            destination,
            float(request.get("max_seconds") or MAX_RECORD_SECONDS),
        )
        return (result, False)

    if command == "wav_info":
        path = Path(str(request.get("path") or ""))
        if not str(path) or not path.is_file():
            return ({"ok": False, "error": "no such file"}, False)
        return ({"ok": True, "seconds": read_wav_duration(path)}, False)

    if command == "info":
        return (
            {
                "ok": True,
                "python": sys.version.split()[0],
                "platform": platform.platform(),
                "executable": sys.executable,
                "sidecar_version": VERSION,
            },
            False,
        )

    if command == "shutdown":
        return ({"ok": True, "bye": True}, True)

    return ({"ok": False, "error": f"unknown command: {command!r}"}, False)


# --------------------------------------------------------------------- main


def main() -> int:
    global VERSION

    # Device names are frequently non-ASCII ("Primärer Soundtreiber"). Force
    # UTF-8 on both streams so they survive the pipe to the Rust GUI.
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[union-attr]
        except Exception:  # noqa: BLE001 - older interpreters, not fatal
            pass

    parser = argparse.ArgumentParser(description="ProjectRem Python sidecar")
    parser.add_argument("--version", default=VERSION, help="ProjectRem version, passed in by the GUI")
    args = parser.parse_args()
    VERSION = args.version

    log(f"started, ProjectRem {VERSION}, python {platform.python_version()}")

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as err:
            log(f"bad JSON: {err}")
            continue

        try:
            payload, should_exit = handle(request)
        except Exception as err:  # noqa: BLE001 - never let one command kill the sidecar
            log(traceback.format_exc())
            payload, should_exit = {"ok": False, "error": str(err)}, False

        reply(request.get("id"), payload)
        if should_exit:
            log("shutdown requested")
            return 0

    log("stdin closed, exiting")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(0) from None