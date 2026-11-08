#!/bin/sh
# Install or remove the GitHub Actions runner of one self-hosted pool on a macOS or Linux machine
# (docs/contracts/self-hosted-runners.md). Run it as the user the runner should run as, never as root.
# 在 macOS 或 Linux 机器上安装或移除一个自托管池的 GitHub Actions 运行器（docs/contracts/self-hosted-runners.md）。
# 以运行器应使用的用户身份运行，不得以 root 运行。
#
#   self-hosted-runner.sh install --repository <owner/name> --label <pool label> [--name <runner name>] [--dir <directory>] [--manual] [--node <version> --rust <version>] < registration-token
#   self-hosted-runner.sh remove [--dir <directory>] < removal-token
#   self-hosted-runner.sh status [--dir <directory>]
#   self-hosted-runner.sh run [--dir <directory>]
#
# --manual installs no service: the runner is online only while `run` (its start.sh) runs in a terminal, and offline
# otherwise, so CI and release use the hosted images. The directory defaults to ~/actions-runner/vulcan-code.
# --manual 不安装服务：运行器只在终端中运行 `run`（即其 start.sh）期间在线，其余时间离线，CI 与发布随之使用托管
# 镜像。目录默认为 ~/actions-runner/vulcan-code。
#
# The macos-x64 label on an Apple silicon Mac installs a Rosetta runner, because the build scripts produce only the
# platform they run on and do not cross-compile: the x64 runner, an x86_64 Rust toolchain (--rust, in ~/.cargo-x64
# and ~/.rustup-x64) and an x86_64 Node (--node, linked in ~/.local/x64/bin) are installed, and every job runs
# under Rosetta. Its directory defaults to ~/actions-runner/vulcan-code-x64, beside a native macos-arm64 runner;
# remove it with --dir.
# 在 Apple 芯片 Mac 上使用 macos-x64 标签时安装 Rosetta 运行器，因为构建脚本只产出其所在平台、不做交叉编译：安装
# x64 运行器、x86_64 Rust 工具链（--rust，位于 ~/.cargo-x64 与 ~/.rustup-x64）与 x86_64 Node（--node，链接到
# ~/.local/x64/bin），每个作业都在 Rosetta 下运行。其目录默认为 ~/actions-runner/vulcan-code-x64，与原生
# macos-arm64 运行器并存；移除时用 --dir 指定。
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
# The x86_64 toolchains of a Rosetta runner, apart from the native ones of the same user.
# Rosetta 运行器的 x86_64 工具链，与同一用户的原生工具链分开。
X64CargoHome="$HOME/.cargo-x64"
X64RustupHome="$HOME/.rustup-x64"
X64Bin="$HOME/.local/x64/bin"

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

