#!/usr/bin/env python3
"""Run the Rust checks and gameplay tests on this computer."""

import argparse
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quick", action="store_true", help="run the four basic gameplay cases")
    parser.add_argument("--jobs", type=int, default=2, help="simultaneous game launches (default: 2)")
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be at least 1")

    root = Path(__file__).resolve().parents[1]
    target = root / "builds"
    cargo = ["--profile", "ci", "--locked", "--offline", "--target-dir", str(target)]
    binary = "car-game.exe" if os.name == "nt" else "car-game"

    subprocess.run(["cargo", "fmt", "--all", "--check"], cwd=root, check=True)
    subprocess.run(["cargo", "clippy", *cargo, "--workspace", "--all-targets", "--", "-D", "warnings"], cwd=root, check=True)
    subprocess.run(["cargo", "test", *cargo, "--workspace"], cwd=root, check=True)
    # Reuse the test build's dependencies for the playable executable.
    subprocess.run(["cargo", "build", *cargo, "--package", "car_game"], cwd=root, check=True)

    runs = target / "test-runs"
    runs.mkdir(exist_ok=True)
    output = tempfile.mkdtemp(dir=runs)
    smoke = [sys.executable, str(root / "tools/packaging/smoke.py"), str(target / "ci" / binary), output, "--jobs", str(args.jobs)]
    if not args.quick:
        smoke.append("--full")
    print(f"Game logs and screenshots: {output}", flush=True)
    return subprocess.run(smoke, cwd=root).returncode


if __name__ == "__main__":
    try:
        sys.exit(main())
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
