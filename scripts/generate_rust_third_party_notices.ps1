# generate_rust_third_party_notices.ps1 builds the application-side Rust dependency notice document through cargo-about.
# generate_rust_third_party_notices.ps1 用于通过 cargo-about 构建应用侧 Rust 依赖声明文档。

[CmdletBinding()]
param(
    # CheckOnly verifies generated notices against repository-owned source artifacts without replacing them.
    # CheckOnly 对照仓库持有的源码制品验证生成结果，且不替换这些制品。
    [switch]$CheckOnly
)

$ErrorActionPreference = "Stop"

# Relaunch under PowerShell 7 when available so UTF-8 comments and modern syntax stay stable.
# 若检测到 PowerShell 7，则自动在其下重启，确保 UTF-8 注释与现代语法保持稳定。
if ($PSVersionTable.PSEdition -ne "Core") {
    $CurrentScriptPath = $MyInvocation.MyCommand.Path
    if (-not [string]::IsNullOrWhiteSpace($PSCommandPath)) {
        $CurrentScriptPath = $PSCommandPath
    }
    $PowerShellCore = Get-Command "pwsh" -ErrorAction SilentlyContinue
    if ($PowerShellCore -and -not [string]::IsNullOrWhiteSpace($CurrentScriptPath)) {
        # RelaunchArguments preserves the script mode when forwarding execution to PowerShell 7.
        # RelaunchArguments 在把执行转发到 PowerShell 7 时保留脚本模式。
        $RelaunchArguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $CurrentScriptPath)
        if ($CheckOnly) {
            $RelaunchArguments += "-CheckOnly"
        }
        & $PowerShellCore.Source @RelaunchArguments
        exit $LASTEXITCODE
    }
}

# ProjectDir keeps the repository root stable so all derived paths remain deterministic.
# ProjectDir 用于固定仓库根目录，确保所有派生路径保持确定性。
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

# SourceLicenseResourcesRoot points at the repository-owned runtime license source directory.
# SourceLicenseResourcesRoot 指向仓库持有的运行时许可证源码目录。
$SourceLicenseResourcesRoot = Join-Path $ProjectDir "crates/vulcan-runtime-assets/resources/licenses"

# VerificationLicenseResourcesRoot isolates check-only output under Cargo's ignored build directory.
# VerificationLicenseResourcesRoot 把只读检查输出隔离到 Cargo 忽略的构建目录下。
$VerificationLicenseResourcesRoot = Join-Path $ProjectDir "target/dependency-policy/licenses"

# GeneratedLicenseResourcesRoot selects repository artifacts for generation and ignored artifacts for comparison.
# GeneratedLicenseResourcesRoot 为正式生成选择仓库制品，为差异检查选择忽略的构建制品。
$GeneratedLicenseResourcesRoot = if ($CheckOnly) { $VerificationLicenseResourcesRoot } else { $SourceLicenseResourcesRoot }

# GeneratedThirdPartyNoticesFile points at the generated Markdown notice file.
# GeneratedThirdPartyNoticesFile 指向生成的 Markdown 声明文件。
$GeneratedThirdPartyNoticesFile = Join-Path $GeneratedLicenseResourcesRoot "THIRD_PARTY_NOTICES.md"

# GeneratedThirdPartyNoticesIndexFile points at the generated JSON sidecar used for lazy section loading.
# GeneratedThirdPartyNoticesIndexFile 指向供懒加载分段使用的生成 JSON 旁路索引文件。
$GeneratedThirdPartyNoticesIndexFile = Join-Path $GeneratedLicenseResourcesRoot "THIRD_PARTY_NOTICES_INDEX.json"

# SourceThirdPartyNoticesFile points at the authoritative Markdown artifact checked into runtime assets.
# SourceThirdPartyNoticesFile 指向签入运行时资源的权威 Markdown 制品。
$SourceThirdPartyNoticesFile = Join-Path $SourceLicenseResourcesRoot "THIRD_PARTY_NOTICES.md"

