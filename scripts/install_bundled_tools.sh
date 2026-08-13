#!/usr/bin/env sh
# install_bundled_tools.sh downloads the latest supported command-line tools for the current host.
# install_bundled_tools.sh 为当前宿主下载最新的受支持命令行工具。

set -eu

# ScriptDir stores the absolute scripts directory for stable repository resolution.
# ScriptDir 保存绝对 scripts 目录，确保仓库解析稳定。
ScriptDir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)

# ProjectDir stores the repository root for stable dependency paths.
# ProjectDir 保存仓库根目录，确保依赖路径稳定。
ProjectDir=$(CDPATH= cd "$ScriptDir/.." && pwd -P)

# ToolsRoot stores target-specific downloaded tool payloads outside source control.
# ToolsRoot 保存目标平台专属的已下载工具载荷，且不纳入源码版本控制。
ToolsRoot="$ProjectDir/third_party/tools"

# AstGrepRepo identifies the authoritative ast-grep GitHub repository.
# AstGrepRepo 标识权威的 ast-grep GitHub 仓库。
AstGrepRepo="ast-grep/ast-grep"

# RipgrepRepo identifies the authoritative ripgrep GitHub repository.
# RipgrepRepo 标识权威的 ripgrep GitHub 仓库。
RipgrepRepo="BurntSushi/ripgrep"

# GitHubUserAgent provides a stable API user agent for release discovery and downloads.
# GitHubUserAgent 为发行版查询和下载提供稳定的 API User-Agent。
GitHubUserAgent="Vulcan-Code-Bundled-Tools"

# Selects one strict JSON parser available on the host.
# 选择宿主上可用的一个严格 JSON 解析器。
#
# Returns:
# 返回值：
# The command id jq or node; exits when neither parser is installed.
# 返回 jq 或 node 命令标识；两者均未安装时退出。
select_json_parser() {
    if command -v jq >/dev/null 2>&1; then
        printf '%s\n' "jq"
        return
    fi
    if command -v node >/dev/null 2>&1; then
        printf '%s\n' "node"
        return
    fi
    echo "jq or node is required to parse authoritative GitHub release metadata." >&2
    exit 127
}

# Downloads one exact URL with redirects and retry handling.
# 下载一个精确 URL，并处理重定向与重试。
#
# Parameters:
# 参数：
# The first argument is the source URL; the second is the destination file.
# 第一个参数是源 URL；第二个参数是目标文件。
download_file() (
    # DownloadUrl is the exact remote resource to retrieve.
    # DownloadUrl 是要获取的精确远程资源。
    DownloadUrl=$1
    # DownloadDestination is the exact local output file.
    # DownloadDestination 是精确的本地输出文件。
    DownloadDestination=$2

    if command -v curl >/dev/null 2>&1; then
        if [ -n "${GITHUB_TOKEN:-}" ]; then
            curl --fail --location --silent --show-error --retry 3 \
                --user-agent "$GitHubUserAgent" \
                --header "Authorization: Bearer $GITHUB_TOKEN" \
                --output "$DownloadDestination" \
                "$DownloadUrl"
        else
            curl --fail --location --silent --show-error --retry 3 \
                --user-agent "$GitHubUserAgent" \
                --output "$DownloadDestination" \
                "$DownloadUrl"
        fi
        exit
    fi
    if command -v wget >/dev/null 2>&1; then
        if [ -n "${GITHUB_TOKEN:-}" ]; then
            wget --quiet --tries=3 \
                --user-agent="$GitHubUserAgent" \
                --header="Authorization: Bearer $GITHUB_TOKEN" \
                --output-document="$DownloadDestination" \
                "$DownloadUrl"
        else
            wget --quiet --tries=3 \
                --user-agent="$GitHubUserAgent" \
                --output-document="$DownloadDestination" \
                "$DownloadUrl"
        fi
        exit
    fi
    echo "curl or wget is required to download bundled tools." >&2
    exit 127
)

