#!/usr/bin/env sh
# run_tauri.sh runs the Tauri/WebUI main application.
# run_tauri.sh 用于运行 Tauri/WebUI 主应用。

set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable child command paths.
# ProjectDir 保存仓库根目录，确保子命令路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# Release stores whether this run should launch the release output.
# Release 保存本次运行是否启动 release 产物。
Release=0

if [ "${1:-}" = "--release" ] || [ "${1:-}" = "release" ]; then
    Release=1
elif [ "${1:-}" != "" ]; then
    echo "Unsupported run_tauri.sh argument: $1" >&2
    exit 1
fi

# IsWindowsPlatform records whether the current shell host is Windows-like.
# IsWindowsPlatform 记录当前 shell 宿主是否类似 Windows。
IsWindowsPlatform=0

case "$(uname -s 2>/dev/null || printf unknown)" in
    MINGW*|MSYS*|CYGWIN*)
        IsWindowsPlatform=1
        ;;
esac

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
WebAppDir="$ProjectDir/apps/vulcan-code-web"

# BinaryFileName stores the platform-specific Tauri shell binary name.
# BinaryFileName 保存平台相关的 Tauri 桌面壳二进制名称。
BinaryFileName=vulcan-code-tauri
if [ "$IsWindowsPlatform" = "1" ]; then
    BinaryFileName=vulcan-code-tauri.exe
fi

# OutputBinDir stores the stable runtime binary output directory produced by make build.
# OutputBinDir 保存 make build 产出的稳定运行时二进制输出目录。
OutputBinDir="$ProjectDir/output/bin"

# OutputBinaryPath stores the exact executable that this script launches.
# OutputBinaryPath 保存本脚本启动的确切可执行文件路径。
OutputBinaryPath="$OutputBinDir/$BinaryFileName"

if [ ! -f "$OutputBinaryPath" ]; then
    # BuildCommand tells the user which build mode should produce the missing runtime executable.
    # BuildCommand 告知用户应使用哪种构建模式产出缺失的运行时可执行文件。
    BuildCommand="./make.sh build"
    if [ "$Release" = "1" ]; then
        BuildCommand="./make.sh build release"
    fi
    echo "Missing built Tauri binary: $OutputBinaryPath. Run $BuildCommand first." >&2
    exit 1
fi

if [ "$Release" = "1" ]; then
    echo "==> Running Vulcan Code Tauri release from output/bin..."
    "$OutputBinaryPath"
    exit $?
fi

# PnpmCommand resolves the pnpm executable used for the debug frontend dev server.
# PnpmCommand 解析 debug 前端开发服务器使用的 pnpm 可执行文件。
PnpmCommand=$(command -v pnpm || command -v pnpm.cmd || true)
if [ "$PnpmCommand" = "" ]; then
    echo "pnpm not found. Install pnpm before running the Tauri WebUI." >&2
    exit 1
fi

if [ ! -d "$WebAppDir/node_modules" ]; then
    echo "==> Installing WebUI dependencies..."
    "$PnpmCommand" --dir "$WebAppDir" install
fi

# ViteProcessId stores the background frontend dev server process id.
# ViteProcessId 保存前端开发服务器后台进程标识。
ViteProcessId=

# RunExitCode defaults to failure until the output binary exits with a concrete status.
# RunExitCode 在 output 二进制返回明确状态前默认表示失败。
RunExitCode=1

# cleanup_vite stops the background frontend dev server created by this script.
# cleanup_vite 停止本脚本创建的前端开发服务器后台进程。
cleanup_vite() {
    if [ "$ViteProcessId" != "" ] && kill -0 "$ViteProcessId" 2>/dev/null; then
        kill "$ViteProcessId" 2>/dev/null || true
        wait "$ViteProcessId" 2>/dev/null || true
    fi
}

# wait_for_vite waits until the debug WebUI port accepts TCP connections.
# wait_for_vite 等待 debug WebUI 端口开始接受 TCP 连接。
wait_for_vite() {
    # AttemptIndex stores the bounded retry counter for port probing.
    # AttemptIndex 保存端口探测使用的有界重试计数器。
    AttemptIndex=0
    while [ "$AttemptIndex" -lt 20 ]; do
        if node -e "const net=require('net');const s=net.createConnection(17601,'127.0.0.1');s.setTimeout(250);s.on('connect',()=>{s.destroy();process.exit(0)});s.on('timeout',()=>{s.destroy();process.exit(1)});s.on('error',()=>process.exit(1));" >/dev/null 2>&1; then
            return 0
        fi
        AttemptIndex=$((AttemptIndex + 1))
        sleep 1
    done
    return 1
}

trap cleanup_vite EXIT INT TERM

cd "$ProjectDir"

echo "==> Starting WebUI dev server at http://127.0.0.1:17601 ..."
"$PnpmCommand" --dir "$WebAppDir" run dev --host 127.0.0.1 --port 17601 --strictPort >/dev/null 2>&1 &
ViteProcessId=$!

if ! wait_for_vite; then
    echo "WebUI dev server did not become ready at http://127.0.0.1:17601" >&2
    exit 1
fi

echo "==> Running Vulcan Code Tauri debug from output/bin..."
"$OutputBinaryPath" || RunExitCode=$?

exit "$RunExitCode"
