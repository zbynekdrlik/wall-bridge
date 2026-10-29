#!/usr/bin/env python3
"""Version gate: the workspace version on this branch must be greater than on the base branch.

Single source: [workspace.package].version in Cargo.toml. A base without a workspace
version (the seed MVP used [package].version) is read from [package].version, else 0.0.0.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


def parse(version: str) -> tuple[int, int, int]:
    match = SEMVER.match(version)
    if not match:
        raise ValueError(f"not a plain SemVer version: {version!r}")
    return int(match[1]), int(match[2]), int(match[3])


def version_of(cargo_toml: str) -> str:
    data = tomllib.loads(cargo_toml)
    ws = data.get("workspace", {}).get("package", {}).get("version")
    return ws or data.get("package", {}).get("version") or "0.0.0"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base-ref", required=True)
    args = ap.parse_args()
    head = version_of(Path("Cargo.toml").read_text())
    try:
        base_toml = subprocess.run(
            ["git", "show", f"{args.base_ref}:Cargo.toml"], check=True, capture_output=True, text=True
        ).stdout
        base = version_of(base_toml)
    except subprocess.CalledProcessError:
        base = "0.0.0"
    print(f"head {head}, base {args.base_ref} {base}")
    if parse(head) <= parse(base):
        print(f"::error::version {head} must be greater than {base} on {args.base_ref}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