# Resolves the canonical target directory shared by sg and rg payloads.
# 解析 sg 与 rg 载荷共用的规范目标目录。
#
# Returns:
# 返回值：
# The Rust-style target triple for the current host.
# 当前宿主对应的 Rust 风格目标三元组。
get_current_tool_target() {
    # HostSystem is the operating-system identity reported by uname.
    # HostSystem 是 uname 报告的操作系统标识。
    HostSystem=$(uname -s)
    # HostArchitecture is the processor architecture reported by uname.
    # HostArchitecture 是 uname 报告的处理器架构。
    HostArchitecture=$(uname -m)
    # ArchitectureKey is the normalized architecture selected for upstream release assets.
    # ArchitectureKey 是为上游发行制品选择的规范化架构。
    ArchitectureKey=""

    case "$HostArchitecture" in
        x86_64|amd64) ArchitectureKey="x86_64" ;;
        arm64|aarch64) ArchitectureKey="aarch64" ;;
        *)
            echo "Unsupported bundled-tool architecture: $HostArchitecture" >&2
            return 1
            ;;
    esac

    case "$HostSystem" in
        Darwin) printf '%s\n' "$ArchitectureKey-apple-darwin" ;;
        Linux) printf '%s\n' "$ArchitectureKey-unknown-linux-gnu" ;;
        MINGW*|MSYS*|CYGWIN*) printf '%s\n' "$ArchitectureKey-pc-windows-msvc" ;;
        *)
            echo "Unsupported bundled-tool operating system: $HostSystem" >&2
            return 1
            ;;
    esac
}

# Reads the non-empty Latest Release tag from one GitHub API response.
# 从一个 GitHub API 响应中读取非空 Latest Release 标签。
#
# Parameters:
# 参数：
# The first argument selects jq or node; the second is the release JSON file.
# 第一个参数选择 jq 或 node；第二个参数是发行版 JSON 文件。
read_release_tag() (
    # ParserId identifies the strict JSON parser selected at startup.
    # ParserId 标识启动时选择的严格 JSON 解析器。
    ParserId=$1
    # ReleaseJsonPath is the downloaded authoritative release response.
    # ReleaseJsonPath 是已下载的权威发行版响应。
    ReleaseJsonPath=$2

    if [ "$ParserId" = "jq" ]; then
        jq --exit-status --raw-output \
            '.tag_name | select(type == "string") | gsub("^\\s+|\\s+$"; "") | select(length > 0)' \
            "$ReleaseJsonPath"
        exit
    fi
    node -e '
// fs provides exact UTF-8 release metadata reads.
// fs 提供精确的 UTF-8 发行元数据读取。
const fs = require("fs");
// release is the strictly decoded GitHub Latest Release response.
// release 是严格解码后的 GitHub Latest Release 响应。
const release = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
if (typeof release.tag_name !== "string" || release.tag_name.trim().length === 0) {
  throw new Error("GitHub Latest Release did not provide a non-empty tag_name.");
}
process.stdout.write(release.tag_name.trim());
' "$ReleaseJsonPath"
)

