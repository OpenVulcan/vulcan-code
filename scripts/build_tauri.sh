#!/usr/bin/env sh
# build_tauri.sh builds the renamed service host and its WebUI assets.
# build_tauri.sh 用于构建重命名后的服务宿主及其 WebUI 产物.
# This legacy entry name is retained as a thin forwarder; the service no longer
# embeds a workbench window.

set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# Release stores whether this build should target Cargo release output.
# Release 保存本次构建是否面向 Cargo release 产物。
Release=0

if [ "${1:-}" = "--release" ] || [ "${1:-}" = "release" ]; then
    Release=1
elif [ "${1:-}" != "" ]; then
    echo "Unsupported build_tauri.sh argument: $1" >&2
    exit 1
fi

# ProfileName selects the distribution this build owns, so the other profile's root is never touched.
# ProfileName 选择本次构建拥有的发行目录，因此绝不触碰另一个模式的根目录。
ProfileName=debug
if [ "$Release" = "1" ]; then
    ProfileName=release
fi

# OutputRootDir is the same validated distribution root used by sync and packaging.
# OutputRootDir 是同步与打包共同使用的已验证发行根目录。
OutputRootDir=$(node "$ProjectDir/scripts/desktop-runtime.mjs" root "$ProfileName")

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
WebAppDir="$ProjectDir/apps/vulcan-code-web"

# IsWindowsPlatform records whether the current shell host is Windows-like.
# IsWindowsPlatform 记录当前 shell 宿主是否类似 Windows。
IsWindowsPlatform=0

case "$(uname -s 2>/dev/null || printf unknown)" in
    MINGW*|MSYS*|CYGWIN*)
        IsWindowsPlatform=1
        ;;
esac

cd "$ProjectDir"

# Use the persistent cache for both package lifecycle scripts and the explicit Electron installer.
# 为包生命周期脚本与显式 Electron 安装器共同使用持久缓存。
electron_config_cache=$(node "$ProjectDir/scripts/build-input-paths.mjs" electron)
export electron_config_cache

# Validate release dependency locks even when node_modules already exists.
# 即使 node_modules 已存在，也要验证发布依赖锁。
if [ "$Release" = "1" ] || [ ! -d "$WebAppDir/node_modules" ]; then
    pnpm --dir "$WebAppDir" install --frozen-lockfile
fi
if [ "$Release" = "1" ] || [ ! -d "$ProjectDir/apps/vulcan-code-desktop/node_modules" ]; then
    pnpm --dir "$ProjectDir/apps/vulcan-code-desktop" install --frozen-lockfile
fi

# The official installer verifies executable/version markers even when node_modules already exists.
# 即使 node_modules 已存在，官方安装器仍须核验可执行文件和版本标记。
# Missing runtime downloads or extraction failures must stop before expensive Cargo compilation.
# 运行时下载或解压失败必须在耗时的 Cargo 编译前终止。
node "$ProjectDir/scripts/install-electron.mjs"

# CargoPackageArguments derives the shipped executables from the same inventory used by Windows and notices.
# CargoPackageArguments 从 Windows 与声明生成共用的清单派生分发可执行文件。
CargoPackageArguments=$(node "$ProjectDir/scripts/shipped-rust-packages.mjs" cargo-args binary)
# CargoLibraryArguments selects the shipped libraries, built in their own invocation so they never take the service's features.
# CargoLibraryArguments 选择分发库，在独立调用中构建，绝不沾染服务的特性。
CargoLibraryArguments=$(node "$ProjectDir/scripts/shipped-rust-packages.mjs" cargo-args library)
# PrebuiltKinds lists the invocations whose release files the release workflow already restored into the
# Cargo profile directory from a published release (release-reuse.mjs); they are not built again.
# PrebuiltKinds 列出发布工作流已从已发布版本还原到 Cargo 配置目录的调用（release-reuse.mjs），不再重新构建。
PrebuiltKinds=$(node "$ProjectDir/scripts/shipped-rust-packages.mjs" prebuilt-kinds)

# cargo_release_build runs the release invocation of one kind unless its files were restored.
# cargo_release_build 执行某一类的 release 调用，除非其文件已被还原。
cargo_release_build() {
    Kind=$1
    shift
    case " $PrebuiltKinds " in
        *" $Kind "*)
            echo "==> Using the $Kind files restored from a published release"
            return 0
            ;;
    esac
    cargo build --locked "$@" --release
}

if [ "$Release" = "1" ]; then

    echo "==> Building WebUI for release..."
    pnpm --dir "$WebAppDir" run build

    echo "==> Building Vulcan Core release..."
    # Split only the fixed -p selectors and Cargo package names emitted by the inventory.
    # 仅拆分清单输出的固定 -p 选择项和 Cargo 包名。
    cargo_release_build binary $CargoPackageArguments
    cargo_release_build library $CargoLibraryArguments
else
    if [ -n "$PrebuiltKinds" ]; then
        echo "Restored release files cannot stand in for a debug build." >&2
        exit 1
    fi
    echo "==> Building Vulcan Core debug..."
    cargo build --locked $CargoPackageArguments
    cargo build --locked $CargoLibraryArguments
fi

