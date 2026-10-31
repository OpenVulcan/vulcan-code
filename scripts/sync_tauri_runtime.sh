#!/usr/bin/env sh
# sync_tauri_runtime.sh synchronizes runtime resources into the stable output tree.
# sync_tauri_runtime.sh 用于把运行时资源同步到稳定的 output 目录树。

set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable source and output paths.
# ProjectDir 保存仓库根目录，确保源目录与输出目录路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# ProfileName selects the distribution this synchronization owns: debug by default, or release.
# ProfileName 选择本次同步拥有的发行目录：默认 debug，或 release。
ProfileName=debug
if [ "${1:-}" = "release" ]; then
    ProfileName=release
elif [ "${1:-}" != "" ]; then
    echo "Unsupported sync_tauri_runtime.sh argument: $1" >&2
    exit 1
fi

# OutputRootDir is validated before any synchronization or recursive cleanup.
# OutputRootDir 在任何同步或递归清理之前完成验证。
OutputRootDir=$(node "$ProjectDir/scripts/desktop-runtime.mjs" root "$ProfileName")

# Download locked browser dependencies only during explicit build synchronization.
# 仅在显式构建同步期间下载锁定浏览器依赖。
node "$ProjectDir/scripts/browser-runtime.mjs" prepare "$OutputRootDir"

# SourceConfigDir points at repository-owned packaged configuration templates.
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

# BundledToolsHostIdentity captures the operating system and architecture used to select one verified payload.
# BundledToolsHostIdentity 捕获用于选择一个已校验载荷的操作系统与架构。
BundledToolsHostIdentity="$(uname -s 2>/dev/null || printf unknown):$(uname -m 2>/dev/null || printf unknown)"

# BundledToolsTarget stores the canonical current-host target prepared by make deps tools.
# BundledToolsTarget 保存由 make deps tools 准备的规范当前宿主目标。
BundledToolsTarget=""

# BundledToolBinarySuffix stores the executable suffix required by the current host.
# BundledToolBinarySuffix 保存当前宿主所需的可执行文件后缀。
BundledToolBinarySuffix=""
case "$BundledToolsHostIdentity" in
    MINGW*:aarch64|MSYS*:aarch64|CYGWIN*:aarch64) BundledToolsTarget="aarch64-pc-windows-msvc"; BundledToolBinarySuffix=".exe" ;;
    MINGW*:*|MSYS*:*|CYGWIN*:*) BundledToolsTarget="x86_64-pc-windows-msvc"; BundledToolBinarySuffix=".exe" ;;
    Darwin:arm64|Darwin:aarch64) BundledToolsTarget="aarch64-apple-darwin" ;;
    Darwin:x86_64) BundledToolsTarget="x86_64-apple-darwin" ;;
    Linux:aarch64|Linux:arm64) BundledToolsTarget="aarch64-unknown-linux-gnu" ;;
    Linux:x86_64) BundledToolsTarget="x86_64-unknown-linux-gnu" ;;
    *)
        echo "Unsupported bundled-tools synchronization target: $BundledToolsHostIdentity" >&2
        exit 1
        ;;
esac

# SourceBundledToolsDir points at the verified current-target sg and rg payload.
# SourceBundledToolsDir 指向已校验的当前目标 sg 与 rg 载荷。
SourceBundledToolsDir="$ProjectDir/third_party/tools/$BundledToolsTarget"

# SourceVulcanPluginsDir contains every manifest-driven plugin bundled with Vulcan Code.
# SourceVulcanPluginsDir 包含随 Vulcan Code 打包的所有清单驱动 Vulcan 插件。
SourceVulcanPluginsDir="$ProjectDir/plugins"

# SourcePluginBridgeDir contains the only LuaSkills System package used by Vulcan plugins.
# SourcePluginBridgeDir 包含 Vulcan 插件使用的唯一 LuaSkills System package。
SourcePluginBridgeDir="$ProjectDir/plugin-bridge/vulcan-plugin-bridge"

# OutputConfigDir stores packaged configuration read by output/bin runtime launches.
# OutputConfigDir 保存 output/bin 运行时启动读取的打包配置目录。
OutputConfigDir="$OutputRootDir/config"