# Reads one exact release asset URL and mandatory GitHub SHA-256 digest.
# 读取一个精确发行资产 URL 与必需的 GitHub SHA-256 摘要。
#
# Parameters:
# 参数：
# The arguments select the parser, release JSON file, and exact asset name.
# 参数依次选择解析器、发行版 JSON 文件与精确资产名称。
#
# Returns:
# 返回值：
# Two lines containing the browser download URL and lowercase SHA-256 digest.
# 返回两行内容，依次为浏览器下载 URL 与小写 SHA-256 摘要。
read_release_asset() (
    # ParserId identifies the strict JSON parser selected at startup.
    # ParserId 标识启动时选择的严格 JSON 解析器。
    ParserId=$1
    # ReleaseJsonPath is the downloaded authoritative release response.
    # ReleaseJsonPath 是已下载的权威发行版响应。
    ReleaseJsonPath=$2
    # RequestedAssetName is the one platform asset accepted by the installer.
    # RequestedAssetName 是安装器接受的唯一平台资产。
    RequestedAssetName=$3

    if [ "$ParserId" = "jq" ]; then
        jq --exit-status --raw-output --arg asset "$RequestedAssetName" '
            [.assets[] | select(.name == $asset)] as $matches
            | if ($matches | length) != 1 then
                error("Expected exactly one release asset named " + $asset)
              else
                $matches[0]
              end
            | if (.browser_download_url | type == "string" and length > 0) then
                .
              else
                error("Release asset " + $asset + " does not provide a download URL")
              end
            | (.digest | capture("^sha256:(?<hash>[0-9a-fA-F]{64})$").hash) as $hash
            | .browser_download_url, ($hash | ascii_downcase)
        ' "$ReleaseJsonPath"
        exit
    fi
    node -e '
// fs provides exact UTF-8 release metadata reads.
// fs 提供精确的 UTF-8 发行元数据读取。
const fs = require("fs");
// release is the strictly decoded GitHub Latest Release response.
// release 是严格解码后的 GitHub Latest Release 响应。
const release = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
// assetName is the one exact platform archive requested by the installer.
// assetName 是安装器请求的唯一精确平台压缩包。
const assetName = process.argv[2];
// matches contains only assets whose names exactly equal the requested archive.
// matches 仅包含名称与请求压缩包精确相等的资产。
const matches = Array.isArray(release.assets)
  ? release.assets.filter((asset) => asset && asset.name === assetName)
  : [];
if (matches.length !== 1) {
  // available lists upstream asset names for actionable failure diagnostics.
  // available 列出上游资产名称，以提供可操作的失败诊断。
  const available = Array.isArray(release.assets)
    ? release.assets.map((asset) => asset && asset.name).filter(Boolean).join(", ")
    : "";
  throw new Error(`Expected exactly one release asset named ${assetName}. Available assets: ${available}`);
}
// asset is the unique release asset accepted for installation.
// asset 是接受用于安装的唯一发行资产。
const asset = matches[0];
// digestMatch validates and captures GitHub computed SHA-256 metadata.
// digestMatch 校验并捕获 GitHub 计算的 SHA-256 元数据。
const digestMatch = /^sha256:([0-9a-fA-F]{64})$/.exec(String(asset.digest || ""));
if (!digestMatch) {
  throw new Error(`Release asset ${assetName} does not provide a valid SHA-256 digest.`);
}
if (typeof asset.browser_download_url !== "string" || asset.browser_download_url.length === 0) {
  throw new Error(`Release asset ${assetName} does not provide a download URL.`);
}
process.stdout.write(`${asset.browser_download_url}\n${digestMatch[1].toLowerCase()}\n`);
' "$ReleaseJsonPath" "$RequestedAssetName"
)

# Calculates one local file's lowercase SHA-256 digest.
# 计算一个本地文件的小写 SHA-256 摘要。
#
# Parameters:
# 参数：
# The first argument is the exact file to hash.
# 第一个参数是要计算摘要的精确文件。
sha256_file() (
    # HashPath is the exact downloaded artifact to hash.
    # HashPath 是要计算摘要的精确下载制品。
    HashPath=$1

    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$HashPath" | awk '{ print tolower($1) }'
        exit
    fi
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$HashPath" | awk '{ print tolower($1) }'
        exit
    fi
    if command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 -r "$HashPath" | awk '{ print tolower($1) }'
        exit
    fi
    echo "sha256sum, shasum, or openssl is required to verify bundled tools." >&2
    exit 127
)

# Verifies one downloaded archive against the authoritative GitHub SHA-256 digest.
# 根据 GitHub 权威 SHA-256 摘要校验一个已下载压缩包。
#
# Parameters:
# 参数：
# The arguments are the downloaded path, expected digest, and diagnostic label.
# 参数依次为下载路径、预期摘要与诊断标签。
assert_sha256() (
    # ArchivePath is the exact downloaded file to verify.
    # ArchivePath 是要校验的精确下载文件。
    ArchivePath=$1
    # ExpectedHash is the mandatory authoritative digest.
    # ExpectedHash 是必需的权威摘要。
    ExpectedHash=$2
    # HashLabel identifies the asset in mismatch diagnostics.
    # HashLabel 在不匹配诊断中标识制品。
    HashLabel=$3
    # ActualHash is the digest calculated from the downloaded bytes.
    # ActualHash 是根据下载字节计算出的摘要。
    ActualHash=$(sha256_file "$ArchivePath")

    if [ "$ActualHash" != "$ExpectedHash" ]; then
        echo "Checksum mismatch for $HashLabel. Expected $ExpectedHash, got $ActualHash" >&2
        exit 1
    fi
)