# install_pwsh installs PowerShell unless the runner already finds one. A Rosetta runner always gets its own x86_64
# copy first on its PATH: scripts read the platform from PowerShell's process architecture, and a native arm64 pwsh
# runs as arm64 even when a Rosetta job starts it.
# install_pwsh 在运行器找不到 PowerShell 时安装它。Rosetta 运行器总是使用排在其 PATH 最前的独立 x86_64 副本：脚本按
# PowerShell 进程架构判断平台，而原生 arm64 的 pwsh 即使由 Rosetta 作业启动也以 arm64 运行。
install_pwsh() {
    if [ "$Rosetta" = true ]; then
        PwshDir="$Opt/pwsh-x64" PwshLink="$X64Bin/pwsh"
        if [ -x "$PwshLink" ]; then return; fi
    else
        PwshDir="$Opt/pwsh" PwshLink="$Bin/pwsh"
        if command -v pwsh > /dev/null 2>&1; then return; fi
    fi
    mkdir -p "$PwshDir" "$(dirname "$PwshLink")"
    fetch_verified PowerShell/PowerShell "^powershell-[0-9.]+-$PwshAsset\\.tar\\.gz\$" "$PwshDir.tar.gz"
    tar -xzf "$PwshDir.tar.gz" -C "$PwshDir" && rm -f "$PwshDir.tar.gz"
    chmod +x "$PwshDir/pwsh"
    ln -sfn "$PwshDir/pwsh" "$PwshLink"
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

# write_start writes start.sh, which a manual runner is started with. run.sh keeps the PATH of the terminal that
# starts it, where ~/.local/bin and ~/.cargo/bin may be missing; only the service loads .path. start.sh loads .path
# first, so a manual runner gives its jobs the same PATH as a service.
# write_start 写出手动运行器的启动脚本 start.sh。run.sh 沿用启动它的终端的 PATH，其中可能没有 ~/.local/bin 与
# ~/.cargo/bin；只有服务会载入 .path。start.sh 先载入 .path，使手动运行器的作业与服务得到相同的 PATH。
write_start() {
    # A Rosetta runner starts under Rosetta, so its jobs and every universal program they run are x86_64.
    # Rosetta 运行器在 Rosetta 下启动，使其作业及作业运行的每个通用程序都是 x86_64。
    if [ -e "$RunnerDir/.rosetta" ]; then Exec='arch -x86_64 ./run.sh'; else Exec='./run.sh'; fi
    cat > "$RunnerDir/start.sh" <<EOF
#!/bin/sh
# Start this runner with the PATH config.sh recorded in .path, as its service would (self-hosted-runner.sh).
cd "\$(dirname "\$0")" || exit 1
PATH=\$(cat .path)
export PATH
exec $Exec "\$@"
EOF
    chmod +x "$RunnerDir/start.sh"
}

# install_rosetta_toolchains installs what a Rosetta runner's jobs run before their own setup steps: an x86_64
# rustup with the repository toolchain as its default, and an x86_64 Node checked against the official SHASUMS256.txt.
# Both are kept when already present, so registering again downloads nothing.
# install_rosetta_toolchains 安装 Rosetta 运行器的作业在其自身准备步骤之前要用的内容：以仓库工具链为默认的
# x86_64 rustup，以及按官方 SHASUMS256.txt 校验的 x86_64 Node。已存在时保留，因此重新注册不会再次下载。
install_rosetta_toolchains() {
    [ -n "$NodeVersion" ] && [ -n "$RustVersion" ] || fail "a Rosetta runner needs --node and --rust"
    arch -x86_64 /usr/bin/true 2> /dev/null ||
        fail "Rosetta is not installed; run: sudo softwareupdate --install-rosetta --agree-to-license"
    if [ ! -x "$X64CargoHome/bin/rustup" ]; then
        Init="$(mktemp -d)/rustup-init"
        Url=https://static.rust-lang.org/rustup/dist/x86_64-apple-darwin/rustup-init
        curl -fsSL --retry 2 -o "$Init" "$Url"
        [ "$(sha256_of "$Init")" = "$(curl -fsSL "$Url.sha256" | cut -d ' ' -f 1)" ] || fail "$Url does not match its SHA-256"
        chmod +x "$Init"
        arch -x86_64 env CARGO_HOME="$X64CargoHome" RUSTUP_HOME="$X64RustupHome" "$Init" -y --no-modify-path \
            --profile minimal --default-toolchain "$RustVersion" --default-host x86_64-apple-darwin
        rm -rf "$(dirname "$Init")"
    else
        arch -x86_64 env CARGO_HOME="$X64CargoHome" RUSTUP_HOME="$X64RustupHome" "$X64CargoHome/bin/rustup" \
            default "$RustVersion"
    fi
    NodeName="node-v$NodeVersion-darwin-x64"
    if [ ! -x "$Opt/$NodeName/bin/node" ]; then
        mkdir -p "$Opt"
        curl -fsSL --retry 2 -o "$Opt/$NodeName.tar.xz" "https://nodejs.org/dist/v$NodeVersion/$NodeName.tar.xz"
        Expected=$(curl -fsSL "https://nodejs.org/dist/v$NodeVersion/SHASUMS256.txt" | awk -v f="$NodeName.tar.xz" '$2 == f { print $1 }')
        [ -n "$Expected" ] && [ "$(sha256_of "$Opt/$NodeName.tar.xz")" = "$Expected" ] || fail "$NodeName.tar.xz does not match SHASUMS256.txt"
        tar -xf "$Opt/$NodeName.tar.xz" -C "$Opt" && rm -f "$Opt/$NodeName.tar.xz"
    fi
    mkdir -p "$X64Bin"
    ln -sfn "$Opt/$NodeName/bin/node" "$X64Bin/node"
}

command=${1:-}
[ $# = 0 ] || shift
Repository= Label= Name=$(hostname -s) Manual=false NodeVersion= RustVersion= DirGiven=false
while [ $# -gt 0 ]; do
    case "$1" in
        --repository) Repository=$2; shift 2 ;;
        --label) Label=$2; shift 2 ;;
        --name) Name=$2; shift 2 ;;
        --dir) RunnerDir=$2; DirGiven=true; shift 2 ;;
        --manual) Manual=true; shift ;;
        --node) NodeVersion=$2; shift 2 ;;
        --rust) RustVersion=$2; shift 2 ;;
        *) fail "unknown argument $1" ;;
    esac
