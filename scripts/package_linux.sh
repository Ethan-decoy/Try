#!/usr/bin/env bash
set -euo pipefail

task_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$task_root"
task_output="$task_root/local_artifacts/releases/linux_x64"
if [ -e "$task_output" ]; then
    echo "Release directory already exists; inspect it before replacing it" >&2
    exit 1
fi
task_appdir="$task_output/appdir"
task_tools="$task_output/tools"
mkdir -p "$task_appdir/usr/bin" "$task_appdir/usr/share/doc/try" "$task_tools" "$task_output/download"
cp local_artifacts/target/release/try "$task_appdir/usr/bin/try"
chmod +x "$task_appdir/usr/bin/try"
task_version=$(local_artifacts/target/release/try --distribution-info | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')
cat > "$task_output/try.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Try
Comment=A little practice, now and then.
Exec=try
Icon=try
Terminal=false
Categories=Education;
DESKTOP
cp assets/logo.svg "$task_output/try.svg"

download_verified() {
    curl --fail --location --retry 3 "$1" --output "$2"
    printf '%s  %s\n' "$3" "$2" | sha256sum --check --strict
    chmod +x "$2"
}
# Continuous release URLs are guarded by exact digests: upstream replacement fails closed.
download_verified 'https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage' "$task_tools/linuxdeploy-x86_64.AppImage" '8aea8da0f7f7039d2a2cecb14657d752a222a5e1d3825caeef186c82f751cdd1'
download_verified 'https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage' "$task_tools/linuxdeploy-plugin-appimage-x86_64.AppImage" '49d6a17160675a6bd1781699aae6bdf7692d98552e02a3671d2183d10547842e'
download_verified 'https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64' "$task_tools/runtime-x86_64" '156f4bdbde9c52d01814600013e0a273f0118dc2de98975f3c8c63427ec79074'
curl --fail --location --retry 3 'https://raw.githubusercontent.com/AppImage/type2-runtime/8f39b89e2ac31e1640b3d3f7e9a5108e6ce805fa/LICENSE' --output "$task_appdir/usr/share/doc/try/appimage_runtime_license"
python3 scripts/prepare_appimage_runtime.py --source-dir "$task_output/runtime_sources" --notice-dir "$task_appdir/usr/share/doc/try/appimage_runtime"
mkdir "$task_output/sources_download"
task_sources="$task_output/sources_download/try_linux_x64_runtime_sources_${task_version}.tar.gz"
tar -czf "$task_sources" -C "$task_output" runtime_sources
(cd "$task_output/sources_download" && sha256sum "$(basename "$task_sources")" > "$(basename "$task_sources").sha256")

export APPIMAGE_EXTRACT_AND_RUN=1
export LDAI_RUNTIME_FILE="$task_tools/runtime-x86_64"
export LINUXDEPLOY_OUTPUT_VERSION="$task_version"
export LDAI_OUTPUT="$task_output/download/try_linux_x64_${task_version}.AppImage"
export LDAI_NO_APPSTREAM=1
# Winit loads these X11 libraries dynamically, so ldd cannot discover them.
task_extra=()
for task_name in libXcursor.so.1 libXrandr.so.2 libXi.so.6 libxkbcommon-x11.so.0 libwayland-client.so.0; do
    task_library=$(ldconfig -p | awk -v name="$task_name" '$1 == name && /x86-64/ {print $NF}')
    test -n "$task_library"
    task_extra+=(--library "$task_library")
done
"$task_tools/linuxdeploy-x86_64.AppImage" --appdir "$task_appdir" --executable "$task_appdir/usr/bin/try" --desktop-file "$task_output/try.desktop" --icon-file "$task_output/try.svg" "${task_extra[@]}"
python3 scripts/collect_linux_notices.py "$task_appdir"
"$task_tools/linuxdeploy-x86_64.AppImage" --appdir "$task_appdir" --output appimage
python3 scripts/check_unix_release.py --output "$task_output/check.json" -- "$LDAI_OUTPUT" --appimage-extract-and-run
dbus-run-session -- xvfb-run -a python3 scripts/check_unix_release.py --smoke --output "$task_output/startup_check.json" -- "$LDAI_OUTPUT" --appimage-extract-and-run
dbus-run-session -- xvfb-run -a python3 scripts/check_in_app_update.py --executable "$LDAI_OUTPUT" --helper "$task_appdir/usr/bin/try" --output "$task_output/update_install_check.json"
(cd "$task_output/download" && sha256sum "$(basename "$LDAI_OUTPUT")" > "$(basename "$LDAI_OUTPUT").sha256")
echo "Packaged: $LDAI_OUTPUT"