# Extracts one verified archive into an empty destination directory.
# 把一个已校验压缩包解压到空目标目录。
#
# Parameters:
# 参数：
# The arguments are archive kind, archive path, and extraction destination.
# 参数依次为压缩类型、压缩包路径与解压目标。
extract_archive() (
    # ArchiveKind selects zip or tar.gz extraction behavior.
    # ArchiveKind 选择 zip 或 tar.gz 解压行为。
    ArchiveKind=$1
    # ArchivePath is the exact verified archive.
    # ArchivePath 是精确的已校验压缩包。
    ArchivePath=$2
    # ExtractDestination receives only verified archive contents.
    # ExtractDestination 仅接收已校验压缩包内容。
    ExtractDestination=$3

    if [ "$ArchiveKind" = "tar.gz" ]; then
        command -v tar >/dev/null 2>&1 || {
            echo "tar is required to extract $ArchivePath." >&2
            exit 127
        }
        tar -xzf "$ArchivePath" -C "$ExtractDestination"
        exit
    fi
    if command -v unzip >/dev/null 2>&1; then
        unzip -q "$ArchivePath" -d "$ExtractDestination"
        exit
    fi
    if command -v bsdtar >/dev/null 2>&1; then
        bsdtar -xf "$ArchivePath" -C "$ExtractDestination"
        exit
    fi
    if command -v ditto >/dev/null 2>&1; then
        ditto -x -k "$ArchivePath" "$ExtractDestination"
        exit
    fi
    echo "unzip, bsdtar, or ditto is required to extract $ArchivePath." >&2
    exit 127
)

# Validates that one installed executable reports the discovered release version.
# 校验一个已安装可执行文件是否报告查询得到的发行版本。
#
# Parameters:
# 参数：
# The arguments are binary path, expected release tag, and tool id.
# 参数依次为二进制路径、预期发行标签与工具标识。
assert_tool_version() (
    # BinaryPath is the executable being validated.
    # BinaryPath 是正在校验的可执行文件。
    BinaryPath=$1
    # ReleaseTag is the exact Latest Release tag selected for this installation.
    # ReleaseTag 是本次安装选择的精确 Latest Release 标签。
    ReleaseTag=$2
    # ToolId labels any validation failure.
    # ToolId 用于标识任何校验失败。
    ToolId=$3
    # ExpectedVersion removes the optional v prefix from the upstream tag.
    # ExpectedVersion 去除上游标签可选的 v 前缀。
    ExpectedVersion=${ReleaseTag#v}
    # VersionOutput captures stdout and stderr from the actual installed command.
    # VersionOutput 捕获实际已安装命令的标准输出与标准错误。
    if ! VersionOutput=$("$BinaryPath" --version 2>&1); then
        echo "Bundled tool '$ToolId' failed its --version check: $VersionOutput" >&2
        exit 1
    fi

    case "$VersionOutput" in
        *"$ExpectedVersion"*) ;;
        *)
            echo "Bundled tool '$ToolId' reported '$VersionOutput' instead of release '$ReleaseTag'." >&2
            exit 1
            ;;
    esac
)