done
# Rosetta is the macos-x64 pool on an Apple silicon Mac; see the header.
# Rosetta 即 Apple 芯片 Mac 上的 macos-x64 池；见文件开头。
Rosetta=false
if [ "$(uname -s)-$(uname -m)" = Darwin-arm64 ] && [ "$Label" = vulcan-macos-x64 ]; then
    Rosetta=true
    RunnerAsset=osx-x64
    PwshAsset=osx-x64
    [ "$DirGiven" = true ] || RunnerDir="$HOME/actions-runner/vulcan-code-x64"
fi
case "$command" in
    install)
        [ -n "$Repository" ] && [ -n "$Label" ] || fail "--repository and --label are required"
        IFS= read -r Token || fail "the registration token is expected on standard input"
        [ ! -e "$RunnerDir/.runner" ] || fail "a runner is already configured in $RunnerDir; remove it first"
        # config.sh records PATH in .path; jobs find rustup, pwsh and user tools through it. A Rosetta runner puts
        # its x86_64 toolchains first.
        # config.sh 将 PATH 记录到 .path；作业据此找到 rustup、pwsh 与用户工具。Rosetta 运行器把其 x86_64 工具链放在最前。
        PATH="$HOME/.cargo/bin:$Bin:$PATH"
        if [ "$Rosetta" = true ]; then
            install_rosetta_toolchains
            PATH="$X64CargoHome/bin:$X64Bin:$PATH"
        fi
        export PATH
        install_pwsh
        mkdir -p "$RunnerDir"
        fetch_verified actions/runner "^actions-runner-$RunnerAsset-[0-9.]+\\.tar\\.gz\$" "$RunnerDir/runner.tar.gz"
        tar -xzf "$RunnerDir/runner.tar.gz" -C "$RunnerDir" && rm -f "$RunnerDir/runner.tar.gz"
        (cd "$RunnerDir" && ./config.sh --unattended --url "https://github.com/$Repository" --token "$Token" \
            --name "$Name" --labels "$Label" --work _work --replace)
        if [ "$Rosetta" = true ]; then
            # Jobs read .env, so their cargo and rustup use the x86_64 homes; .rosetta makes start.sh use Rosetta.
            # 作业读取 .env，使其 cargo 与 rustup 使用 x86_64 目录；.rosetta 使 start.sh 经 Rosetta 启动。
            printf 'CARGO_HOME=%s\nRUSTUP_HOME=%s\n' "$X64CargoHome" "$X64RustupHome" >> "$RunnerDir/.env"
            : > "$RunnerDir/.rosetta"
        fi
        if [ "$Manual" = true ]; then
            write_start
            echo "Runner $Name ($Label) is installed in $RunnerDir; start it with: $RunnerDir/start.sh"
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
        [ -e "$RunnerDir/start.sh" ] || write_start
        exec "$RunnerDir/start.sh"
        ;;
    *)
        fail "usage: self-hosted-runner.sh install --repository <owner/name> --label <label> [--name <name>] [--dir <dir>] [--manual] [--node <version> --rust <version>] | remove | status | run"
        ;;
esac
