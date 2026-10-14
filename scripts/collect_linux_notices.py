"""Preserve distro copyright texts for shared libraries included in the AppDir."""

import json
from pathlib import Path
import shutil
import subprocess
import sys

appdir = Path(sys.argv[1]).resolve()
root = Path(__file__).resolve().parent.parent
if not appdir.is_relative_to(root / "local_artifacts"):
    raise ValueError("AppDir must stay within local_artifacts")
packages = {}
system_libraries = subprocess.run(
    ["ldconfig", "-p"], check=True, capture_output=True, text=True
).stdout
for library in sorted((appdir / "usr/lib").glob("**/*.so*")):
    if not library.is_file():
        continue
    candidates = [line.split(" => ", 1)[1] for line in system_libraries.splitlines()
                  if line.strip().startswith(library.name + " ") and " => " in line]
    if not candidates:
        raise RuntimeError(f"Cannot identify bundled system library: {library.name}")
    source = Path(candidates[0])
    owners = [subprocess.run(
        ["dpkg-query", "--search", str(path)], capture_output=True, text=True
    ) for path in (source, source.resolve())]
    owner = next((item.stdout.split(": ", 1)[0] for item in owners
                  if item.returncode == 0), None)
    if owner is None:
        raise RuntimeError(f"Cannot find distro package for {source}")
    package = owner.split(":", 1)[0]
    if package in packages:
        continue
    copyright_file = Path("/usr/share/doc") / package / "copyright"
    if not copyright_file.is_file():
        raise RuntimeError(f"Missing distro copyright text: {package}")
    destination = appdir / "usr/share/doc/try/bundled_libraries" / package
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "copyright").write_bytes(copyright_file.read_bytes())
    metadata = subprocess.run(
        ["dpkg-query", "--show", "--showformat=${Version}\t${source:Package}\t${source:Version}", owner],
        check=True, capture_output=True, text=True
    ).stdout.split("\t")
    source_name = metadata[1] or package
    source_version = metadata[2] or metadata[0]
    packages[package] = {
        "version": metadata[0], "copyright": f"{package}/copyright",
        "source": f"https://launchpad.net/ubuntu/+source/{source_name}/{source_version}",
    }
(appdir / "usr/share/doc/try/bundled_libraries/inventory.json").write_text(
    json.dumps(packages, indent=2) + "\n", encoding="utf-8"
)
# Distro copyright files can refer to these shared license texts by absolute path.
shutil.copytree("/usr/share/common-licenses",
                appdir / "usr/share/doc/try/bundled_libraries/common_licenses")
print(f"Preserved copyright texts for {len(packages)} bundled library packages")