# SourceThirdPartyNoticesIndexFile points at the authoritative JSON index checked into runtime assets.
# SourceThirdPartyNoticesIndexFile 指向签入运行时资源的权威 JSON 索引。
$SourceThirdPartyNoticesIndexFile = Join-Path $SourceLicenseResourcesRoot "THIRD_PARTY_NOTICES_INDEX.json"

# CargoAboutConfigFile points at the cargo-about acceptance policy used for generation.
# CargoAboutConfigFile 指向生成时使用的 cargo-about 许可接受策略文件。
$CargoAboutConfigFile = Join-Path $ProjectDir "about.toml"

# CargoAboutTemplateFile points at the cargo-about Handlebars template used for Markdown output.
# CargoAboutTemplateFile 指向用于 Markdown 输出的 cargo-about Handlebars 模板。
$CargoAboutTemplateFile = Join-Path $ProjectDir "about.hbs"

# WorkspaceManifestPath points at the workspace manifest so every shipped binary's graph is scanned together.
# WorkspaceManifestPath 指向工作区清单，使全部分发二进制的依赖图一并扫描。
$WorkspaceManifestPath = Join-Path $ProjectDir "Cargo.toml"

# Load the shared deterministic indexer used by generation and fixture validation.
# 加载生成流程与样例验证共用的确定性索引器。
. (Join-Path $PSScriptRoot "rust_third_party_notices_index.ps1")

# Load the shipped package list shared with the distribution build.
# 加载与发行构建共用的分发包列表。
. (Join-Path $PSScriptRoot "shipped_rust_packages.ps1")

# Assert-RustThirdPartyNoticeScope rejects notice entries outside the shipped packages' non-dev dependency graph.
# Assert-RustThirdPartyNoticeScope 拒绝落在分发包非开发依赖图之外的声明条目。
function Assert-RustThirdPartyNoticeScope {
    param(
        # SectionIndex is the generated index whose dependencies are checked.
        # SectionIndex 是待检查依赖的生成索引。
        [Parameter(Mandatory = $true)]$SectionIndex,
        # RtkPath is the repository command proxy used for Cargo execution.
        # RtkPath 是 Cargo 执行所用的仓库命令代理。
        [Parameter(Mandatory = $true)][string]$RtkPath
    )

    # The workspace scan also sees non-shipped members; any crate only they or dev edges reach must not appear.
    # Build edges stay allowed to match about.toml, and the allowed set spans all targets because cargo-about
    # filters targets per edge, which keeps it a superset of the generated output.
    # 工作区扫描也会看到非分发成员；仅由它们或开发依赖边引入的 crate 不得出现。
    # 构建依赖边与 about.toml 一致保持允许；cargo-about 逐边过滤目标平台，因此允许集合覆盖全部目标，以保持为生成输出的超集。
    # Every inventory package is scanned whatever the current platform or build kind, matching `--target all`.
    # 无论当前平台或构建类别，清单中的每个包都参与扫描，与 `--target all` 一致。
    $PackageArguments = @($ShippedRustPackages | Sort-Object -Unique | ForEach-Object { '-p'; $_ })
    $TreeArguments = @('cargo', 'tree', '--locked', '-e', 'normal,build', '--target', 'all', '--prefix', 'none', '--format', '{p}') + $PackageArguments
    $TreeOutput = & $RtkPath @TreeArguments
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    # ShippedCrates stores `name@version` keys reachable from shipped packages through normal and build edges.
    # ShippedCrates 保存经普通与构建依赖边可从分发包到达的 `name@version` 键。
    $ShippedCrates = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($Line in $TreeOutput) {
        if ($Line -match '^(\S+) v(\S+)') {
            [void]$ShippedCrates.Add("$($Matches[1])@$($Matches[2])")
        }
    }
    foreach ($PackageName in $ShippedRustPackages) {
        if (-not ($ShippedCrates | Where-Object { $_.StartsWith("$PackageName@") })) {
            throw "cargo tree output does not list shipped package $PackageName; the scope check cannot run."
        }
    }

    # OutsideCrates lists notice dependencies that no shipped package reaches.
    # OutsideCrates 列出任何分发包都不会到达的声明依赖。
    $OutsideCrates = @(
        foreach ($License in $SectionIndex.licenses) {
            foreach ($Section in $License.sections) {
                foreach ($Dependency in $Section.dependencies) {
                    $Key = "$($Dependency.name)@$($Dependency.version)"
                    if (-not $ShippedCrates.Contains($Key)) { $Key }
                }
            }
        }
    ) | Sort-Object -Unique
    if ($OutsideCrates.Count -gt 0) {
        throw "Rust third-party notices include crates outside the shipped binaries' dependency graph: $($OutsideCrates -join ', '). Mark non-shipped workspace members ``publish = false`` or update scripts/shipped-rust-packages.json."
    }
}

