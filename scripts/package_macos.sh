#!/usr/bin/env bash
set -euo pipefail

task_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$task_root"
task_platform=${1:?Specify macos_arm64}
case "$task_platform" in
    macos_arm64) task_arch=arm64; task_minimum=11.0 ;;
    *) echo "Official macOS packages support arm64 only" >&2; exit 1 ;;
esac
task_output="$task_root/local_artifacts/releases/$task_platform"
if [ -e "$task_output" ]; then
    echo "Release directory already exists; inspect it before replacing it" >&2
    exit 1
fi
task_app="$task_output/Try.app"
mkdir -p "$task_app/Contents/MacOS" "$task_app/Contents/Resources" "$task_output/download"
cp local_artifacts/target/release/try "$task_app/Contents/MacOS/try"
chmod +x "$task_app/Contents/MacOS/try"

task_icon=$(python3 -c 'from pathlib import Path; print(max(Path("local_artifacts/target/release/build").glob("try-*/out/try_1024.png"), key=lambda p: p.stat().st_mtime))')
task_iconset="$task_output/try.iconset"
mkdir "$task_iconset"
for task_size in 16 32 128 256 512; do
    sips -z "$task_size" "$task_size" "$task_icon" --out "$task_iconset/icon_${task_size}x${task_size}.png" >/dev/null
    task_double=$((task_size * 2))
    sips -z "$task_double" "$task_double" "$task_icon" --out "$task_iconset/icon_${task_size}x${task_size}@2x.png" >/dev/null
done
iconutil -c icns "$task_iconset" -o "$task_app/Contents/Resources/try.icns"
task_version=$("$task_app/Contents/MacOS/try" --distribution-info | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')
python3 - "$task_app/Contents/Info.plist" "$task_version" "$task_minimum" <<'PY'
import plistlib
from pathlib import Path
import sys
Path(sys.argv[1]).write_bytes(plistlib.dumps({
    "CFBundleName": "Try", "CFBundleDisplayName": "Try",
    "CFBundleIdentifier": "io.github.ethan-decoy.try",
    "CFBundleExecutable": "try", "CFBundlePackageType": "APPL",
    "CFBundleIconFile": "try.icns",
    "CFBundleShortVersionString": sys.argv[2], "CFBundleVersion": sys.argv[2],
    "LSMinimumSystemVersion": sys.argv[3], "NSHighResolutionCapable": True,
}))
PY
plutil -lint "$task_app/Contents/Info.plist"
lipo "$task_app/Contents/MacOS/try" -verify_arch "$task_arch"
# Only Apple system frameworks/libraries may remain outside this bundle.
otool -L "$task_app/Contents/MacOS/try"
otool -L "$task_app/Contents/MacOS/try" | tail -n +2 | awk '{print $1}' | while IFS= read -r task_library; do
    case "$task_library" in /System/Library/*|/usr/lib/*) ;; *) echo "Unexpected external library: $task_library" >&2; exit 1 ;; esac
done
# Ad-hoc signing verifies bundle integrity; it is not Developer ID notarization.
codesign --force --sign - "$task_app"
codesign --verify --deep --strict "$task_app"
python3 scripts/check_unix_release.py --smoke --output "$task_output/check.json" -- "$task_app/Contents/MacOS/try"
task_zip="$task_output/download/try_${task_platform}_${task_version}.zip"
ditto -c -k --sequesterRsrc --keepParent "$task_app" "$task_zip"
(cd "$task_output/download" && shasum -a 256 "$(basename "$task_zip")" > "$(basename "$task_zip").sha256")
echo "Packaged: $task_zip"
