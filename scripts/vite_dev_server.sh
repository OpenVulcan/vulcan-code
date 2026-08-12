#!/usr/bin/env sh
# vite_dev_server.sh manages the standalone WebUI Vite development server.
# vite_dev_server.sh 用于管理独立的 WebUI Vite 开发服务器。

set -eu

# ScriptDir stores this script directory for stable repository-root resolution.
# ScriptDir 保存当前脚本目录，确保仓库根目录解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable project and runtime-state paths.
# ProjectDir 保存仓库根目录，确保工程与运行时状态路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# Action selects the exact lifecycle operation performed by this invocation.
# Action 用于选择本次调用执行的精确生命周期操作。
Action=${1:-}

# WebAppDir points at the Vite React WebUI project.
# WebAppDir 指向 Vite React WebUI 工程。
WebAppDir="$ProjectDir/apps/vulcan-code-web"

# ViteEntryPath points at the project-local Vite CLI started as the tracked process.
# ViteEntryPath 指向作为受跟踪进程启动的项目本地 Vite CLI。
ViteEntryPath="$WebAppDir/node_modules/vite/bin/vite.js"

# RuntimeStateDir stores ignored local lifecycle state and logs.
# RuntimeStateDir 保存被忽略的本地生命周期状态与日志。
RuntimeStateDir="$ProjectDir/.vulcan/temp"

# VitePidPath stores the managed Vite process id.
# VitePidPath 保存受管 Vite 进程标识。
VitePidPath="$RuntimeStateDir/vite-dev-server.pid"

# ViteStdoutPath stores detached Vite standard output.
# ViteStdoutPath 保存独立 Vite 进程的标准输出。
ViteStdoutPath="$RuntimeStateDir/vite-dev-server.stdout.log"

# ViteStderrPath stores detached Vite standard error.
# ViteStderrPath 保存独立 Vite 进程的标准错误。
ViteStderrPath="$RuntimeStateDir/vite-dev-server.stderr.log"

# ViteAddress stores the only loopback address accepted for the managed server.
# ViteAddress 保存受管服务器唯一允许使用的回环地址。
ViteAddress=127.0.0.1

# VitePort stores the fixed development-server port shared with the Tauri dev URL.
# VitePort 保存与 Tauri 开发 URL 共用的固定开发服务器端口。
VitePort=17601

# NodeCommand stores the Node.js executable used for endpoint probes and Vite execution.
# NodeCommand 保存用于端点探测与 Vite 执行的 Node.js 可执行文件。
NodeCommand=$(command -v node || true)

# ViteProcessId stores the validated pid read from state or returned by a new start.
# ViteProcessId 保存从状态读取或由新启动返回的已验证进程标识。
ViteProcessId=

# endpoint_is_ready checks whether the fixed Vite loopback endpoint accepts connections.
# endpoint_is_ready 用于检查固定 Vite 回环端点是否接受连接。
# The function accepts no parameters and returns success only for a reachable endpoint.
# 此函数不接收参数，仅在端点可连接时返回成功。
endpoint_is_ready() {
    [ "$NodeCommand" != "" ] || return 1
    "$NodeCommand" -e "const net=require('net');const socket=net.createConnection($VitePort,'$ViteAddress');socket.setTimeout(300);socket.on('connect',()=>{socket.destroy();process.exit(0)});socket.on('timeout',()=>{socket.destroy();process.exit(1)});socket.on('error',()=>process.exit(1));" >/dev/null 2>&1
}

# load_vite_pid reads and validates the generated pid file into ViteProcessId.
# load_vite_pid 用于读取并验证生成的进程标识文件到 ViteProcessId。
# The function accepts no parameters and returns one when state is absent and two when invalid.
# 此函数不接收参数；状态不存在时返回一，状态无效时返回二。
load_vite_pid() {
    if [ ! -f "$VitePidPath" ]; then
        ViteProcessId=
        return 1
    fi
    ViteProcessId=$(tr -d '[:space:]' < "$VitePidPath")
    case "$ViteProcessId" in
        ""|*[!0-9]*)
            echo "Invalid Vite lifecycle pid file: $VitePidPath" >&2
            return 2
            ;;
    esac
    if [ "$ViteProcessId" -le 0 ]; then
        echo "Invalid Vite lifecycle pid file: $VitePidPath" >&2
        return 2
    fi
    return 0
}

# process_is_tracked_vite verifies that one pid still executes this repository's exact Vite entrypoint.
# process_is_tracked_vite 用于验证一个进程标识仍在执行本仓库的精确 Vite 入口。
# ProcessId selects the process to inspect; the function returns success only for an exact command-line path match.
# ProcessId 用于选择待检查进程；此函数仅在命令行路径精确匹配时返回成功。
process_is_tracked_vite() {
    # ProcessId is the persisted operating-system process id under inspection.
    # ProcessId 是待检查的持久化操作系统进程标识。
    ProcessId=$1
    kill -0 "$ProcessId" 2>/dev/null || return 1
    # ProcessCommandLine is the current command line used to reject pid reuse.
    # ProcessCommandLine 是用于拒绝进程标识复用的当前命令行。
    ProcessCommandLine=$(ps -p "$ProcessId" -o command= 2>/dev/null || true)
    case "$ProcessCommandLine" in
        *"$ViteEntryPath"*) return 0 ;;
        *) return 1 ;;
    esac
}

