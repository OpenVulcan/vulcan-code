#!/bin/sh
# Downloads the Vulcan Code setup program @@VERSION@@ for this computer, checks its SHA-256,
# and runs it; the setup program then verifies everything it installs with the release signing key.
# 下载适合本机的 Vulcan Code 安装器 @@VERSION@@，校验其 SHA-256 后运行；安装器再用发布签名公钥校验
# 它安装的全部内容。
#
#   curl -fsSL https://static.openvulcan.com/vulcancode/install.sh | sh
#   curl -fsSL https://static.openvulcan.com/vulcancode/install.sh | sh -s -- --no-gui --yes
#
# Every argument goes to vulcan-code-setup unchanged; `--help` lists them. This file is a template:
# scripts/entry-scripts.mjs fills in the version, the origins and the digests when a setup version is
# published.
# 每个参数原样交给 vulcan-code-setup；`--help` 列出全部参数。本文件是模板：发布安装器版本时由
# scripts/entry-scripts.mjs 填入版本、下载源与摘要。

set -eu

Version='@@VERSION@@'
Origins='@@ORIGINS@@'

fail() {
    printf 'vulcan-code install: %s\n' "$*" >&2
    exit 1
}

case "$(uname -s)" in
    Linux) Os=linux ;;
    Darwin) Os=macos ;;
    *) fail "unsupported operating system $(uname -s); on Windows use install.ps1" ;;
esac
Machine=$(uname -m)
case "$Machine" in
    x86_64|amd64) Arch=x64 ;;
    aarch64|arm64) Arch=arm64 ;;
    *) fail "unsupported architecture $Machine; Vulcan Code is built for x64 and arm64" ;;
esac
# A Rosetta shell reports x86_64 on Apple silicon; the native arm64 build is the right one there.
# Rosetta 下的 shell 在 Apple 芯片上报告 x86_64；此时应安装原生 arm64 版本。
if [ "$Os" = macos ] && [ "$Arch" = x64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
    Arch=arm64
fi
Platform="$Os-$Arch"
case "$Platform" in
    linux-x64) Digest='@@SHA256_LINUX_X64@@' ;;
    linux-arm64) Digest='@@SHA256_LINUX_ARM64@@' ;;
    macos-x64) Digest='@@SHA256_MACOS_X64@@' ;;
    macos-arm64) Digest='@@SHA256_MACOS_ARM64@@' ;;
esac
File="vulcan-code-setup-$Version-$Platform"

# A terminal shows a progress bar. An origin slower than 1 KB/s for 30 seconds fails, so the next one is tried
# instead of waiting without end.
# 终端上显示进度条。下载源连续 30 秒低于 1 KB/s 即失败，改试下一个，而不是无限等待。
if command -v curl > /dev/null 2>&1; then
    if [ -t 2 ]; then Progress=--progress-bar; else Progress=--silent; fi
    fetch() { curl -fL "$Progress" -S --connect-timeout 10 --speed-limit 1024 --speed-time 30 -o "$2" "$1"; }
elif command -v wget > /dev/null 2>&1; then
    fetch() { wget -q -T 30 -O "$2" "$1"; }
else
    fail "curl or wget is needed to download the setup program"
fi
if command -v sha256sum > /dev/null 2>&1; then
    sha256_of() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum > /dev/null 2>&1; then
    sha256_of() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
    fail "sha256sum or shasum is needed to check the setup program"
fi

Work=$(mktemp -d "${TMPDIR:-/tmp}/vulcan-code-setup.XXXXXX") || fail "cannot create a temporary directory"
trap 'rm -rf "$Work"' EXIT
trap 'exit 130' INT TERM
Program="$Work/vulcan-code-setup"

# The origins are tried in order; the mirror serves the same verified objects.
# 按顺序尝试各下载源；镜像提供相同的已校验对象。
Fetched=0
for Origin in $Origins; do
    printf 'Downloading the Vulcan Code setup program %s (%s) from %s\n' "$Version" "$Platform" "$Origin" >&2
    if fetch "$Origin/setup/$Version/$File" "$Program"; then
        Fetched=1
        break
    fi
    printf 'vulcan-code install: cannot download from %s, trying the next origin\n' "$Origin" >&2
done
[ "$Fetched" = 1 ] || fail "cannot download $File"
Actual=$(sha256_of "$Program")
[ "$Actual" = "$Digest" ] || fail "$File does not match its published SHA-256 (expected $Digest, got $Actual)"
chmod 755 "$Program"
printf 'Starting the setup program %s\n' "$Version" >&2

# Under `curl | sh` standard input is this script, so the setup program reads the terminal instead;
# without a terminal it needs --yes.
# 在 `curl | sh` 下标准输入是本脚本，因此安装器改为读取终端；没有终端时需要 --yes。
Status=0
if [ -t 0 ]; then
    "$Program" "$@" || Status=$?
elif (exec < /dev/tty) 2> /dev/null; then
    "$Program" "$@" < /dev/tty || Status=$?
else
    "$Program" "$@" < /dev/null || Status=$?
fi
exit "$Status"