# OutputResourcesDir stores the packaged resource catalog read by output/bin runtime launches.
# OutputResourcesDir 保存 output/bin 运行时启动读取的打包资源目录。
OutputResourcesDir="$OutputRootDir/resources"

# OutputSystemPromptSkillsDir stores packaged system Agent Skills read directly by the runtime.
# OutputSystemPromptSkillsDir 保存运行时直接读取的打包系统 Agent Skills。
OutputSystemPromptSkillsDir="$OutputResourcesDir/prompt-skills/system"

# OutputLuaRuntimeDir stores the application-owned LuaSkills runtime root.
# OutputLuaRuntimeDir 保存应用持有的 LuaSkills 运行时根目录。
OutputLuaRuntimeDir="$OutputRootDir/lua_runtime"

# OutputBundledToolsDir is the runtime-owned sg, rg, and rtk distribution root.
# OutputBundledToolsDir 是运行时持有的 sg、rg 与 rtk 发行根目录。
OutputBundledToolsDir="$OutputRootDir/tools"

# LegacyManagedRuntimeDistributionDir is the obsolete LuaSkills-private Python/Node location.
# LegacyManagedRuntimeDistributionDir 是已经废弃的 LuaSkills 私有 Python/Node 位置。
LegacyManagedRuntimeDistributionDir="$OutputLuaRuntimeDir/dependencies/runtimes"

# LegacyManagedRuntimeScriptsDir stores obsolete downloader scripts removed by native Rust bootstrap.
# LegacyManagedRuntimeScriptsDir 保存由 Rust 原生初始化取代的废弃下载脚本。
LegacyManagedRuntimeScriptsDir="$OutputLuaRuntimeDir/scripts/deps"

# OutputVulcanPluginsDir is the Vulcan-owned runtime directory scanned by the plugin host.
# OutputVulcanPluginsDir 是插件宿主扫描的 Vulcan 自有运行时目录。
OutputVulcanPluginsDir="$OutputRootDir/plugins"

# OutputPluginBridgeDir is the only plugin-related package deployed below system_lua_lib.
# OutputPluginBridgeDir 是部署到 system_lua_lib 下的唯一插件相关 package。
OutputPluginBridgeDir="$OutputLuaRuntimeDir/system_lua_lib/vulcan-plugin-bridge"

# ManagedVulcanPluginIndexPath records only packages owned by this sync script for safe stale cleanup.
# ManagedVulcanPluginIndexPath 仅记录当前同步脚本拥有的插件包，用于安全清理陈旧项。
ManagedVulcanPluginIndexPath="$OutputVulcanPluginsDir/.vulcan-code-managed-plugins"