# process_owns_endpoint verifies that the tracked process owns the ready Vite listener when native inspection is available.
# process_owns_endpoint 在本机检查工具可用时验证受跟踪进程持有已就绪的 Vite 监听器。
# ProcessId selects the verified process; the function returns success only for an owned ready endpoint.
# ProcessId 用于选择已验证进程；此函数仅在端点就绪且归属正确时返回成功。
process_owns_endpoint() {
    # ProcessId is the exact tracked process expected to own the listener.
    # ProcessId 是预期持有监听器的精确受跟踪进程。
    ProcessId=$1
    process_is_tracked_vite "$ProcessId" || return 1
    endpoint_is_ready || return 1
    if command -v lsof >/dev/null 2>&1; then
        lsof -nP -a -p "$ProcessId" -iTCP@"$ViteAddress:$VitePort" -sTCP:LISTEN >/dev/null 2>&1
        return $?
    fi
    if command -v ss >/dev/null 2>&1; then
        # ListenerTable is the bounded socket table used for pid ownership validation on Linux.
        # ListenerTable 是 Linux 上用于进程归属验证的有界套接字表。
        ListenerTable=$(ss -ltnp "sport = :$VitePort" 2>/dev/null || true)
        printf '%s\n' "$ListenerTable" | grep -F "pid=$ProcessId," >/dev/null 2>&1
        return $?
    fi
    return 0
}

# remove_vite_pid deletes only the generated Vite pid file when present.
# remove_vite_pid 仅在存在时删除生成的 Vite 进程标识文件。
# The function accepts no parameters and returns no value.
# 此函数不接收参数，也不返回值。
remove_vite_pid() {
    if [ -f "$VitePidPath" ]; then
        rm -f -- "$VitePidPath"
    fi
}

# wait_for_managed_endpoint waits a bounded interval for one tracked Vite listener.
# wait_for_managed_endpoint 用于有界等待一个受跟踪 Vite 监听器。
# ProcessId selects the started process; the function returns success when ownership becomes ready.
# ProcessId 用于选择已启动进程；监听归属就绪时此函数返回成功。
wait_for_managed_endpoint() {
    # ProcessId is the newly started Vite process being monitored.
    # ProcessId 是正在监控的新启动 Vite 进程。
    ProcessId=$1
    # AttemptIndex stores the bounded readiness retry count.
    # AttemptIndex 保存有界就绪重试次数。
    AttemptIndex=0
    while [ "$AttemptIndex" -lt 20 ]; do
        process_is_tracked_vite "$ProcessId" || return 1
        if process_owns_endpoint "$ProcessId"; then
            return 0
        fi
        AttemptIndex=$((AttemptIndex + 1))
        sleep 1
    done
    return 1
}

# start_vite starts one detached Vite process and persists its pid.
# start_vite 用于启动一个独立 Vite 进程并持久化其进程标识。
# The function accepts no parameters and returns success when the managed listener is ready.
# 此函数不接收参数；受管监听器就绪时返回成功。
start_vite() {
    # LoadResult preserves whether state is valid, absent, or invalid.
    # LoadResult 保留状态有效、缺失或无效的区别。
    LoadResult=0
    load_vite_pid || LoadResult=$?
    if [ "$LoadResult" -eq 0 ]; then
        if process_is_tracked_vite "$ViteProcessId"; then
            if process_owns_endpoint "$ViteProcessId"; then
                echo "Vite dev server is already running: pid=$ViteProcessId, url=http://$ViteAddress:$VitePort"
                return 0
            fi
            echo "Tracked Vite process $ViteProcessId exists but its listener is not ready. Run 'make vite stop' before starting again." >&2
            return 1
        fi
        remove_vite_pid
    elif [ "$LoadResult" -eq 2 ]; then
        return 1
    fi

    if endpoint_is_ready; then
        echo "Port $VitePort is already occupied by a process not managed by this repository." >&2
        return 1
    fi
    if [ "$NodeCommand" = "" ]; then
        echo "Node.js not found. Install Node.js before starting the Vite dev server." >&2
        return 1
    fi

    # PnpmCommand resolves the package manager used only when WebUI dependencies are missing.
    # PnpmCommand 解析仅在 WebUI 依赖缺失时使用的包管理器。
    PnpmCommand=$(command -v pnpm || command -v pnpm.cmd || true)
    if [ "$PnpmCommand" = "" ]; then
        echo "pnpm not found. Install pnpm before starting the Vite dev server." >&2
        return 1
    fi
    if [ ! -d "$WebAppDir/node_modules" ]; then
        echo "==> Installing WebUI dependencies..."
        "$PnpmCommand" --dir "$WebAppDir" install
    fi
    if [ ! -f "$ViteEntryPath" ]; then
        echo "Missing project-local Vite entrypoint: $ViteEntryPath" >&2
        return 1
    fi

    mkdir -p "$RuntimeStateDir"
    echo "==> Starting standalone WebUI dev server at http://$ViteAddress:$VitePort ..."
    nohup "$NodeCommand" "$ViteEntryPath" --host "$ViteAddress" --port "$VitePort" --strictPort >"$ViteStdoutPath" 2>"$ViteStderrPath" &
    ViteProcessId=$!
    # VitePidTempPath is the same-directory temporary file used for atomic pid publication.
    # VitePidTempPath 是用于原子发布进程标识的同目录临时文件。
    VitePidTempPath="$VitePidPath.tmp.$$"
    printf '%s\n' "$ViteProcessId" > "$VitePidTempPath"
    mv -f "$VitePidTempPath" "$VitePidPath"

    if ! wait_for_managed_endpoint "$ViteProcessId"; then
        if process_is_tracked_vite "$ViteProcessId"; then
            kill "$ViteProcessId" 2>/dev/null || true
        fi
        remove_vite_pid
        echo "Vite did not become ready at http://$ViteAddress:$VitePort. Inspect $ViteStderrPath" >&2
        return 1
    fi
    echo "Vite dev server started: pid=$ViteProcessId, url=http://$ViteAddress:$VitePort"
    return 0
}