# Checks whether existing metadata exactly matches one selected release asset.
# 检查既有元数据是否精确匹配一个选定发行资产。
#
# Parameters:
# 参数：
# The arguments identify the parser, metadata file, tool, repository, tag, asset, source binary, and digest.
# 参数依次标识解析器、元数据文件、工具、仓库、标签、资产、源二进制与摘要。
metadata_matches() (
    # ParserId identifies the strict JSON parser selected at startup.
    # ParserId 标识启动时选择的严格 JSON 解析器。
    ParserId=$1
    # MetadataPath is the previous successful installation record.
    # MetadataPath 是上次成功安装记录。
    MetadataPath=$2
    # ToolId is the installed command id.
    # ToolId 是已安装命令标识。
    ToolId=$3
    # RepositoryId is the authoritative owner/name repository.
    # RepositoryId 是权威的 owner/name 仓库标识。
    RepositoryId=$4
    # ReleaseTag is the selected latest release tag.
    # ReleaseTag 是选定的最新发行标签。
    ReleaseTag=$5
    # AssetName is the selected platform archive.
    # AssetName 是选定的平台压缩包。
    AssetName=$6
    # SourceBinaryName is the upstream executable name inside the archive.
    # SourceBinaryName 是压缩包内的上游可执行文件名。
    SourceBinaryName=$7
    # ExpectedHash is the selected archive's authoritative digest.
    # ExpectedHash 是选定压缩包的权威摘要。
    ExpectedHash=$8

    if [ "$ParserId" = "jq" ]; then
        jq --exit-status \
            --arg tool "$ToolId" \
            --arg repository "$RepositoryId" \
            --arg tag "$ReleaseTag" \
            --arg asset "$AssetName" \
            --arg source_binary "$SourceBinaryName" \
            --arg sha256 "$ExpectedHash" '
                .schema_version == 2
                and .tool == $tool
                and .repository == $repository
                and .tag == $tag
                and .asset == $asset
                and .source_binary == $source_binary
                and .sha256 == $sha256
            ' "$MetadataPath" >/dev/null
        exit
    fi
    node -e '
// fs provides exact UTF-8 installation metadata reads.
// fs 提供精确的 UTF-8 安装元数据读取。
const fs = require("fs");
// metadataText removes an optional Windows PowerShell UTF-8 BOM before strict JSON decoding.
// metadataText 在严格 JSON 解码前去除可选的 Windows PowerShell UTF-8 BOM。
const metadataText = fs.readFileSync(process.argv[1], "utf8").replace(/^\uFEFF/, "");
// metadata is the strictly decoded previous successful installation record.
// metadata 是严格解码后的上次成功安装记录。
const metadata = JSON.parse(metadataText);
// matches proves every identity field equals the selected latest release asset.
// matches 证明每个身份字段都等于选定的最新发行资产。
const matches = metadata.schema_version === 2
  && metadata.tool === process.argv[2]
  && metadata.repository === process.argv[3]
  && metadata.tag === process.argv[4]
  && metadata.asset === process.argv[5]
  && metadata.source_binary === process.argv[6]
  && metadata.sha256 === process.argv[7];
process.exit(matches ? 0 : 1);
' "$MetadataPath" "$ToolId" "$RepositoryId" "$ReleaseTag" "$AssetName" "$SourceBinaryName" "$ExpectedHash"
)

# Writes one schema-v2 successful-installation record atomically.
# 原子写入一个 schema-v2 成功安装记录。
#
# Parameters:
# 参数：
# The arguments identify the parser, destination, tool, repository, tag, asset, source binary, and digest.
# 参数依次标识解析器、目标、工具、仓库、标签、资产、源二进制与摘要。
write_metadata() (
    # ParserId identifies the strict JSON parser selected at startup.
    # ParserId 标识启动时选择的严格 JSON 解析器。
    ParserId=$1
    # MetadataPath is the durable installation record destination.
    # MetadataPath 是持久安装记录目标。
    MetadataPath=$2
    # ToolId is the installed command id.
    # ToolId 是已安装命令标识。
    ToolId=$3
    # RepositoryId is the authoritative owner/name repository.
    # RepositoryId 是权威的 owner/name 仓库标识。
    RepositoryId=$4
    # ReleaseTag is the selected latest release tag.
    # ReleaseTag 是选定的最新发行标签。
    ReleaseTag=$5
    # AssetName is the selected platform archive.
    # AssetName 是选定的平台压缩包。
    AssetName=$6
    # SourceBinaryName is the upstream executable name inside the archive.
    # SourceBinaryName 是压缩包内的上游可执行文件名。
    SourceBinaryName=$7
    # ExpectedHash is the selected archive's authoritative digest.
    # ExpectedHash 是选定压缩包的权威摘要。
    ExpectedHash=$8
    # MetadataTempPath is the same-directory atomic-write staging file.
    # MetadataTempPath 是同目录原子写入暂存文件。
    MetadataTempPath="$MetadataPath.tmp.$$"

    if [ "$ParserId" = "jq" ]; then
        jq --null-input \
            --arg tool "$ToolId" \
            --arg repository "$RepositoryId" \
            --arg tag "$ReleaseTag" \
            --arg asset "$AssetName" \
            --arg source_binary "$SourceBinaryName" \
            --arg sha256 "$ExpectedHash" '
                {
                    schema_version: 2,
                    tool: $tool,
                    repository: $repository,
                    tag: $tag,
                    asset: $asset,
                    source_binary: $source_binary,
                    sha256: $sha256
                }
            ' > "$MetadataTempPath"
    else
        node -e '
// fs provides exact UTF-8 metadata writes.
// fs 提供精确的 UTF-8 元数据写入。
const fs = require("fs");
// metadata is the schema-v2 successful installation record written atomically by the shell wrapper.
// metadata 是由 shell 包装层原子写入的 schema-v2 成功安装记录。
const metadata = {
  schema_version: 2,
  tool: process.argv[2],
  repository: process.argv[3],
  tag: process.argv[4],
  asset: process.argv[5],
  source_binary: process.argv[6],
  sha256: process.argv[7],
};
fs.writeFileSync(process.argv[1], `${JSON.stringify(metadata, null, 2)}\n`, "utf8");
' "$MetadataTempPath" "$ToolId" "$RepositoryId" "$ReleaseTag" "$AssetName" "$SourceBinaryName" "$ExpectedHash"
    fi
    mv -f "$MetadataTempPath" "$MetadataPath"
)

