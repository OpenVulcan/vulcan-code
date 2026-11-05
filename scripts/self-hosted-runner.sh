#!/bin/sh
# Install or remove the GitHub Actions runner of one self-hosted pool on a macOS or Linux machine
# (docs/contracts/self-hosted-runners.md). Run it as the user the runner should run as, never as root.
# 在 macOS 或 Linux 机器上安装或移除一个自托管池的 GitHub Actions 运行器（docs/contracts/self-hosted-runners.md）。
# 以运行器应使用的用户身份运行，不得以 root 运行。
#
#   self-hosted-runner.sh install --repository <owner/name> --label <pool label> [--name <runner name>] [--dir <directory>] [--manual] < registration-token
#   self-hosted-runner.sh remove [--dir <directory>] < removal-token
#   self-hosted-runner.sh status [--dir <directory>]
#   self-hosted-runner.sh run [--dir <directory>]
#
# --manual installs no service: the runner is online only while `run` (its run.sh) runs in a terminal, and offline
# otherwise, so CI and release use the hosted images. The directory defaults to ~/actions-runner/vulcan-code.
# --manual 不安装服务：运行器只在终端中运行 `run`（即其 run.sh）期间在线，其余时间离线，CI 与发布随之使用托管
# 镜像。目录默认为 ~/actions-runner/vulcan-code。
#
# The token comes on standard input so it never appears in a typed command or a log; `node scripts/ci-runners.mjs
# register` fetches it with the maintainer's gh and runs this script over SSH. The runner and PowerShell (for
# `make prune`) come from their latest GitHub releases and are checked against the SHA-256 GitHub publishes.
# 令牌经标准输入传入，因此不会出现在输入的命令或日志中；`node scripts/ci-runners.mjs register` 用维护者的 gh 获取令牌
# 并通过 SSH 运行本脚本。运行器与 PowerShell（供 `make prune` 使用）取自其最新 GitHub 发布，并按 GitHub 公布的
# SHA-256 校验。
set -eu

RunnerDir="$HOME/actions-runner/vulcan-code"
Opt="$HOME/.local/opt"
Bin="$HOME/.local/bin"

fail() {
    printf 'self-hosted-runner: %s\n' "$*" >&2
    exit 1
}

[ "$(id -u)" != 0 ] || fail "run this as the runner's own user, not as root"

case "$(uname -s)-$(uname -m)" in
    Darwin-x86_64) RunnerAsset=osx-x64; PwshAsset=osx-x64 ;;
    Darwin-arm64) RunnerAsset=osx-arm64; PwshAsset=osx-arm64 ;;
    Linux-x86_64) RunnerAsset=linux-x64; PwshAsset=linux-x64 ;;
    Linux-aarch64) RunnerAsset=linux-arm64; PwshAsset=linux-arm64 ;;
    *) fail "unsupported machine $(uname -s) $(uname -m)" ;;
esac

if command -v sha256sum > /dev/null 2>&1; then
    sha256_of() { sha256sum "$1" | cut -d ' ' -f 1; }
else
    sha256_of() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
fi

# latest_asset prints "<url> <sha256>" of the asset of the latest release of `repo` whose name matches the
# extended regular expression `pattern`.
# latest_asset 输出 `repo` 最新发布中名称匹配扩展正则 `pattern` 的资源的 "<地址> <sha256>"。
latest_asset() {
    curl -fsSL -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/$1/releases/latest" |
        awk -v pattern="$2" '
            /"name":/ { name = $0; sub(/.*"name": *"/, "", name); sub(/".*/, "", name); digest = "" }
            /"digest":/ { digest = $0; sub(/.*"sha256:/, "", digest); sub(/".*/, "", digest) }
            /"browser_download_url":/ && name ~ pattern {
                url = $0; sub(/.*"browser_download_url": *"/, "", url); sub(/".*/, "", url)
                print url, digest; exit
            }'
}

# fetch_verified downloads the asset matching `pattern` of `repo` to `file` and checks its SHA-256.
# fetch_verified 将 `repo` 中匹配 `pattern` 的资源下载到 `file` 并校验其 SHA-256。
fetch_verified() {
    set -- "$1" "$2" "$3" $(latest_asset "$1" "$2")
    [ -n "${4:-}" ] && [ -n "${5:-}" ] || fail "no release asset of $1 matches $2"
    curl -fsSL --retry 2 -o "$3" "$4"
    [ "$(sha256_of "$3")" = "$5" ] || fail "$4 does not match its published SHA-256"
}

