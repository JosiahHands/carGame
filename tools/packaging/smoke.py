#!/usr/bin/env python3
"""Launch the packaged game with isolated data and capture real rendered frames."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import signal
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
    # Reject flat frames captured while the graphics pipelines are still loading.
    if len(data) < width * height // 32:
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
    use_process_group = os.name == "posix"
    with (directory / "game.log").open("wb") as log:
        with subprocess.Popen(
            [str(executable)],
            env=game_environment,
            cwd=directory,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=use_process_group,
        ) as process:
            try:
                exit_code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                if use_process_group:
                    # The AppImage runtime starts the game as a child process.
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                else:
                    process.kill()
                raise
    if exit_code != 0:
        raise RuntimeError(f"game exited with code {exit_code}")
    with (directory / "game.log").open(encoding="utf-8", errors="replace") as log:
        for line in log:
            if any(message in line for message in ("panicked at ", "Validation Error", "Vertex_Color has a different vertex count")):
                raise RuntimeError(f"game logged an error: {line.strip()}")
    return check_png(screenshot)


def cases(full):
    yield "menu", {}
    yield "city-packs", {
        "env": {"CAR_GAME_MODE": "drive", "CAR_GAME_MAP": "city", "CAR_GAME_AUTOPILOT": "0"},
        "packs": ["npc", "physics"],
    }
    yield "same-map-stunt", {"env": {"CAR_GAME_LEVEL": "quick:stunt:city", "CAR_GAME_AUTOPILOT": "0"}}
    city = {"CAR_GAME_MODE": "drive", "CAR_GAME_MAP": "city"}
    yield "on-foot-pistol", {"env": city | {"CAR_GAME_FOOT": "1", "CAR_GAME_FIRE": "pistol"}}
    if full:
        yield "open-world-highway", {"env": city | {"CAR_GAME_AT": "road:1:0.5", "CAR_GAME_AUTOPILOT": "0"}}
        yield "open-world-town", {"env": city | {"CAR_GAME_AT": "town:Red Rock", "CAR_GAME_AUTOPILOT": "0"}, "packs": ["npc"]}
        yield "big-map", {"env": city | {"CAR_GAME_BIGMAP": "1"}}
        yield "bayport-and-controls", {"env": city | {"CAR_GAME_AT": "town:Bayport", "CAR_GAME_GARAGE": "controls"}}
        yield "generative-map", {
            "env": city | {"CAR_GAME_AT": "9000,300,1.57", "CAR_GAME_AUTOPILOT": "0"},
            "settings": {"maps": {"city": {"generative": True, "weather": "rain"}}},
        }
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


def check_case(executable, directory, name, scenario, timeout):
    try:
        dimensions = run(executable, directory, scenario, timeout)
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        message = f"FAIL {name}: {error} ({directory})"
        log_path = directory / "game.log"
        if log_path.is_file():
            with log_path.open("rb") as log:
                log.seek(max(0, log_path.stat().st_size - 3500))
                message += "\n" + log.read().decode("utf-8", errors="replace")
        print(message, file=sys.stderr, flush=True)
        return 1
    print(f"PASS {name}: {dimensions} ({directory})", flush=True)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--full", action="store_true")
    parser.add_argument("--timeout", type=int, default=90, help="seconds per launch")
    parser.add_argument("--jobs", type=int, default=1, help="simultaneous game launches")
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be at least 1")
    executable = args.executable.resolve(strict=True)
    output = args.output.resolve()
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        checks = [
            pool.submit(check_case, executable, output / name, name, scenario, args.timeout)
            for name, scenario in cases(args.full)
        ]
        return max(check.result() for check in checks)


if __name__ == "__main__":
    sys.exit(main())