# Installs one latest-release CLI into the current target payload directory.
# 把一个 Latest Release CLI 安装到当前目标载荷目录。
#
# Parameters:
# 参数：
# The arguments are sg or rg, its authoritative repository, target triple, and JSON parser id.
# 参数依次为 sg 或 rg、其权威仓库、目标三元组与 JSON 解析器标识。
install_latest_tool() (
    # ToolId is the command installed into the shared bin directory.
    # ToolId 是安装到共享 bin 目录的命令。
    ToolId=$1
    # RepositoryId is the authoritative owner/name upstream repository.
    # RepositoryId 是权威的 owner/name 上游仓库。
    RepositoryId=$2
    # ToolTarget is the canonical current-host installation target.
    # ToolTarget 是规范的当前宿主安装目标。
    ToolTarget=$3
    # ParserId identifies the strict JSON parser selected at startup.
    # ParserId 标识启动时选择的严格 JSON 解析器。
    ParserId=$4
    # TempBase is the resolved temporary parent allowed to own installer transaction directories.
    # TempBase 是允许持有安装事务目录的已解析临时父目录。
    TempBase=$(CDPATH= cd "${TMPDIR:-/tmp}" && pwd -P)
    # TempRoot isolates this exact release query, download, and extraction transaction.
    # TempRoot 隔离本次精确发行查询、下载与解压事务。
    TempRoot=$(mktemp -d "$TempBase/vulcan_tools.XXXXXX")
    # ReleaseJsonPath stores the authoritative Latest Release API response.
    # ReleaseJsonPath 保存权威 Latest Release API 响应。
    ReleaseJsonPath="$TempRoot/release.json"

    # cleanup_install_temp removes only the temporary directory owned by this invocation.
    # cleanup_install_temp 仅删除本次调用持有的临时目录。
    cleanup_install_temp() {
        if [ -n "${TempRoot:-}" ] && [ -d "$TempRoot" ]; then
            case "$TempRoot" in
                "$TempBase"/vulcan_tools.??????) rm -rf -- "$TempRoot" ;;
                *)
                    echo "Refusing to clean unexpected bundled-tools temp path: $TempRoot" >&2
                    ;;
            esac
        fi
    }
    trap cleanup_install_temp 0 HUP INT TERM

    echo "==> Resolving latest $ToolId release from $RepositoryId..."
    download_file "https://api.github.com/repos/$RepositoryId/releases/latest" "$ReleaseJsonPath"
    # ReleaseTag is the exact upstream version selected for this invocation.
    # ReleaseTag 是本次调用选择的精确上游版本。
    ReleaseTag=$(read_release_tag "$ParserId" "$ReleaseJsonPath")
    # BinarySuffix selects the Windows executable suffix only for an MSVC target.
    # BinarySuffix 仅为 MSVC 目标选择 Windows 可执行文件后缀。
    case "$ToolTarget" in
        *-pc-windows-msvc) BinarySuffix=".exe" ;;
        *) BinarySuffix="" ;;
    esac

    if [ "$ToolId" = "sg" ]; then
        # AssetName is the ast-grep application archive for the current target.
        # AssetName 是当前目标对应的 ast-grep 应用压缩包。
        AssetName="app-$ToolTarget.zip"
        # ArchiveKind selects ZIP extraction for all ast-grep application archives.
        # ArchiveKind 为全部 ast-grep 应用压缩包选择 ZIP 解压。
        ArchiveKind="zip"
        # SourceBinaryName is the non-deprecated upstream command copied from the archive.
        # SourceBinaryName 是从压缩包复制的非弃用上游命令。
        SourceBinaryName="ast-grep$BinarySuffix"
        # BinaryName is the short command exposed through the injected PATH.
        # BinaryName 是通过注入 PATH 暴露的短命令。
        BinaryName="sg$BinarySuffix"
    else
        # RipgrepAssetTarget selects static musl Linux archives and native Darwin or Windows archives.
        # RipgrepAssetTarget 为 Linux 选择静态 musl 压缩包，并为 Darwin 或 Windows 选择原生压缩包。
        case "$ToolTarget" in
            x86_64-unknown-linux-gnu) RipgrepAssetTarget="x86_64-unknown-linux-musl" ;;
            aarch64-unknown-linux-gnu) RipgrepAssetTarget="aarch64-unknown-linux-musl" ;;
            *) RipgrepAssetTarget="$ToolTarget" ;;
        esac
        # ArchiveKind selects ZIP on Windows and tar.gz on Unix-like hosts.
        # ArchiveKind 在 Windows 上选择 ZIP，在 Unix-like 宿主上选择 tar.gz。
        case "$ToolTarget" in
            *-pc-windows-msvc) ArchiveKind="zip" ;;
            *) ArchiveKind="tar.gz" ;;
        esac
        # AssetName is the exact versioned ripgrep archive for the current host.
        # AssetName 是当前宿主对应的精确版本化 ripgrep 压缩包。
        AssetName="ripgrep-$ReleaseTag-$RipgrepAssetTarget.$ArchiveKind"
        # SourceBinaryName is the upstream executable inside the archive.
        # SourceBinaryName 是压缩包内的上游可执行文件。
        SourceBinaryName="rg$BinarySuffix"
        # BinaryName is the command exposed through the injected PATH.
        # BinaryName 是通过注入 PATH 暴露的命令。
        BinaryName="rg$BinarySuffix"
    fi

    # AssetData contains the exact download URL and mandatory digest on separate lines.
    # AssetData 在不同行保存精确下载 URL 与必需摘要。
    AssetData=$(read_release_asset "$ParserId" "$ReleaseJsonPath" "$AssetName")
    # AssetUrl is the authoritative browser download URL for the selected asset.
    # AssetUrl 是选定资产的权威浏览器下载 URL。
    AssetUrl=$(printf '%s\n' "$AssetData" | sed -n '1p')
    # ExpectedHash is the mandatory lowercase GitHub-computed archive digest.
    # ExpectedHash 是 GitHub 计算的必需小写压缩包摘要。
    ExpectedHash=$(printf '%s\n' "$AssetData" | sed -n '2p')
    # TargetRoot stores all current-platform tool payloads.
    # TargetRoot 保存全部当前平台工具载荷。
    TargetRoot="$ToolsRoot/$ToolTarget"
    # TargetBinDir is synchronized directly into output/tools/bin.
    # TargetBinDir 会被直接同步到 output/tools/bin。
    TargetBinDir="$TargetRoot/bin"
    # LicenseRoot stores distributable upstream license texts by tool.
    # LicenseRoot 按工具保存可分发的上游许可证文本。
    LicenseRoot="$TargetRoot/licenses/$ToolId"
    # BinaryDestination is the exact installed command resolved through PATH.
    # BinaryDestination 是通过 PATH 解析的精确已安装命令。
    BinaryDestination="$TargetBinDir/$BinaryName"
    # MetadataPath records the resolved latest tag and verified archive identity.
    # MetadataPath 记录已解析的最新标签与已校验压缩包身份。
    MetadataPath="$TargetRoot/$ToolId.json"
    mkdir -p "$TargetBinDir" "$LicenseRoot"

    # BinaryIsCurrent proves metadata identity and executable version before skipping download.
    # BinaryIsCurrent 在跳过下载前同时证明元数据身份与可执行文件版本。
    BinaryIsCurrent=0
    if [ -f "$MetadataPath" ] \
        && [ -f "$BinaryDestination" ] \
        && metadata_matches "$ParserId" "$MetadataPath" "$ToolId" "$RepositoryId" "$ReleaseTag" "$AssetName" "$SourceBinaryName" "$ExpectedHash" \
        && assert_tool_version "$BinaryDestination" "$ReleaseTag" "$ToolId" >/dev/null 2>&1; then
        BinaryIsCurrent=1
    fi

    if [ "$BinaryIsCurrent" -eq 0 ]; then
        # ArchivePath is the local verified release archive.
        # ArchivePath 是本地已校验发行压缩包。
        ArchivePath="$TempRoot/$AssetName"
        # ExtractRoot contains only files expanded from the verified archive.
        # ExtractRoot 仅包含从已校验压缩包展开的文件。
        ExtractRoot="$TempRoot/expanded"
        mkdir -p "$ExtractRoot"
        echo "==> Downloading $ToolId $ReleaseTag: $AssetName"
        download_file "$AssetUrl" "$ArchivePath"
        assert_sha256 "$ArchivePath" "$ExpectedHash" "$AssetName"
        extract_archive "$ArchiveKind" "$ArchivePath" "$ExtractRoot"
        # BinarySource is the exact executable discovered inside the verified archive.
        # BinarySource 是在已校验压缩包内发现的精确可执行文件。
        BinarySource=$(find "$ExtractRoot" -type f -name "$SourceBinaryName" -print | sed -n '1p')
        if [ -z "$BinarySource" ] || [ ! -f "$BinarySource" ]; then
            echo "Executable '$SourceBinaryName' was not found in '$AssetName'." >&2
            exit 1
        fi
        cp -f "$BinarySource" "$BinaryDestination"
        chmod 755 "$BinaryDestination"
        assert_tool_version "$BinaryDestination" "$ReleaseTag" "$ToolId"
        write_metadata "$ParserId" "$MetadataPath" "$ToolId" "$RepositoryId" "$ReleaseTag" "$AssetName" "$SourceBinaryName" "$ExpectedHash"
    else
        echo "==> $ToolId is already current ($ReleaseTag)."
    fi

    if [ "$ToolId" = "sg" ]; then
        download_file "https://raw.githubusercontent.com/$RepositoryId/$ReleaseTag/LICENSE" "$LicenseRoot/LICENSE"
    else
        download_file "https://raw.githubusercontent.com/$RepositoryId/$ReleaseTag/LICENSE-MIT" "$LicenseRoot/LICENSE-MIT"
        download_file "https://raw.githubusercontent.com/$RepositoryId/$ReleaseTag/UNLICENSE" "$LicenseRoot/UNLICENSE"
    fi
    echo "==> $ToolId $ReleaseTag installed: $BinaryDestination"
)

# JsonParser identifies the strict metadata parser shared by both tool installations.
# JsonParser 标识两个工具安装共用的严格元数据解析器。
JsonParser=$(select_json_parser)

# CurrentToolTarget is the canonical host target shared by sg and rg.
# CurrentToolTarget 是 sg 与 rg 共用的规范宿主目标。
CurrentToolTarget=$(get_current_tool_target)

mkdir -p "$ToolsRoot"
install_latest_tool "sg" "$AstGrepRepo" "$CurrentToolTarget" "$JsonParser"
install_latest_tool "rg" "$RipgrepRepo" "$CurrentToolTarget" "$JsonParser"

echo "==> Latest bundled tools installed under: $ToolsRoot"