# Invoke-GenerateRustThirdPartyNotices generates the Markdown notice file and its lightweight section index.
# Invoke-GenerateRustThirdPartyNotices 用于生成 Markdown 声明文件及其轻量分段索引。
function Invoke-GenerateRustThirdPartyNotices {
    if (-not (Test-Path -LiteralPath $GeneratedLicenseResourcesRoot)) {
        New-Item -ItemType Directory -Path $GeneratedLicenseResourcesRoot -Force | Out-Null
    }

    $CargoAboutCommand = Get-Command "cargo-about" -ErrorAction SilentlyContinue
    if (-not $CargoAboutCommand) {
        throw 'Missing cargo-about. Run `make deps-rust-tools` before generating Rust third-party notices.'
    }

    # RtkCommand resolves the mandatory repository command proxy used for Cargo execution.
    # RtkCommand 解析 Cargo 执行所需的仓库强制命令代理。
    $RtkCommand = Get-Command "rtk" -ErrorAction SilentlyContinue
    if (-not $RtkCommand) {
        throw 'Missing rtk. Run `make deps tools` before generating Rust third-party notices.'
    }

    if (-not (Test-Path -LiteralPath $CargoAboutConfigFile)) {
        throw "Missing cargo-about config: $CargoAboutConfigFile"
    }
    if (-not (Test-Path -LiteralPath $CargoAboutTemplateFile)) {
        throw "Missing cargo-about template: $CargoAboutTemplateFile"
    }
    if (-not (Test-Path -LiteralPath $WorkspaceManifestPath)) {
        throw "Missing workspace manifest: $WorkspaceManifestPath"
    }

    # Private members (`publish = false`) are omitted by about.toml, but their non-dev dependencies stay in scope.
    # 私有成员（`publish = false`）由 about.toml 省略，但其非开发依赖仍在扫描范围内。
    & $RtkCommand.Source cargo about generate $CargoAboutTemplateFile --workspace --manifest-path $WorkspaceManifestPath --config $CargoAboutConfigFile --output-file $GeneratedThirdPartyNoticesFile --locked --fail
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    # GeneratedMarkdown stores cargo-about output before repository newline normalization.
    # GeneratedMarkdown 保存仓库换行规范化前的 cargo-about 输出。
    $GeneratedMarkdown = [System.IO.File]::ReadAllText($GeneratedThirdPartyNoticesFile)
    # NormalizedMarkdown makes the checked-in artifact byte-stable and removes upstream trailing whitespace.
    # NormalizedMarkdown 使签入制品保持字节稳定，并移除上游文本中的行尾空白。
    $NormalizedMarkdown = (($GeneratedMarkdown -replace "`r`n", "`n") -replace "`r", "`n") -replace '(?m)[ \t]+$', ''
    [System.IO.File]::WriteAllText(
        $GeneratedThirdPartyNoticesFile,
        $NormalizedMarkdown,
        [System.Text.UTF8Encoding]::new($false)
    )

    # SectionIndex stores the deterministic lazy-loading index derived from the generated Markdown.
    # SectionIndex 保存从生成 Markdown 派生的确定性懒加载索引。
    $SectionIndex = Get-RustThirdPartyNoticeSectionIndex -MarkdownPath $GeneratedThirdPartyNoticesFile
    # SectionIndexJson stores normalized UTF-8 JSON with one terminal LF for cross-platform hashing.
    # SectionIndexJson 保存使用单个结尾 LF 规范化的 UTF-8 JSON，供跨平台哈希使用。
    $SectionIndexJson = (($SectionIndex | ConvertTo-Json -Depth 10) -replace "`r`n", "`n") + "`n"
    [System.IO.File]::WriteAllText(
        $GeneratedThirdPartyNoticesIndexFile,
        $SectionIndexJson,
        [System.Text.UTF8Encoding]::new($false)
    )

    if ($SectionIndex.licenses.Count -eq 0) {
        throw "Generated Rust third-party notice index does not contain any license sections."
    }
    Assert-RustThirdPartyNoticeScope -SectionIndex $SectionIndex -RtkPath $RtkCommand.Source

    if ($CheckOnly) {
        if (-not (Test-Path -LiteralPath $SourceThirdPartyNoticesFile)) {
            throw "Missing authoritative Rust third-party notice artifact: $SourceThirdPartyNoticesFile"
        }
        if (-not (Test-Path -LiteralPath $SourceThirdPartyNoticesIndexFile)) {
            throw "Missing authoritative Rust third-party notice index: $SourceThirdPartyNoticesIndexFile"
        }

        # GeneratedNoticeHash stores the generated Markdown digest used for byte-exact consistency validation.
        # GeneratedNoticeHash 保存用于逐字节一致性验证的生成 Markdown 摘要。
        $GeneratedNoticeHash = (Get-FileHash -LiteralPath $GeneratedThirdPartyNoticesFile -Algorithm SHA256).Hash
        # SourceNoticeHash stores the authoritative Markdown digest used for byte-exact consistency validation.
        # SourceNoticeHash 保存用于逐字节一致性验证的权威 Markdown 摘要。
        $SourceNoticeHash = (Get-FileHash -LiteralPath $SourceThirdPartyNoticesFile -Algorithm SHA256).Hash
        if ($GeneratedNoticeHash -ne $SourceNoticeHash) {
            throw "Rust third-party notices are stale. Run `make licenses` and review the generated source artifact."
        }

        # GeneratedIndexHash stores the generated JSON digest used for byte-exact consistency validation.
        # GeneratedIndexHash 保存用于逐字节一致性验证的生成 JSON 摘要。
        $GeneratedIndexHash = (Get-FileHash -LiteralPath $GeneratedThirdPartyNoticesIndexFile -Algorithm SHA256).Hash
        # SourceIndexHash stores the authoritative JSON digest used for byte-exact consistency validation.
        # SourceIndexHash 保存用于逐字节一致性验证的权威 JSON 摘要。
        $SourceIndexHash = (Get-FileHash -LiteralPath $SourceThirdPartyNoticesIndexFile -Algorithm SHA256).Hash
        if ($GeneratedIndexHash -ne $SourceIndexHash) {
            throw "Rust third-party notice index is stale. Run `make licenses` and review the generated source artifact."
        }

        Write-Host "==> Rust third-party notice source artifacts are current."
        return
    }

    Write-Host "==> Rust third-party notices generated to $GeneratedThirdPartyNoticesFile"
    Write-Host "==> Rust third-party notice index generated to $GeneratedThirdPartyNoticesIndexFile"
    Write-Host "==> Run `make sync` to copy the reviewed source artifacts into output."
}

Invoke-GenerateRustThirdPartyNotices