# CargoTargetDir stores Cargo's effective artifact root, including environment and config overrides.
# CargoTargetDir 保存 Cargo 实际使用的制品根目录，包括环境变量和配置覆盖。
CargoTargetDir=$(cargo metadata --locked --format-version 1 --no-deps | node -e "const fs=require('fs'); const metadata=JSON.parse(fs.readFileSync(0, 'utf8')); process.stdout.write(metadata.target_directory || '');")
if [ -z "$CargoTargetDir" ]; then
    echo "Cargo metadata did not return target_directory." >&2
    exit 1
fi

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
BinaryFileName=vulcan-code-service
if [ "$IsWindowsPlatform" = "1" ]; then
    BinaryFileName=vulcan-code-service.exe
fi

# BuiltBinaryPath stores the desktop shell under Cargo's effective artifact root.
# BuiltBinaryPath 保存 Cargo 实际制品根目录下的桌面壳可执行文件。
BuiltBinaryPath="$CargoTargetDir/$ProfileName/$BinaryFileName"

# TerminalHostFileName stores the platform-specific isolated terminal-host binary name.
# TerminalHostFileName 保存平台相关的隔离终端宿主二进制名称。
TerminalHostFileName=vulcan-terminal-host
if [ "$IsWindowsPlatform" = "1" ]; then
    TerminalHostFileName=vulcan-terminal-host.exe
fi

# BuiltTerminalHostPath stores the terminal host built for the shell's Cargo profile.
# BuiltTerminalHostPath 保存与桌面壳 Cargo profile 一致的终端宿主制品路径。
BuiltTerminalHostPath="$CargoTargetDir/$ProfileName/$TerminalHostFileName"

# BuiltJudgeHostPath and BuiltTuiPath are required siblings of Core, never copied from an older output.
# BuiltJudgeHostPath 与 BuiltTuiPath 是 Core 必需的同级制品，绝不从旧发行物取用。
# ExecutableSuffix follows the detected host executable convention.
# ExecutableSuffix 遵循已检测宿主的可执行文件命名约定。
ExecutableSuffix=
if [ "$IsWindowsPlatform" = "1" ]; then
    ExecutableSuffix=.exe
fi
BuiltJudgeHostPath="$CargoTargetDir/$ProfileName/vulcan-judge-host$ExecutableSuffix"
BuiltTuiPath="$CargoTargetDir/$ProfileName/vulcan-tui$ExecutableSuffix"
# BuiltLanceLibraryPath stores the native LanceDB library the service loads from its own directory.
# BuiltLanceLibraryPath 保存服务从自身目录加载的原生 LanceDB 库。
BuiltLanceLibraryPath="$CargoTargetDir/$ProfileName/$(node "$ProjectDir/scripts/shipped-rust-packages.mjs" file-name vulcan-lancedb-native)"

# OutputBinDir stores runtime executables beneath the selected distribution root.
# OutputBinDir 保存选定发行根目录下的运行时可执行文件。
OutputBinDir="$OutputRootDir/bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
OutputBinaryPath="$OutputBinDir/$BinaryFileName"

# OutputTerminalHostPath stores the isolated terminal host beside the shell binary.
# OutputTerminalHostPath 保存复制到桌面壳旁边的隔离终端宿主路径。
OutputTerminalHostPath="$OutputBinDir/$TerminalHostFileName"

if [ ! -f "$BuiltBinaryPath" ]; then
    echo "Built Core binary was not found: $BuiltBinaryPath" >&2
    exit 1
fi
if [ ! -f "$BuiltTerminalHostPath" ]; then
    echo "Built terminal host was not found: $BuiltTerminalHostPath" >&2
    exit 1
fi
# RequiredBinaryPath checks the current build before packaging can observe any stale output files.
# RequiredBinaryPath 在打包读取任何旧发行文件之前，检查本轮构建制品。
for RequiredBinaryPath in "$BuiltJudgeHostPath" "$BuiltTuiPath" "$BuiltLanceLibraryPath"; do
    if [ ! -f "$RequiredBinaryPath" ]; then
        echo "Built distribution binary was not found: $RequiredBinaryPath" >&2
        exit 1
    fi
done

# Check the actual Core exports before publishing a macOS distribution.
# 发布 macOS 发行目录前校验实际 Core 的导出。
if [ "$(uname -s)" = Darwin ]; then
    pwsh -NoProfile -File "$ScriptDir/macos-lua-runtime.ps1" \
        -RuntimeRoot "$OutputRootDir/lua_runtime" -CoreExecutable "$BuiltBinaryPath"
fi

# Debug distributions also serve a complete static WebUI through Core.
# 调试发行物同样通过 Core 提供完整静态网页。
if [ "$Release" = "0" ]; then
    pnpm --dir "$ProjectDir/apps/vulcan-code-web" run build
fi
# Stage and validate a complete generation before switching the published directory.
# 切换正式目录之前，暂存并验证完整一代。
node "$ProjectDir/scripts/publish-distribution.mjs" "$ProfileName" "$BuiltBinaryPath" "$BuiltTerminalHostPath" "$BuiltJudgeHostPath" "$BuiltTuiPath" "$BuiltLanceLibraryPath"
