#!/bin/sh
# Packages native graphical setup binaries that already contain their console installers.
# 为已嵌入命令行安装器的原生图形安装器打包。
set -eu

# Keep the brand resource shared with the installer window.
# 与安装器窗口共用品牌资源。
project_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
logo="$project_dir/apps/vulcan-code-web/public/vulcan-logo-startup.png"

# Prints the platform-specific inputs and exits without producing a partial package.
# 输出各平台所需的参数后退出，避免生成不完整的安装包。
usage() {
    printf '%s\n' 'Usage: package-setup-gui.sh linux <gui-binary> <output.AppImage> <appimagetool> <type2-runtime>' \
        '       package-setup-gui.sh macos <arm64-gui-binary> <x64-gui-binary> <output.dmg>' >&2
    exit 2
}

[ "$#" -ge 1 ] || usage
platform=$1
shift
staging=$(mktemp -d)
trap 'rm -rf -- "$staging"' EXIT
trap 'exit 1' HUP INT TERM

case "$platform" in
    linux)
        [ "$#" -eq 4 ] || usage
        # Linux mounts an executable AppImage and launches the UI without a terminal.
        # Linux 挂载可执行的 AppImage，并在不打开终端的情况下启动界面。
        app="$staging/VulcanCodeSetup.AppDir"
        mkdir -p "$app/usr/bin"
        install -m 755 "$1" "$app/usr/bin/vulcan-code-setup-gui"
        install -m 644 "$logo" "$app/vulcan-code-setup.png"
        ln -s vulcan-code-setup.png "$app/.DirIcon"
        ln -s usr/bin/vulcan-code-setup-gui "$app/AppRun"
        cat > "$app/vulcan-code-setup.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Vulcan Code Setup
Icon=vulcan-code-setup
Exec=vulcan-code-setup-gui
Terminal=false
Categories=Development;
DESKTOP
        # The caller supplies a pinned runtime for the binary's architecture.
        # 调用方提供与二进制架构匹配且已锁定版本的运行时。
        APPIMAGE_EXTRACT_AND_RUN=1 "$3" --runtime-file "$4" "$app" "$2"
        chmod +x "$2"
        ;;
    macos)
        [ "$#" -eq 3 ] || usage
        # Each slice keeps the matching console installer when combined into one application.
        # 合并为单个应用时，每个架构片段都保留对应的命令行安装器。
        app="$staging/Vulcan Code Setup.app"
        contents="$app/Contents"
        executable="$contents/MacOS/vulcan-code-setup-gui"
        mkdir -p "$contents/MacOS" "$contents/Resources"
        lipo -verify_arch arm64 "$1"
        lipo -verify_arch x86_64 "$2"
        lipo -create "$1" "$2" -output "$executable"
        chmod 755 "$executable"
        # Finder and the Dock use the same existing artwork as the Iced window.
        # Finder 和程序坞使用与 Iced 窗口相同的现有图标。
        iconset="$staging/Setup.iconset"
        mkdir "$iconset"
        for size in 16 32 128 256 512; do
            sips -z "$size" "$size" "$logo" --out "$iconset/icon_${size}x${size}.png" >/dev/null
            scale=$((size * 2))
            sips -z "$scale" "$scale" "$logo" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
        done
        iconutil -c icns "$iconset" -o "$contents/Resources/Setup.icns"
        rm -r -- "$iconset"
        # Read the installer version from the native executable used to create this package.
        # 从本次打包的原生可执行文件读取安装器版本。
        version=$("$executable" --version)
        version=${version#vulcan-code-setup }
        case "$version" in ''|*[!0-9.]*) exit 1 ;; esac
        cat > "$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>Vulcan Code Setup</string>
    <key>CFBundleExecutable</key><string>vulcan-code-setup-gui</string>
    <key>CFBundleIconFile</key><string>Setup.icns</string>
    <key>CFBundleIdentifier</key><string>com.openvulcan.vulcancode.setup</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>Vulcan Code Setup</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
        plutil -lint "$contents/Info.plist"
        # An explicit identity produces a hardened app for notarization. Local test builds are ad hoc signed.
        # 指定签名身份时生成可送公证的强化应用；本地测试包使用临时签名。
        if [ -n "${VULCAN_SETUP_CODESIGN_IDENTITY:-}" ]; then
            codesign --force --options runtime --timestamp --sign "$VULCAN_SETUP_CODESIGN_IDENTITY" "$app"
        else
            codesign --force --sign - "$app"
        fi
        codesign --verify --strict "$app"
        hdiutil create -fs HFS+ -srcfolder "$staging" -volname 'Vulcan Code Setup' -format UDZO "$3"
        ;;
    *) usage ;;
esac
