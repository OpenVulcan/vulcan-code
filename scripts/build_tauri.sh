#!/usr/bin/env sh
# build_tauri.sh builds the Tauri/WebUI main application.
# build_tauri.sh 用于构建 Tauri/WebUI 主应用。

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

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
WebAppDir="$ProjectDir/apps/vulcan-code-web"

# SourceConfigDir points at the repository-owned packaged configuration templates.
# SourceConfigDir 指向仓库持有的打包配置模板目录。
SourceConfigDir="$ProjectDir/config"

# SourceResourcesDir points at the repository-owned runtime resource catalog.
# SourceResourcesDir 指向仓库持有的运行时资源目录。
SourceResourcesDir="$ProjectDir/crates/vulcan-runtime-assets/resources"

# SourceSystemPromptSkillsDir points at repository-owned system Agent Skills.
# SourceSystemPromptSkillsDir 指向仓库持有的系统 Agent Skills 目录。
SourceSystemPromptSkillsDir="$ProjectDir/crates/vulcan-prompt-skills/src/assets/system"

# SourceLuaRuntimeDir points at the official LuaSkills runtime payload installed by make deps-lua.
# SourceLuaRuntimeDir 指向 make deps-lua 安装的官方 LuaSkills 运行时载荷。
SourceLuaRuntimeDir="$ProjectDir/third_party/luaskills_runtime"

# SourceSystemPluginsDir contains every manifest-driven System Plugin bundled with Vulcan Code.
# SourceSystemPluginsDir 包含随 Vulcan Code 打包的所有清单驱动系统插件。
SourceSystemPluginsDir="$ProjectDir/plugins"

# SourcePluginBridgeDir contains the only LuaSkills System package used by Vulcan plugins.
# SourcePluginBridgeDir 包含 Vulcan 插件使用的唯一 LuaSkills System package。
SourcePluginBridgeDir="$ProjectDir/plugin-bridge/vulcan-plugin-bridge"

# IsWindowsPlatform records whether the current shell host is Windows-like.
# IsWindowsPlatform 记录当前 shell 宿主是否类似 Windows。
IsWindowsPlatform=0

case "$(uname -s 2>/dev/null || printf unknown)" in
    MINGW*|MSYS*|CYGWIN*)
        IsWindowsPlatform=1
        ;;
esac