install_pwsh() {
    if command -v pwsh > /dev/null 2>&1; then return; fi
    mkdir -p "$Opt/pwsh" "$Bin"
    fetch_verified PowerShell/PowerShell "^powershell-[0-9.]+-$PwshAsset\\.tar\\.gz\$" "$Opt/pwsh.tar.gz"
    tar -xzf "$Opt/pwsh.tar.gz" -C "$Opt/pwsh" && rm -f "$Opt/pwsh.tar.gz"
    chmod +x "$Opt/pwsh/pwsh"
    ln -sfn "$Opt/pwsh/pwsh" "$Bin/pwsh"
}

# installed_service reports whether svc.sh installed a service for this runner (it records it in .service).
# installed_service 判断 svc.sh 是否为本运行器安装了服务（记录在 .service 中）。
installed_service() {
    [ -e "$RunnerDir/.service" ]
}

# service runs the runner's own service script: svc.sh installs a LaunchAgent on macOS and needs sudo for a
# systemd unit on Linux.
# service 运行运行器自带的服务脚本：svc.sh 在 macOS 上安装 LaunchAgent，在 Linux 上安装 systemd 单元需要 sudo。
service() {
    if [ "$(uname -s)" = Darwin ]; then (cd "$RunnerDir" && ./svc.sh "$@"); else (cd "$RunnerDir" && sudo ./svc.sh "$@"); fi
}

command=${1:-}
[ $# = 0 ] || shift
Repository= Label= Name=$(hostname -s) Manual=false
while [ $# -gt 0 ]; do
    case "$1" in
        --repository) Repository=$2; shift 2 ;;
        --label) Label=$2; shift 2 ;;
        --name) Name=$2; shift 2 ;;
        --dir) RunnerDir=$2; shift 2 ;;
        --manual) Manual=true; shift ;;
        *) fail "unknown argument $1" ;;
    esac
done
case "$command" in
    install)
        [ -n "$Repository" ] && [ -n "$Label" ] || fail "--repository and --label are required"
        IFS= read -r Token || fail "the registration token is expected on standard input"
        [ ! -e "$RunnerDir/.runner" ] || fail "a runner is already configured in $RunnerDir; remove it first"
        # config.sh records PATH in .path; jobs find rustup, pwsh and user tools through it.
        # config.sh 将 PATH 记录到 .path；作业据此找到 rustup、pwsh 与用户工具。
        PATH="$HOME/.cargo/bin:$Bin:$PATH"
        export PATH
        install_pwsh
        mkdir -p "$RunnerDir"
        fetch_verified actions/runner "^actions-runner-$RunnerAsset-[0-9.]+\\.tar\\.gz\$" "$RunnerDir/runner.tar.gz"
        tar -xzf "$RunnerDir/runner.tar.gz" -C "$RunnerDir" && rm -f "$RunnerDir/runner.tar.gz"
        (cd "$RunnerDir" && ./config.sh --unattended --url "https://github.com/$Repository" --token "$Token" \
            --name "$Name" --labels "$Label" --work _work --replace)
        if [ "$Manual" = true ]; then
            echo "Runner $Name ($Label) is installed in $RunnerDir; start it with: $RunnerDir/run.sh"
        else
            service install
            service start
            echo "Runner $Name ($Label) is installed in $RunnerDir and started."
        fi
        ;;
    remove)
        IFS= read -r Token || fail "the removal token is expected on standard input"
        [ -e "$RunnerDir/.runner" ] || fail "no runner is configured in $RunnerDir"
        if installed_service; then
            service stop || true
            service uninstall || true
        fi
        (cd "$RunnerDir" && ./config.sh remove --token "$Token")
        echo "Runner removed; $RunnerDir keeps its work directory until you delete it."
        ;;
    status)
        [ -e "$RunnerDir/.runner" ] || fail "no runner is configured in $RunnerDir"
        if installed_service; then service status; else echo "Runner in $RunnerDir has no service; it runs while run.sh does."; fi
        ;;
    run)
        [ -e "$RunnerDir/.runner" ] || fail "no runner is configured in $RunnerDir"
        exec "$RunnerDir/run.sh"
        ;;
    *)
        fail "usage: self-hosted-runner.sh install --repository <owner/name> --label <label> [--name <name>] [--dir <dir>] [--manual] | remove | status | run"
        ;;
esac
