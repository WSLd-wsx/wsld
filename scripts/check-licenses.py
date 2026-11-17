#!/usr/bin/env python3
"""Check the reviewed public license inventory offline; no compatibility inference."""
import hashlib
import json
from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PROJECT_LICENSE = "BUSL-1.1"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def checked_file(relative, expected):
    path = (ROOT / relative).resolve()
    require(path.is_relative_to(ROOT), f"Notice path escapes repository: {relative}")
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    require(actual == expected, f"License inventory hash mismatch: {relative}")


def main():
    inventory = json.loads((ROOT / "licenses/dependency-inventory.json").read_text())
    require(inventory["schema_version"] == 1, "Unsupported license inventory schema")
    checked_file("Cargo.lock", inventory["cargo_lock_sha256"])
    checked_file(inventory["project_license"]["path"], inventory["project_license"]["sha256"])
    for entry in inventory["bundled_notices"] + inventory["bundled_data"]:
        checked_file(entry["path"], entry["sha256"])
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    expected = {
        (p["name"], p["version"], p["checksum"])
        for p in lock["package"] if "source" in p
    }
    packages = inventory["packages"]
    actual = {(p["name"], p["version"], p["checksum"]) for p in packages}
    require(actual == expected, "Dependency license inventory differs from Cargo.lock")
    require(len(actual) == len(packages), "Duplicate dependency in license inventory")
    notices = set()
    for package in packages:
        require(package["declared_license"] and package["notices"],
                f"Missing license evidence: {package['name']}")
        for notice in package["notices"]:
            checked_file(notice["path"], notice["sha256"])
            notices.add(notice["path"])
    retained = {p.relative_to(ROOT).as_posix()
                for p in (ROOT / "licenses/dependencies").rglob("*") if p.is_file()}
    require(notices == retained, "Unlisted or missing dependency notice file")
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]
    require(workspace["package"]["license"] == PROJECT_LICENSE,
            "Workspace license differs from the owner's selection")
    checked_file(inventory["core_license"]["path"], inventory["core_license"]["sha256"])
    checked_file(inventory["core_change_license"]["path"], inventory["core_change_license"]["sha256"])
    for member in workspace["members"]:
        manifest = tomllib.loads((ROOT / member / "Cargo.toml").read_text())
        expected_license = "BUSL-1.1" if member == "core" else {"workspace": True}
        require(manifest["package"].get("license") == expected_license,
                f"Package does not inherit the reviewed license: {member}")
    for path in ("NOTICE", "LICENSING.md", "THIRD-PARTY-NOTICES.md", "core/LICENSE", "core/CHANGE-LICENSE-APACHE-2.0.txt",
                 "core/data/UNICODE-LICENSE.txt", "integration/LICENSE"):
        require((ROOT / path).is_file(), f"Missing distribution notice: {path}")
    print(f"License inventory verified: {len(packages)} locked packages, "
          f"{len(notices)} dependency notices; external components are outside this check.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        print(f"License inventory check failed: {error}", file=sys.stderr)
        sys.exit(1)
