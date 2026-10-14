"""Preserve upstream runtime notices and source for its statically linked libfuse."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import tarfile
import urllib.request


SOURCES = [
    ("runtime_source.tar.gz", "https://codeload.github.com/AppImage/type2-runtime/tar.gz/8f39b89e2ac31e1640b3d3f7e9a5108e6ce805fa", "c449eb42c2190820872631f5e22f8e15b1e985a36dc4a9e2de3585bb7536c785"),
    ("fuse_3_15_0.tar.xz", "https://github.com/libfuse/libfuse/releases/download/fuse-3.15.0/fuse-3.15.0.tar.xz", "70589cfd5e1cff7ccd6ac91c86c01be340b227285c5e200baa284e401eea2ca0"),
    ("squashfuse_0_5_2.tar.gz", "https://github.com/vasi/squashfuse/archive/0.5.2.tar.gz", "db0238c5981dabbd80ee09ae15387f390091668ca060a7bc38047912491443d3"),
    ("musl_copyright.txt", "https://git.musl-libc.org/cgit/musl/plain/COPYRIGHT?h=v1.2.5", "f9bc4423732350eb0b3f7ed7e91d530298476f8fec0c6c427a1c04ade22655af"),
    ("zstd_license.txt", "https://raw.githubusercontent.com/facebook/zstd/v1.5.6/LICENSE", "7055266497633c9025b777c78eb7235af13922117480ed5c674677adc381c9d8"),
    ("zlib_license.txt", "https://raw.githubusercontent.com/madler/zlib/v1.3.1/LICENSE", "845efc77857d485d91fb3e0b884aaa929368c717ae8186b66fe1ed2495753243"),
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", required=True)
    parser.add_argument("--notice-dir", required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parent.parent / "local_artifacts"
    source_dir = Path(arguments.source_dir).resolve()
    notice_dir = Path(arguments.notice_dir).resolve()
    for directory in (source_dir, notice_dir):
        if not directory.is_relative_to(root):
            raise ValueError("Runtime preparation must stay within local_artifacts")
        directory.mkdir(parents=True, exist_ok=True)
    inventory = []
    for name, url, digest in SOURCES:
        source = source_dir / name
        if not source.exists():
            request = urllib.request.Request(url, headers={"User-Agent": "try-runtime-notices"})
            with urllib.request.urlopen(request, timeout=30) as response:
                source.write_bytes(response.read())
        if hashlib.sha256(source.read_bytes()).hexdigest() != digest:
            raise RuntimeError(f"Runtime source checksum mismatch: {name}")
        documents = []
        if name.endswith((".tar.gz", ".tar.xz")):
            with tarfile.open(source) as archive:
                for member in archive.getmembers():
                    path = Path(member.name)
                    if (member.isfile() and len(path.parts) == 2
                            and re.match(r"^(license|copying|copyright|lgpl|gpl)", path.name, re.I)):
                        target = f"{name.split('.')[0]}_notice_{len(documents):02}.txt"
                        notice = archive.extractfile(member)
                        if notice is None:
                            raise RuntimeError(f"Missing runtime notice: {member.name}")
                        (notice_dir / target).write_bytes(notice.read())
                        documents.append({"original": member.name, "bundled": target})
        else:
            (notice_dir / name).write_bytes(source.read_bytes())
            documents.append({"original": url, "bundled": name})
        if not documents:
            raise RuntimeError(f"No runtime license documents in {name}")
        inventory.append({"file": name, "source": url, "sha256": digest, "documents": documents})
    manifest = json.dumps(inventory, indent=2) + "\n"
    (source_dir / "inventory.json").write_text(manifest, encoding="utf-8")
    (notice_dir / "inventory.json").write_text(manifest, encoding="utf-8")
    (source_dir / "readme.txt").write_text(
        "Sources accompany Try's AppImage runtime, separate from the Rust application.\n"
        "The runtime source archive includes upstream build scripts and the libfuse patch.\n"
        "Its scripts/common/install-dependencies.sh documents static libfuse and squashfuse builds;\n"
        "scripts/docker and scripts/chroot document the Alpine build environment and relinking.\n"
        "Original source archives and license texts are unchanged and SHA-256 checked.\n",
        encoding="utf-8",
    )
    print(f"Prepared {len(inventory)} runtime source/notice records")


if __name__ == "__main__":
    main()