# copy_directory_contents overlays one source directory onto one output directory.
# copy_directory_contents 用于把一个源目录覆盖同步到一个输出目录。
copy_directory_contents() {
    # SourceDirectory is the directory whose immediate children should be copied.
    # SourceDirectory 表示需要复制其直接子项的源目录。
    SourceDirectory=$1
    # DestinationDirectory is the output directory that receives the copied children.
    # DestinationDirectory 表示接收复制子项的输出目录。
    DestinationDirectory=$2
    # Clean removes the existing destination before copying.
    # Clean 表示复制前先删除已有目标目录。
    Clean=${3:-0}

    if [ ! -d "$SourceDirectory" ]; then
        echo "Missing source directory: $SourceDirectory" >&2
        exit 1
    fi

    if [ "$Clean" = "1" ] && [ -e "$DestinationDirectory" ]; then
        case "$DestinationDirectory" in
            "$ProjectDir"/output/*)
                rm -rf "$DestinationDirectory"
                ;;
            *)
                echo "Refusing to clean path outside output: $DestinationDirectory" >&2
                exit 1
                ;;
        esac
    fi

    mkdir -p "$DestinationDirectory"
    cp -R "$SourceDirectory"/. "$DestinationDirectory"/
}

cd "$ProjectDir"

if [ "$Release" = "1" ]; then
    if [ ! -d "$WebAppDir/node_modules" ]; then
        echo "==> Installing WebUI dependencies..."
        pnpm --dir "$WebAppDir" install
    fi

    echo "==> Building WebUI for release..."
    pnpm --dir "$WebAppDir" run build

    echo "==> Building Tauri shell release..."
    if [ "$IsWindowsPlatform" = "1" ]; then
        cargo build -p vulcan-code-tauri -p vulcan-windows-console-helper --release --features custom-protocol
    else
        cargo build -p vulcan-code-tauri --release --features custom-protocol
    fi
else
    echo "==> Building Tauri shell debug..."
    if [ "$IsWindowsPlatform" = "1" ]; then
        cargo build -p vulcan-code-tauri -p vulcan-windows-console-helper
    else
        cargo build -p vulcan-code-tauri
    fi
fi

# ProfileName selects the Cargo profile directory that contains the built shell.
# ProfileName 选择包含已构建桌面壳的 Cargo profile 目录。
ProfileName=debug
if [ "$Release" = "1" ]; then
    ProfileName=release
fi

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
BinaryFileName=vulcan-code-tauri
if [ "$IsWindowsPlatform" = "1" ]; then
    BinaryFileName=vulcan-code-tauri.exe
fi

# BuiltBinaryPath stores the Cargo-built desktop shell executable.
# BuiltBinaryPath 保存 Cargo 构建出的桌面壳可执行文件。
BuiltBinaryPath="$ProjectDir/target/$ProfileName/$BinaryFileName"

# HelperFileName stores the platform-specific Windows console helper binary name.
# HelperFileName 保存平台相关的 Windows 控制台 helper 二进制名称。
HelperFileName=vulcan-windows-console-helper
if [ "$IsWindowsPlatform" = "1" ]; then
    HelperFileName=vulcan-windows-console-helper.exe
fi

# BuiltHelperPath stores the helper executable built for the same Cargo profile as the shell.
# BuiltHelperPath 保存与桌面壳相同 Cargo profile 构建出的 helper 可执行文件。
BuiltHelperPath="$ProjectDir/target/$ProfileName/$HelperFileName"

# OutputBinDir stores the stable runtime binary output directory used by make run.
# OutputBinDir 保存 make run 使用的稳定运行时二进制输出目录。
OutputBinDir="$ProjectDir/output/bin"

# OutputBinaryPath stores the copied executable path that users and run scripts should launch.
# OutputBinaryPath 保存用户与运行脚本应启动的已复制可执行文件路径。
OutputBinaryPath="$OutputBinDir/$BinaryFileName"

# OutputHelperPath stores the copied Windows console helper beside the shell binary.
# OutputHelperPath 保存复制到桌面壳旁边的 Windows 控制台 helper。
OutputHelperPath="$OutputBinDir/$HelperFileName"

# OutputConfigDir stores the packaged configuration read by output/bin runtime launches.
# OutputConfigDir 保存 output/bin 运行时启动读取的打包配置目录。
OutputConfigDir="$ProjectDir/output/config"

# OutputResourcesDir stores the packaged resource catalog read by output/bin runtime launches.
# OutputResourcesDir 保存 output/bin 运行时启动读取的打包资源目录。
OutputResourcesDir="$ProjectDir/output/resources"

# OutputSystemPromptSkillsDir stores packaged system Agent Skills read directly by the runtime.
# OutputSystemPromptSkillsDir 保存运行时直接读取的打包系统 Agent Skills。
OutputSystemPromptSkillsDir="$OutputResourcesDir/prompt-skills/system"

# OutputToolsDir stores packaged auxiliary host tool executables.
# OutputToolsDir 保存打包后的辅助宿主工具可执行文件。
OutputToolsDir="$ProjectDir/output/tools"

# LegacyOutputRtkToolDir stores the previous nested RTK output directory for cleanup.
# LegacyOutputRtkToolDir 保存旧版嵌套 RTK 输出目录，用于清理。
LegacyOutputRtkToolDir="$OutputToolsDir/rtk"

# OutputLuaRuntimeDir stores the application-owned LuaSkills runtime root.
# OutputLuaRuntimeDir 保存应用持有的 LuaSkills 运行时根目录。
OutputLuaRuntimeDir="$ProjectDir/output/lua_runtime"

# LegacyManagedRuntimeDistributionDir is the obsolete LuaSkills-private Python/Node location.
# LegacyManagedRuntimeDistributionDir 是已经废弃的 LuaSkills 私有 Python/Node 位置。
LegacyManagedRuntimeDistributionDir="$OutputLuaRuntimeDir/dependencies/runtimes"

# LegacyManagedRuntimeScriptsDir stores obsolete downloader scripts removed by native Rust bootstrap.
# LegacyManagedRuntimeScriptsDir 保存由 Rust 原生初始化取代的废弃下载脚本。
LegacyManagedRuntimeScriptsDir="$OutputLuaRuntimeDir/scripts/deps"

# OutputSystemPluginsDir is the Vulcan-owned runtime directory scanned by the plugin host.
# OutputSystemPluginsDir 是插件宿主扫描的 Vulcan 自有运行时目录。
OutputSystemPluginsDir="$ProjectDir/output/plugins"

# OutputPluginBridgeDir is the only plugin-related package deployed below system_lua_lib.
# OutputPluginBridgeDir 是部署到 system_lua_lib 下的唯一插件相关 package。
OutputPluginBridgeDir="$OutputLuaRuntimeDir/system_lua_lib/vulcan-plugin-bridge"

# ManagedSystemPluginIndexPath records only packages owned by this build script for safe stale cleanup.
# ManagedSystemPluginIndexPath 仅记录当前构建脚本拥有的插件包，用于安全清理陈旧项。
ManagedSystemPluginIndexPath="$OutputSystemPluginsDir/.vulcan-code-managed-plugins"

if [ ! -f "$BuiltBinaryPath" ]; then
    echo "Built Tauri binary was not found: $BuiltBinaryPath" >&2
    exit 1
fi
if [ "$IsWindowsPlatform" = "1" ] && [ ! -f "$BuiltHelperPath" ]; then
    echo "Built Windows console helper was not found: $BuiltHelperPath" >&2
    exit 1
fi

echo "==> Syncing packaged config..."
copy_directory_contents "$SourceConfigDir" "$OutputConfigDir" 0

echo "==> Syncing packaged resources..."
copy_directory_contents "$SourceResourcesDir" "$OutputResourcesDir" 0

echo "==> Syncing packaged system Agent Skills..."
copy_directory_contents "$SourceSystemPromptSkillsDir" "$OutputSystemPromptSkillsDir" 1

echo "==> Syncing LuaSkills runtime payload..."
copy_directory_contents "$SourceLuaRuntimeDir" "$OutputLuaRuntimeDir" 0

if [ -d "$LegacyManagedRuntimeDistributionDir" ]; then
    echo "==> Removing obsolete LuaSkills-private managed runtimes..."
    rm -rf -- "$LegacyManagedRuntimeDistributionDir"
fi

if [ -d "$LegacyManagedRuntimeScriptsDir" ]; then
    echo "==> Removing obsolete managed runtime downloader scripts..."
    rm -rf -- "$LegacyManagedRuntimeScriptsDir"
fi

echo "==> Syncing host System Plugin packages..."
if [ ! -d "$SourceSystemPluginsDir" ]; then
    echo "Missing System Plugin source directory: $SourceSystemPluginsDir" >&2
    exit 1
fi
# FoundSystemPlugin records whether the repository contains at least one deployable manifest package.
# FoundSystemPlugin 记录仓库是否至少包含一个可部署的清单插件包。
FoundSystemPlugin=0
for SourceSystemPluginDir in "$SourceSystemPluginsDir"/*; do
    [ -d "$SourceSystemPluginDir" ] || continue
    [ -f "$SourceSystemPluginDir/plugin.yaml" ] || continue
    FoundSystemPlugin=1
    break
done
if [ "$FoundSystemPlugin" -ne 1 ]; then
    echo "No manifest-driven System Plugin packages were found in: $SourceSystemPluginsDir" >&2
    exit 1
fi
mkdir -p "$OutputSystemPluginsDir"
if [ -f "$ManagedSystemPluginIndexPath" ]; then
    while IFS= read -r PluginName || [ -n "$PluginName" ]; do
        [ -n "$PluginName" ] || continue
        case "$PluginName" in
            [a-z]* ) ;;
            * )
                echo "Invalid managed System Plugin index entry: $PluginName" >&2
                exit 1
                ;;
        esac
        case "$PluginName" in
            *[!a-z0-9_-]* )
                echo "Invalid managed System Plugin index entry: $PluginName" >&2
                exit 1
                ;;
        esac
        DeployedPluginDir="$OutputSystemPluginsDir/$PluginName"
        if [ -f "$SourceSystemPluginsDir/$PluginName/plugin.yaml" ]; then
            continue
        fi
        case "$DeployedPluginDir" in
            "$OutputSystemPluginsDir"/*)
                rm -rf "$DeployedPluginDir"
                ;;
            *)
                echo "Refusing to clean System Plugin path outside runtime root: $DeployedPluginDir" >&2
                exit 1
                ;;
        esac
    done < "$ManagedSystemPluginIndexPath"
fi
: > "$ManagedSystemPluginIndexPath"
for SourceSystemPluginDir in "$SourceSystemPluginsDir"/*; do
    [ -d "$SourceSystemPluginDir" ] || continue
    [ -f "$SourceSystemPluginDir/plugin.yaml" ] || continue
    PluginName="$(basename "$SourceSystemPluginDir")"
    case "$PluginName" in
        [a-z]* ) ;;
        * )
            echo "Invalid System Plugin source directory name: $PluginName" >&2
            exit 1
            ;;
    esac
    case "$PluginName" in
        *[!a-z0-9_-]* )
            echo "Invalid System Plugin source directory name: $PluginName" >&2
            exit 1
            ;;
    esac
    # DestinationSystemPluginDir receives an incremental overlay so debug builds preserve existing plugin files.
    # DestinationSystemPluginDir 接收增量覆盖，确保调试构建保留已有插件文件。
    DestinationSystemPluginDir="$OutputSystemPluginsDir/$PluginName"
    if ! copy_directory_contents "$SourceSystemPluginDir" "$DestinationSystemPluginDir" 0; then
        echo "Failed to deploy System Plugin package: $PluginName" >&2
        exit 1
    fi
    printf '%s\n' "$PluginName" >> "$ManagedSystemPluginIndexPath"
done

echo "==> Syncing fixed LuaSkills plugin bridge package..."
copy_directory_contents "$SourcePluginBridgeDir" "$OutputPluginBridgeDir" 0

# LegacyOutputRtkToolDir is removed because RTK is now distributed only inside its plugin package.
# LegacyOutputRtkToolDir 会被移除，因为 RTK 现在只通过其插件包分发。
if [ -e "$LegacyOutputRtkToolDir" ]; then
    case "$LegacyOutputRtkToolDir" in
        "$ProjectDir"/output/*)
            rm -rf "$LegacyOutputRtkToolDir"
            ;;
        *)
            echo "Refusing to clean path outside output: $LegacyOutputRtkToolDir" >&2
            exit 1
            ;;
    esac
fi
find "$OutputToolsDir" -maxdepth 1 -type f \( -name rtk -o -name rtk.exe \) -delete 2>/dev/null || true
# RtkHostIdentity selects the exact target artifact already copied with the plugin package.
# RtkHostIdentity 选择随插件包一并复制的当前宿主精确目标制品。
RtkHostIdentity="$(uname -s 2>/dev/null || printf unknown):$(uname -m 2>/dev/null || printf unknown)"
case "$RtkHostIdentity" in
    MINGW*:*|MSYS*:*|CYGWIN*:*) RtkTargetTriple="x86_64-pc-windows-msvc"; RtkBinaryName="rtk.exe" ;;
    Darwin:arm64|Darwin:aarch64) RtkTargetTriple="aarch64-apple-darwin"; RtkBinaryName="rtk" ;;
    Darwin:*) RtkTargetTriple="x86_64-apple-darwin"; RtkBinaryName="rtk" ;;
    Linux:aarch64|Linux:arm64) RtkTargetTriple="aarch64-unknown-linux-gnu"; RtkBinaryName="rtk" ;;
    Linux:*) RtkTargetTriple="x86_64-unknown-linux-gnu"; RtkBinaryName="rtk" ;;
    *)
        echo "Unsupported RTK plugin packaging target." >&2
        exit 1
        ;;
esac
# OutputRtkPluginBinary is the exact manifest-owned executable required for this package.
# OutputRtkPluginBinary 是当前插件包要求的精确清单自有可执行文件。
OutputRtkPluginBinary="$OutputSystemPluginsDir/vulcan-rtk/bin/$RtkTargetTriple/$RtkBinaryName"
if [ ! -f "$OutputRtkPluginBinary" ]; then
    echo "RTK plugin binary is missing; run make deps-rtk before packaging." >&2
    exit 1
fi

# Copy the executable only after runtime payload synchronization so a locked debug binary cannot suppress plugin updates.
# 仅在运行时载荷同步完成后复制可执行文件，避免被占用的调试二进制阻断插件更新。
mkdir -p "$OutputBinDir"
cp "$BuiltBinaryPath" "$OutputBinaryPath"
if [ "$IsWindowsPlatform" = "1" ]; then
    cp "$BuiltHelperPath" "$OutputHelperPath"
fi

echo "==> Done. Binary: $OutputBinaryPath"
if [ "$IsWindowsPlatform" = "1" ]; then
    echo "==> Done. Windows console helper: $OutputHelperPath"
fi
