#!/usr/bin/env python3
"""Launch the packaged game with isolated data and capture real rendered frames."""

import argparse
import json
import os
from pathlib import Path
import struct
import subprocess
import sys

SETTINGS = {
    "quality": "Low",
    "resolution": 0.5,
    "antialias": False,
    "shadows": "off",
    "effects": "low",
    "showFps": True,
    "viewDistance": 200,
    "fpsLimit": 30,
    "announcer": False,
}
MAPS = ["city", "circuit", "oval", "canyon", "alpine", "neon", "forest", "kart"]
EVENTS = {
    "race": "oval",
    "elimination": "circuit",
    "duel": "kart",
    "timeAttack": "forest",
    "takedown": "city",
    "drift": "neon",
    "speedTrap": "canyon",
    "destruction": "city",
    "stunt": "city",
}


def check_png(path):
    data = path.read_bytes()
    if data[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" or not data.endswith(b"IEND\xaeB\x60\x82"):
        raise ValueError("incomplete PNG")
    width, height = struct.unpack(">II", data[16:24])
    if width < 320 or height < 180:
        raise ValueError(f"unexpected screenshot dimensions {width}x{height}")
    # A flat clear-color frame compresses far below this ratio.
    if len(data) < width * height // 64:
        raise ValueError("the screenshot has too little detail for a game frame")
    return f"{width}x{height}"


def run(executable, directory, scenario, timeout):
    directory.mkdir(parents=True)
    settings = SETTINGS | scenario.get("settings", {})
    saves = {
        "carGame.settings.v1": json.dumps(settings),
        "carGame.packs.v2": json.dumps({"on": scenario.get("packs", [])}),
        "carGame.pack.npc.local": json.dumps("off"),
    }
    (directory / "saves.json").write_text(json.dumps(saves), encoding="utf-8")
    screenshot = directory / "frame.png"
    game_environment = {key: value for key, value in os.environ.items() if not key.startswith("CAR_GAME_")}
    game_environment.update(scenario.get("env", {}))
    if "CAR_GAME_LEVEL" in game_environment:
        game_environment["CAR_GAME_SCREENSHOT_FRAMES"] = "240"
    game_environment.update(CAR_GAME_DATA=str(directory), CAR_GAME_SCREENSHOT=str(screenshot))
    with (directory / "game.log").open("wb") as log:
        result = subprocess.run(
            [str(executable)],
            env=game_environment,
            cwd=directory,
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            check=False,
        )
    if result.returncode != 0:
        raise RuntimeError(f"game exited with code {result.returncode}")
    log_text = (directory / "game.log").read_text(encoding="utf-8", errors="replace")
    if "panicked at " in log_text or "Validation Error" in log_text:
        raise RuntimeError("game logged a panic or graphics validation error")
    return check_png(screenshot)


def cases(full):
    yield "menu", {}
    yield "city-packs", {
        "env": {"CAR_GAME_MODE": "drive", "CAR_GAME_MAP": "city", "CAR_GAME_AUTOPILOT": "0"},
        "packs": ["npc", "physics"],
    }
    yield "same-map-stunt", {"env": {"CAR_GAME_LEVEL": "quick:stunt:city", "CAR_GAME_AUTOPILOT": "0"}}
    if full:
        for venue in MAPS:
            yield f"drive-{venue}", {"env": {"CAR_GAME_MODE": "drive", "CAR_GAME_MAP": venue, "CAR_GAME_AUTOPILOT": "0"}}
        for event, venue in EVENTS.items():
            if event != "stunt":
                yield f"event-{event}", {"env": {"CAR_GAME_LEVEL": f"quick:{event}:{venue}", "CAR_GAME_AUTOPILOT": "0"}}
        yield "full-resolution-photo-real", {
            "env": {"CAR_GAME_LEVEL": "quick:race:oval"},
            "settings": {"quality": "High", "resolution": 1.0, "shadows": "high"},
            "packs": ["graphics", "physics"],
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--full", action="store_true")
    parser.add_argument("--timeout", type=int, default=90, help="seconds per launch")
    args = parser.parse_args()
    executable = args.executable.resolve(strict=True)
    failures = 0
    for name, scenario in cases(args.full):
        directory = args.output.resolve() / name
        try:
            dimensions = run(executable, directory, scenario, args.timeout)
        except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
            print(f"FAIL {name}: {error} ({directory})", file=sys.stderr, flush=True)
            if (directory / "game.log").is_file():
                print((directory / "game.log").read_text(encoding="utf-8", errors="replace")[-3500:], file=sys.stderr, flush=True)
            failures += 1
        else:
            print(f"PASS {name}: {dimensions} ({directory})", flush=True)
    return bool(failures)


if __name__ == "__main__":
    sys.exit(main())
