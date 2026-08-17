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

# TauriManifestPath points at the Tauri/WebUI desktop shell manifest used as the dependency graph root.
# TauriManifestPath 指向作为依赖图根节点的 Tauri/WebUI 桌面壳清单文件。
$TauriManifestPath = Join-Path $ProjectDir "apps/vulcan-code-tauri/Cargo.toml"

# Convert-ToStableLicenseIdentifier maps one free-form title to a lowercase stable identifier fragment.
# Convert-ToStableLicenseIdentifier 用于把自由格式标题映射为小写稳定标识片段。
function Convert-ToStableLicenseIdentifier {
    param([string]$Title)

    $Slug = ($Title.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')
    if ([string]::IsNullOrWhiteSpace($Slug)) {
        return "license"
    }
    return $Slug
}

# Get-RustThirdPartyNoticeSectionIndex parses the generated Markdown and emits a lightweight JSON index for lazy section loading.
# Get-RustThirdPartyNoticeSectionIndex 用于解析生成后的 Markdown，并输出轻量 JSON 索引供懒加载分段使用。
function Get-RustThirdPartyNoticeSectionIndex {
    param([string]$MarkdownPath)

    $Lines = [System.IO.File]::ReadAllLines($MarkdownPath)
    $Entries = New-Object System.Collections.Generic.List[object]
    $MergedEntryIndexBySignature = @{}
    $LineIndex = 0

    while ($LineIndex -lt $Lines.Length) {
        $CurrentLine = $Lines[$LineIndex].Trim()
        if (-not $CurrentLine.StartsWith("### ")) {
            $LineIndex += 1
            continue
        }

        $Title = $CurrentLine.Substring(4).Trim()
        $UsedByCount = 0
        $UsedByEntries = New-Object System.Collections.Generic.List[string]
        $ScanIndex = $LineIndex + 1
        $StartLine = $null
        $EndLine = $null

        while ($ScanIndex -lt $Lines.Length) {
            $ScannedLine = $Lines[$ScanIndex].Trim()
            if ($ScannedLine.StartsWith("### ")) {
                break
            }

            if ($ScannedLine -eq "Used by:") {
                $ScanIndex += 1
                while ($ScanIndex -lt $Lines.Length) {
                    $UsedByLine = $Lines[$ScanIndex].Trim()
                    if ($UsedByLine -eq '```text') {
                        break
                    }
                    if ($UsedByLine.StartsWith("- ")) {
                        $UsedByCount += 1
                        $UsedByEntries.Add($UsedByLine.Substring(2).Trim())
                    }
                    $ScanIndex += 1
                }
                continue
            }

            if ($ScannedLine -eq '```text') {
                $FirstLine = $ScanIndex + 2
                $LastLine = $ScanIndex + 1
                $ScanIndex += 1
                while ($ScanIndex -lt $Lines.Length) {
                    if ($Lines[$ScanIndex].Trim() -eq '```') {
                        $LastLine = $ScanIndex
                        break
                    }
                    $ScanIndex += 1
                }

                if ($LastLine -ge $FirstLine) {
                    while ($FirstLine -le $LastLine -and [string]::IsNullOrWhiteSpace($Lines[$FirstLine - 1])) {
                        $FirstLine += 1
                    }
                    while ($LastLine -ge $FirstLine -and [string]::IsNullOrWhiteSpace($Lines[$LastLine - 1])) {
                        $LastLine -= 1
                    }
                    if ($FirstLine -le $LastLine) {
                        $StartLine = $FirstLine
                        $EndLine = $LastLine
                    }
                }
                break
            }

            $ScanIndex += 1
        }

        if ($null -ne $StartLine -and $null -ne $EndLine) {
            $MergeKey = $Title
            if ($MergedEntryIndexBySignature.ContainsKey($MergeKey)) {
                $ExistingEntry = $Entries[$MergedEntryIndexBySignature[$MergeKey]]
                $ExistingEntry.used_by_entries = @($ExistingEntry.used_by_entries) + @($UsedByEntries)
                $ExistingEntry.used_by_count = @($ExistingEntry.used_by_entries).Count
            }
            else {
                $MergedEntryIndexBySignature[$MergeKey] = $Entries.Count
                $Entries.Add([pscustomobject]@{
                        id = ""
                        title = $Title
                        used_by_count = $UsedByCount
                        used_by_entries = @($UsedByEntries)
                        start_line = $StartLine
                        end_line = $EndLine
                        line_count = ($EndLine - $StartLine + 1)
                    })
            }
        }

        $LineIndex = $ScanIndex + 1
    }

    $SeenIds = @{}
    foreach ($Entry in $Entries) {
        $StableId = Convert-ToStableLicenseIdentifier -Title $Entry.title
        $BaseId = "rust-dependency-license-$StableId"
        if ($SeenIds.ContainsKey($BaseId)) {
            $SeenIds[$BaseId] += 1
            $Entry.id = "$BaseId-$($SeenIds[$BaseId])"
        }
        else {
            $SeenIds[$BaseId] = 1
            $Entry.id = $BaseId
        }
    }

    return [pscustomobject]@{
        source_file = [System.IO.Path]::GetFileName($MarkdownPath)
        licenses = $Entries
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
        throw 'Missing rtk. Run `make deps rtk` before generating Rust third-party notices.'
    }

    if (-not (Test-Path -LiteralPath $CargoAboutConfigFile)) {
        throw "Missing cargo-about config: $CargoAboutConfigFile"
    }
    if (-not (Test-Path -LiteralPath $CargoAboutTemplateFile)) {
        throw "Missing cargo-about template: $CargoAboutTemplateFile"
    }
    if (-not (Test-Path -LiteralPath $TauriManifestPath)) {
        throw "Missing Tauri/WebUI manifest: $TauriManifestPath"
    }

    & $RtkCommand.Source cargo about generate $CargoAboutTemplateFile --manifest-path $TauriManifestPath --config $CargoAboutConfigFile --output-file $GeneratedThirdPartyNoticesFile --locked --fail
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    # GeneratedMarkdown stores cargo-about output before repository newline normalization.
    # GeneratedMarkdown 保存仓库换行规范化前的 cargo-about 输出。
    $GeneratedMarkdown = [System.IO.File]::ReadAllText($GeneratedThirdPartyNoticesFile)
    # NormalizedMarkdown makes the checked-in artifact byte-stable across Windows, Linux, and macOS.
    # NormalizedMarkdown 使签入制品在 Windows、Linux 与 macOS 间保持字节稳定。
    $NormalizedMarkdown = ($GeneratedMarkdown -replace "`r`n", "`n") -replace "`r", "`n"
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
    $SectionIndexJson = (($SectionIndex | ConvertTo-Json -Depth 6) -replace "`r`n", "`n") + "`n"
    [System.IO.File]::WriteAllText(
        $GeneratedThirdPartyNoticesIndexFile,
        $SectionIndexJson,
        [System.Text.UTF8Encoding]::new($false)
    )

    if ($SectionIndex.licenses.Count -eq 0) {
        throw "Generated Rust third-party notice index does not contain any license sections."
    }

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