# stop_vite stops only the exact Vite command selected by the generated pid file.
# stop_vite 仅停止生成的进程标识文件所选择的精确 Vite 命令。
# The function accepts no parameters and returns success when managed state is stopped or already absent.
# 此函数不接收参数；受管状态停止或已不存在时返回成功。
stop_vite() {
    # LoadResult preserves whether state is valid, absent, or invalid.
    # LoadResult 保留状态有效、缺失或无效的区别。
    LoadResult=0
    load_vite_pid || LoadResult=$?
    if [ "$LoadResult" -ne 0 ]; then
        if [ "$LoadResult" -eq 2 ]; then
            return 1
        fi
        if endpoint_is_ready; then
            echo "Port $VitePort is occupied, but no managed Vite state exists; refusing to stop an unrelated process." >&2
            return 1
        fi
        echo "Vite dev server is already stopped."
        return 0
    fi

    if ! process_is_tracked_vite "$ViteProcessId"; then
        remove_vite_pid
        if endpoint_is_ready; then
            echo "Managed Vite state was stale, but port $VitePort is occupied by another process; nothing was stopped." >&2
            return 1
        fi
        echo "Removed stale Vite state; the dev server was already stopped."
        return 0
    fi

    echo "==> Stopping standalone WebUI dev server: pid=$ViteProcessId ..."
    kill "$ViteProcessId"
    # AttemptIndex stores the bounded graceful-stop retry count.
    # AttemptIndex 保存有界优雅停止重试次数。
    AttemptIndex=0
    while process_is_tracked_vite "$ViteProcessId" && [ "$AttemptIndex" -lt 10 ]; do
        AttemptIndex=$((AttemptIndex + 1))
        sleep 1
    done
    if process_is_tracked_vite "$ViteProcessId"; then
        kill -9 "$ViteProcessId"
    fi
    remove_vite_pid
    if endpoint_is_ready; then
        echo "Managed Vite process stopped, but port $VitePort remains occupied by another process." >&2
        return 1
    fi
    echo "Vite dev server stopped."
    return 0
}

# show_vite_status reports running, stopped, unhealthy, stale, or unmanaged state without mutation.
# show_vite_status 用于无状态变更地报告运行、停止、异常、陈旧或非受管状态。
# The function accepts no parameters and returns success for running or stopped, failure for inconsistent state.
# 此函数不接收参数；运行或停止时返回成功，状态不一致时返回失败。
show_vite_status() {
    # LoadResult preserves whether state is valid, absent, or invalid.
    # LoadResult 保留状态有效、缺失或无效的区别。
    LoadResult=0
    load_vite_pid || LoadResult=$?
    if [ "$LoadResult" -ne 0 ]; then
        if [ "$LoadResult" -eq 2 ]; then
            return 1
        fi
        if endpoint_is_ready; then
            echo "Vite dev server status: unmanaged listener at http://$ViteAddress:$VitePort"
            return 1
        fi
        echo "Vite dev server status: stopped"
        return 0
    fi
    if ! process_is_tracked_vite "$ViteProcessId"; then
        echo "Vite dev server status: stale state, tracked process is absent"
        return 1
    fi
    if ! process_owns_endpoint "$ViteProcessId"; then
        echo "Vite dev server status: unhealthy, pid=$ViteProcessId, listener not ready"
        return 1
    fi
    echo "Vite dev server status: running, pid=$ViteProcessId, url=http://$ViteAddress:$VitePort"
    return 0
}

case "$Action" in
    start) start_vite ;;
    stop) stop_vite ;;
    status) show_vite_status ;;
    *)
        echo "Usage: $0 {start|stop|status}" >&2
        exit 1
        ;;
esac
