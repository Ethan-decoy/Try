"""Exercise real updater handoff, restart and rollback with isolated user data."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import time
import zlib


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def spawn(command, **kwargs):
    options = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {"start_new_session": True}
    return subprocess.Popen(command, **options, **kwargs)


def stop(pid, allowed_root):
    if not pid or pid == os.getpid():
        return
    try:
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes
            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            kernel.OpenProcess.restype = wintypes.HANDLE
            kernel.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
            kernel.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
            kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
            kernel.CloseHandle.argtypes = [wintypes.HANDLE]
            handle = kernel.OpenProcess(0x1000 | 0x100000 | 1, False, pid)
            if not handle:
                return
            try:
                buffer = ctypes.create_unicode_buffer(32768)
                size = wintypes.DWORD(len(buffer))
                if not kernel.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(size)):
                    return
                image = os.path.normcase(buffer.value.removeprefix("\\\\?\\"))
                root = os.path.normcase(str(allowed_root.resolve()).removeprefix("\\\\?\\"))
                if os.path.commonpath([image, root]) != root:
                    raise RuntimeError("Refusing to stop a process outside this test profile")
                kernel.TerminateProcess(handle, 1)
                kernel.WaitForSingleObject(handle, 5000)
            finally:
                kernel.CloseHandle(handle)
        else:
            os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    except OSError:
        pass


def wait_for(predicate, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    raise RuntimeError("Updater check timed out")


def png_fixture():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"\0\x44\x88\xcc\xff")) + chunk(b"IEND", b"")


def check_update_log(data, tag, rollback, nonce):
    text = "".join(path.read_text(encoding="utf-8") for path in (data / "logs").glob("*.log"))
    assert f"event=install_started target={tag}" in text, "Missing installation summary"
    assert nonce not in text, "Update credentials must not be logged"
    if rollback:
        assert f"event=install_failed target={tag} stage=startup_confirmation" in text
        assert f"event=rollback_started target={tag}" in text
        assert f"event=rollback_succeeded target={tag}" in text
    else:
        assert "event=updated_startup_confirmed version=" in text


def parent_handoff(folder, driver, target, source, data, tracked, environment):
    shutil.copy2(driver, target)
    process = spawn([str(target), str(source), str(data)], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    tracked.append(process.pid)
    def staged_job():
        return next((path for path in folder.glob(".try_update_*") if (path / "job.json").is_file()), None)
    root = wait_for(staged_job)
    job = json.loads((root / "job.json").read_text(encoding="utf-8"))
    helper_pid = int(wait_for(lambda: (root / "helper_pid").read_text() if (root / "helper_pid").is_file() else None))
    tracked.append(helper_pid)
    child_pid = int(wait_for(lambda: (root / "child_pid").read_text() if (root / "child_pid").is_file() else None))
    tracked.append(child_pid)
    process.wait(timeout=20)
    assert process.returncode == 0, process.stderr.read().decode(errors="replace")
    wait_for(lambda: not root.exists(), seconds=15)
    assert sha(target) == sha(source)
    check_update_log(data, job["tag"], False, job["nonce"])


def check_case(folder, source, kind, helper_source, version, rollback, driver):
    target = folder / {"windows": "try.exe", "linux": "Try.AppImage", "macos": "Try.app"}[kind]
    if kind == "macos":
        shutil.copytree(source, target)
        executable = target / "Contents/MacOS/try"
    else:
        shutil.copy2(source, target)
        executable = target
        # Distinguish old and new bytes while keeping an unsigned PE/ELF runnable.
        with target.open("ab") as stream:
            stream.write(b"try_update_old_fixture")
    environment = {**os.environ}
    environment.pop("TRY_LOG", None)
    if kind == "linux":
        environment["APPIMAGE_EXTRACT_AND_RUN"] = "1"
    data = folder / "data"
    initial = spawn([str(executable), "--background", "--data-dir", str(data)], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    tracked = [initial.pid]
    helper = None
    try:
        wait_for(lambda: (data / "state.json").is_file())
        stop(initial.pid, folder)
        initial.wait(timeout=10)
        state = json.loads((data / "state.json").read_text(encoding="utf-8"))
        state["settings"].update({"interval_minutes": 47, "appearance": "light", "interface_language": "en", "auto_check_updates": False})
        picture = png_fixture()
        image_id = hashlib.sha256(picture).hexdigest()
        (data / "media").mkdir(exist_ok=True)
        (data / "media" / f"{image_id}.png").write_bytes(picture)
        state["packs"][0]["cards"][0]["prompt_image"] = image_id
        (data / "state.json").write_text(json.dumps(state, ensure_ascii=False), encoding="utf-8")
        learning = {"format": "try_learning", "schema_version": 1, "model_version": "fsrs6_default_v6_6_2", "time_adapter": "elapsed_whole_days_v1", "presentations": 42, "packs": {}, "recent": [], "reviews": []}
        (data / "learning_state.json").write_text(json.dumps(learning), encoding="utf-8")
        original = {name: (data / name).read_bytes() for name in ("state.json", "learning_state.json")}
        if driver and not rollback:
            assert kind == "windows"
            parent_handoff(folder, driver, target, source, data, tracked, environment)
            assert json.loads((data / "state.json").read_text(encoding="utf-8")) == state
            assert (data / "learning_state.json").read_bytes() == original["learning_state.json"]
            assert (data / "media" / f"{image_id}.png").read_bytes() == picture
            return {"case": "parent_handoff_update_restart", "passed": True, "data_preserved": ["settings", "packs", "learning", "images"], "temporary_helper_cleaned": True, "logging_verified": True}
        old_hash = sha(executable)
        parent = spawn([str(executable), "--background", "--data-dir", str(data)], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        tracked.append(parent.pid)
        time.sleep(0.7)
        assert parent.poll() is None, "Fixture application failed to start"
        root = Path(tempfile.mkdtemp(prefix=".try_update_", dir=folder))
        payload = root / {"windows": "payload.exe", "linux": "payload.AppImage", "macos": "Try.app"}[kind]
        if kind == "macos":
            shutil.copytree(source, payload)
            payload_executable = payload / "Contents/MacOS/try"
        else:
            shutil.copy2(source, payload)
            payload_executable = payload
        helper_path = root / ("helper.exe" if kind == "windows" else "helper")
        shutil.copy2(helper_source or executable, helper_path)
        helper_path.chmod(0o755)
        nonce = secrets.token_hex(32)
        job = {"schema": 1, "tag": "v99.0.0" if rollback else "v" + version, "kind": kind, "target": str(target.resolve()), "launch_target": str(target.resolve()), "data": str(data.resolve()), "parent_pid": parent.pid, "nonce": nonce, "payload_sha256": sha(payload_executable), "helper_sha256": sha(helper_path), "old_sha256": old_hash, "saved_files": []}
        job_path = root / "job.json"
        job_path.write_text(json.dumps(job), encoding="utf-8")
        helper = spawn([str(helper_path), "--apply-update", str(job_path)], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        tracked.append(helper.pid)
        assert helper.stdout.read(17) == b"try-update-ready\n", "Helper refused the validated handoff"
        stop(parent.pid, folder)
        parent.wait(timeout=10)
        helper.stdin.write(nonce.encode())
        helper.stdin.close()
        child_pid = int(wait_for(lambda: (root / "child_pid").read_text() if (root / "child_pid").is_file() else None))
        tracked.append(child_pid)
        if rollback:
            recovered_pid = int(wait_for(lambda: (root / "rollback_pid").read_text() if (root / "rollback_pid").is_file() else None, seconds=40))
            tracked.append(recovered_pid)
            helper.wait(timeout=10)
            assert helper.returncode != 0
            assert sha(executable) == old_hash, "Previous program was not restored"
            assert (root / "rolled_back").is_file()
        else:
            helper.wait(timeout=40)
            assert helper.returncode == 0, (helper.stderr.read().decode(errors="replace"))
            assert sha(executable) == sha(source / "Contents/MacOS/try" if kind == "macos" else source)
            wait_for(lambda: not root.exists(), seconds=15)
        for name, value in original.items():
            assert (data / name).read_bytes() == value, f"Personal data changed: {name}"
        assert (data / "media" / f"{image_id}.png").read_bytes() == picture
        assert json.loads((data / "state.json").read_text(encoding="utf-8"))["settings"]["interval_minutes"] == 47
        check_update_log(data, job["tag"], rollback, nonce)
        return {"case": "startup_failure_rollback" if rollback else "update_restart", "passed": True, "data_preserved": ["settings", "packs", "learning", "images"], "temporary_helper_cleaned": not root.exists(), "logging_verified": True}
    finally:
        for pid in reversed(tracked):
            stop(pid, folder)
        if helper and helper.poll() is None:
            helper.kill()
        time.sleep(0.5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", required=True, type=Path, help="try.exe, Try.app or an AppImage")
    parser.add_argument("--helper", type=Path, help="Native executable inside an AppImage; unnecessary for Windows/macOS")
    parser.add_argument("--driver", type=Path, help="Windows parent-handoff fixture built with native_checks,distribution")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source = args.executable.resolve()
    kind = "windows" if sys.platform == "win32" else "macos" if sys.platform == "darwin" else "linux"
    info_command = source / "Contents/MacOS/try" if kind == "macos" else source
    environment = {**os.environ, **({"APPIMAGE_EXTRACT_AND_RUN": "1"} if kind == "linux" else {})}
    info = json.loads(subprocess.check_output([str(info_command), "--distribution-info"], env=environment, timeout=30))
    assert info["version"] and info["format"] == "try_distribution_info"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    results = []
    root = Path(tempfile.mkdtemp(prefix="updater_check_", dir=args.output.parent)).resolve()
    try:
        for rollback in (False, True):
            folder = root / ("rollback" if rollback else "success")
            folder.mkdir()
            results.append(check_case(folder, source, kind, args.helper, info["version"], rollback, args.driver))
            args.output.with_suffix(".partial.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    finally:
        assert root.parent == args.output.parent.resolve() and root.name.startswith("updater_check_")
        for attempt in range(50):
            try:
                shutil.rmtree(root)
                break
            except PermissionError:
                if attempt == 49:
                    raise
                time.sleep(0.1)
    report = {"passed": True, "version": info["version"], "platform": kind, "cases": results, "isolated_data": True}
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
