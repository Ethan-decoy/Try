"""Check a packaged executable, exact embedded notices, and isolated GUI startup."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


def output(command, argument):
    return subprocess.run(
        [*command, argument], check=True, capture_output=True, timeout=60
    ).stdout


def check(command, root):
    info = json.loads(output(command, "--distribution-info"))
    commit = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if info.get("source_commit") != commit or info.get("source_state") != "clean":
        raise RuntimeError("Packaged executable must match the clean release source commit")
    bundle = json.loads(
        (root / "local_artifacts/open_source/embedded_notices.json").read_text(
            encoding="utf-8-sig"
        )
    )
    lock_hash = hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest()
    if (
        info.get("format") != "try_distribution_info"
        or not info.get("notices_complete")
        or info.get("lock_sha256") != lock_hash
        or bundle["lock_sha256"] != lock_hash
        or info["components"] != len(bundle["components"])
        or info["components"] <= 3
    ):
        raise RuntimeError("Distribution metadata or notice inventory is inconsistent")
    expected = []
    for component in bundle["components"]:
        expected.append(
            f'{component["name"]} {component["version"]}\n'
            f'License: {component["license"]}\nSource: {component["source"]}\n'
        )
        for document in component["documents"]:
            expected.append(f'\n---------- {document["name"]} ----------\n\n')
            expected.append(document["text"] + "\n")
        expected.append("\n")
    notices = output(command, "--third-party-notices")
    if notices != "".join(expected).encode("utf-8"):
        raise RuntimeError("Packaged notices differ from the original notice collection")
    info["notice_bytes"] = len(notices)
    info["notice_sha256"] = hashlib.sha256(notices).hexdigest()
    return info


def smoke(command, root):
    records = root / "local_artifacts/unix_checks"
    records.mkdir(parents=True, exist_ok=True)
    # No existing user profile, autostart entry, or daily instance is touched.
    with tempfile.TemporaryDirectory(prefix="profile_", dir=records) as profile:
        profile = Path(profile)
        for mode in ("main", "practice"):
            with (records / f"{mode}_stdout.log").open("wb") as stdout, (
                records / f"{mode}_stderr.log"
            ).open("wb") as stderr:
                process = subprocess.Popen(
                    [*command, "--data-dir", str(profile / mode), "--diagnostics"]
                    + (["--practice"] if mode == "practice" else []),
                    stdout=stdout,
                    stderr=stderr,
                    env={**os.environ, "TRY_LOG": "warn,try=debug"},
                    start_new_session=True,
                )
                try:
                    deadline = time.monotonic() + 15
                    event = f"event={mode}_shown"
                    while time.monotonic() < deadline:
                        if process.poll() is not None:
                            raise RuntimeError(
                                f"{mode} startup exited with {process.returncode}: "
                                + (records / f"{mode}_stderr.log").read_text(
                                    encoding="utf-8", errors="replace"
                                )
                            )
                        logs = list((profile / mode / "logs").glob("*.log"))
                        if any(event in path.read_text(encoding="utf-8") for path in logs):
                            time.sleep(2)
                            if process.poll() is not None:
                                raise RuntimeError(f"{mode} exited after showing the window")
                            break
                        time.sleep(0.25)
                    else:
                        raise RuntimeError(f"Missing {event} after startup")
                finally:
                    # An extracting AppImage can have a launcher and app child.
                    # Stop only this probe's process group before deleting its profile.
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        process.wait(timeout=5)
    return ["main_shown", "practice_shown"]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()
    command = arguments.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        parser.error("Provide a packaged executable after --")
    root = Path(__file__).resolve().parent.parent
    destination = Path(arguments.output).resolve()
    if not destination.is_relative_to(root / "local_artifacts"):
        raise ValueError("Check records must stay within local_artifacts")
    info = check(command, root)
    if arguments.smoke:
        info["startup_checks"] = smoke(command, root)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(info, indent=2))


if __name__ == "__main__":
    main()