# remove_output_directory removes one exact directory only after proving it is below the output root.
# remove_output_directory 仅在证明精确目录位于 output 根目录下后执行删除。
# The function accepts one absolute target directory and returns no value.
# 此函数接收一个绝对目标目录，不返回值。
remove_output_directory() {
    # TargetDirectory is the exact output descendant considered for recursive removal.
    # TargetDirectory 是准备递归删除的精确 output 子目录。
    TargetDirectory=$1
    case "$TargetDirectory" in
        "$OutputRootDir"/*)
            rm -rf -- "$TargetDirectory"
            ;;
        *)
            echo "Refusing to clean path outside output: $TargetDirectory" >&2
            exit 1
            ;;
    esac
}

# copy_directory_contents overlays one source directory onto one output directory.
# copy_directory_contents 用于把一个源目录覆盖同步到一个输出目录。
# The function accepts source, destination, and an optional clean flag and returns no value.
# 此函数接收源目录、目标目录与可选清理标记，不返回值。
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
        remove_output_directory "$DestinationDirectory"
    fi

    mkdir -p "$DestinationDirectory"
    cp -R "$SourceDirectory"/. "$DestinationDirectory"/
}

cd "$ProjectDir"

echo "==> Syncing packaged config..."
copy_directory_contents "$SourceConfigDir" "$OutputConfigDir" 0

echo "==> Syncing packaged resources..."
copy_directory_contents "$SourceResourcesDir" "$OutputResourcesDir" 0

echo "==> Syncing packaged system Agent Skills..."
copy_directory_contents "$SourceSystemPromptSkillsDir" "$OutputSystemPromptSkillsDir" 1

echo "==> Syncing LuaSkills runtime payload..."
copy_directory_contents "$SourceLuaRuntimeDir" "$OutputLuaRuntimeDir" 0

# BundledToolNames are the tools the build-input lock declares for this host.
# BundledToolNames 是构建输入锁文件为当前宿主声明的工具。
BundledToolNames=$(node "$ProjectDir/scripts/install-locked-tools.mjs" bundled-tool-ids)
# BundledToolName iterates the exact commands required by the runtime PATH contract.
# BundledToolName 遍历运行时 PATH 契约要求的精确命令。
for BundledToolName in $BundledToolNames; do
    # SourceBundledToolBinary is the exact verified dependency required before synchronization.
    # SourceBundledToolBinary 是同步前必需的精确已校验依赖。
    SourceBundledToolBinary="$SourceBundledToolsDir/bin/$BundledToolName$BundledToolBinarySuffix"
    if [ ! -f "$SourceBundledToolBinary" ]; then
        echo "Bundled tool '$BundledToolName' is missing; run make deps tools before synchronization." >&2
        exit 1
    fi
done
echo "==> Syncing bundled tools ($BundledToolNames)..."
copy_directory_contents "$SourceBundledToolsDir" "$OutputBundledToolsDir" 1

# SourceJudgeRuntimeDir is the lock-verified judge runtime of this host, or empty when the lock declares none.
# SourceJudgeRuntimeDir 是按锁文件校验过的本宿主判定执行库；锁文件未声明时为空。
SourceJudgeRuntimeDir=$(node "$ProjectDir/scripts/install-locked-tools.mjs" verify-judge-runtime)
# OutputJudgeRuntimeDir is the runtime-owned ONNX Runtime and WebGPU plugin root.
# OutputJudgeRuntimeDir 是运行时持有的 ONNX Runtime 与 WebGPU 插件根目录。
OutputJudgeRuntimeDir="$OutputRootDir/judge-runtime"
if [ -n "$SourceJudgeRuntimeDir" ]; then
    echo "==> Syncing bundled judge runtime..."
    copy_directory_contents "$SourceJudgeRuntimeDir" "$OutputJudgeRuntimeDir" 1
elif [ -d "$OutputJudgeRuntimeDir" ]; then
    remove_output_directory "$OutputJudgeRuntimeDir"
fi

if [ -d "$LegacyManagedRuntimeDistributionDir" ]; then
    echo "==> Removing obsolete LuaSkills-private managed runtimes..."
    remove_output_directory "$LegacyManagedRuntimeDistributionDir"
fi

if [ -d "$LegacyManagedRuntimeScriptsDir" ]; then
    echo "==> Removing obsolete managed runtime downloader scripts..."
    remove_output_directory "$LegacyManagedRuntimeScriptsDir"
fi

echo "==> Syncing host Vulcan Plugin packages..."
if [ ! -d "$SourceVulcanPluginsDir" ]; then
    echo "Missing Vulcan Plugin source directory: $SourceVulcanPluginsDir" >&2
    exit 1
fi
# FoundVulcanPlugin records whether the repository contains at least one deployable manifest package.
# FoundVulcanPlugin 记录仓库是否至少包含一个可部署的清单插件包。
FoundVulcanPlugin=0
# SourceVulcanPluginDir iterates each candidate plugin package directory.
# SourceVulcanPluginDir 遍历每个候选插件包目录。
for SourceVulcanPluginDir in "$SourceVulcanPluginsDir"/*; do
    [ -d "$SourceVulcanPluginDir" ] || continue
    [ -f "$SourceVulcanPluginDir/plugin.yaml" ] || continue
    FoundVulcanPlugin=1
    break
done
if [ "$FoundVulcanPlugin" -ne 1 ]; then
    echo "No manifest-driven Vulcan Plugin packages were found in: $SourceVulcanPluginsDir" >&2
    exit 1
fi
mkdir -p "$OutputVulcanPluginsDir"
if [ -f "$ManagedVulcanPluginIndexPath" ]; then
    # CarriageReturn normalizes ownership indexes previously written by the Windows synchronization route.
    # CarriageReturn 规范化此前由 Windows 同步路由写入的所有权索引。
    CarriageReturn=$(printf '\r')
    # PluginName receives each previously managed plugin name from the ownership index.
    # PluginName 接收所有权索引中的每个既有受管插件名称。
    while IFS= read -r PluginName || [ -n "$PluginName" ]; do
        # PluginName drops one trailing CR while preserving the exact managed plugin identifier.
        # PluginName 去除一个尾随 CR，同时保留精确的受管插件标识。
        PluginName=${PluginName%"$CarriageReturn"}
        [ -n "$PluginName" ] || continue
        case "$PluginName" in
            [a-z]* ) ;;
            * )
                echo "Invalid managed Vulcan Plugin index entry: $PluginName" >&2
                exit 1
                ;;
        esac
        case "$PluginName" in
            *[!a-z0-9_-]* )
                echo "Invalid managed Vulcan Plugin index entry: $PluginName" >&2
                exit 1
                ;;
        esac
        # DeployedPluginDir is the exact stale plugin directory considered for removal.
        # DeployedPluginDir 是准备移除的精确陈旧插件目录。
        DeployedPluginDir="$OutputVulcanPluginsDir/$PluginName"
        if [ -f "$SourceVulcanPluginsDir/$PluginName/plugin.yaml" ]; then
            continue
        fi
        case "$DeployedPluginDir" in
            "$OutputVulcanPluginsDir"/*)
                remove_output_directory "$DeployedPluginDir"
                ;;
            *)
                echo "Refusing to clean Vulcan Plugin path outside runtime root: $DeployedPluginDir" >&2
                exit 1
                ;;
        esac
    done < "$ManagedVulcanPluginIndexPath"
fi
: > "$ManagedVulcanPluginIndexPath"
for SourceVulcanPluginDir in "$SourceVulcanPluginsDir"/*; do
    [ -d "$SourceVulcanPluginDir" ] || continue
    [ -f "$SourceVulcanPluginDir/plugin.yaml" ] || continue
    # PluginName is the validated runtime directory name derived from the source package.
    # PluginName 是从源插件包提取并验证的运行时目录名称。
    PluginName="$(basename "$SourceVulcanPluginDir")"
    case "$PluginName" in
        [a-z]* ) ;;
        * )
            echo "Invalid Vulcan Plugin source directory name: $PluginName" >&2
            exit 1
            ;;
    esac
    case "$PluginName" in
        *[!a-z0-9_-]* )
            echo "Invalid Vulcan Plugin source directory name: $PluginName" >&2
            exit 1
            ;;
    esac
    # DestinationVulcanPluginDir receives an incremental overlay so synchronization preserves unmanaged files.
    # DestinationVulcanPluginDir 接收增量覆盖，确保同步过程保留未管理文件。
    DestinationVulcanPluginDir="$OutputVulcanPluginsDir/$PluginName"
    # Replace the application-owned browser dependencies instead of retaining obsolete files.
    # 替换应用自有浏览器依赖，避免保留过期文件。
    if [ "$PluginName" = "vulcan-browser" ]; then
        rm -rf "$DestinationVulcanPluginDir/node/node_modules"
    fi
    if ! copy_directory_contents "$SourceVulcanPluginDir" "$DestinationVulcanPluginDir" 0; then
        echo "Failed to deploy Vulcan Plugin package: $PluginName" >&2
        exit 1
    fi
    printf '%s\n' "$PluginName" >> "$ManagedVulcanPluginIndexPath"
done

echo "==> Syncing fixed LuaSkills plugin bridge package..."
copy_directory_contents "$SourcePluginBridgeDir" "$OutputPluginBridgeDir" 0

echo "==> Done. Runtime resources synchronized into: $OutputRootDir"
