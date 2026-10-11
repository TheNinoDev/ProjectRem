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
import sys
import traceback
from typing import Any

VERSION = "0.0.0"
PROTOCOL = 1


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