# generate_rust_third_party_notices.ps1 builds the application-side Rust dependency notice document through cargo-about.
# generate_rust_third_party_notices.ps1 用于通过 cargo-about 构建应用侧 Rust 依赖声明文档。

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
        & $PowerShellCore.Source -NoProfile -ExecutionPolicy Bypass -File $CurrentScriptPath
        exit $LASTEXITCODE
    }
}

# ProjectDir keeps the repository root stable so all derived paths remain deterministic.
# ProjectDir 用于固定仓库根目录，确保所有派生路径保持确定性。
$ProjectDir = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectDir

# OutputRoot points at the runtime output bundle root used by the packaged desktop app.
# OutputRoot 指向桌面应用打包后使用的运行时 output 根目录。
$OutputRoot = Join-Path $ProjectDir "output"

# OutputResourcesRoot points at the packaged runtime resource directory nested below output.
# OutputResourcesRoot 指向 output 下的打包运行时资源目录。
$OutputResourcesRoot = Join-Path $OutputRoot "resources"

# OutputLicenseResourcesRoot points at the runtime license directory that should store generated notices.
# OutputLicenseResourcesRoot 指向应存放生成声明文件的运行时许可证目录。
$OutputLicenseResourcesRoot = Join-Path $OutputResourcesRoot "licenses"

# OutputThirdPartyNoticesFile points at the generated Markdown notice file consumed by the About page.
# OutputThirdPartyNoticesFile 指向供关于页面消费的生成 Markdown 声明文件。
$OutputThirdPartyNoticesFile = Join-Path $OutputLicenseResourcesRoot "THIRD_PARTY_NOTICES.md"

# OutputThirdPartyNoticesIndexFile points at the generated JSON sidecar used for lazy section loading.
# OutputThirdPartyNoticesIndexFile 指向供懒加载分段使用的生成 JSON 旁路索引文件。
$OutputThirdPartyNoticesIndexFile = Join-Path $OutputLicenseResourcesRoot "THIRD_PARTY_NOTICES_INDEX.json"

# CargoAboutConfigFile points at the cargo-about acceptance policy used for generation.
# CargoAboutConfigFile 指向生成时使用的 cargo-about 许可接受策略文件。
$CargoAboutConfigFile = Join-Path $ProjectDir "about.toml"

# CargoAboutTemplateFile points at the cargo-about Handlebars template used for Markdown output.
# CargoAboutTemplateFile 指向用于 Markdown 输出的 cargo-about Handlebars 模板。
$CargoAboutTemplateFile = Join-Path $ProjectDir "about.hbs"

# GuiManifestPath points at the GUI crate manifest used as the dependency graph root.
# GuiManifestPath 指向作为依赖图根节点的 GUI crate 清单文件。
$GuiManifestPath = Join-Path $ProjectDir "apps\vulcan-code-gui\Cargo.toml"

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
    if (-not (Test-Path -LiteralPath $OutputLicenseResourcesRoot)) {
        New-Item -ItemType Directory -Path $OutputLicenseResourcesRoot -Force | Out-Null
    }

    $CargoAboutCommand = Get-Command "cargo-about" -ErrorAction SilentlyContinue
    if (-not $CargoAboutCommand) {
        throw 'Missing cargo-about. Install it first with `cargo install --locked --features cli cargo-about`.'
    }

    if (-not (Test-Path -LiteralPath $CargoAboutConfigFile)) {
        throw "Missing cargo-about config: $CargoAboutConfigFile"
    }
    if (-not (Test-Path -LiteralPath $CargoAboutTemplateFile)) {
        throw "Missing cargo-about template: $CargoAboutTemplateFile"
    }
    if (-not (Test-Path -LiteralPath $GuiManifestPath)) {
        throw "Missing GUI manifest: $GuiManifestPath"
    }

    & cargo about generate $CargoAboutTemplateFile --manifest-path $GuiManifestPath --config $CargoAboutConfigFile --output-file $OutputThirdPartyNoticesFile
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    $SectionIndex = Get-RustThirdPartyNoticeSectionIndex -MarkdownPath $OutputThirdPartyNoticesFile
    $SectionIndex | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutputThirdPartyNoticesIndexFile -Encoding UTF8

    Write-Host "==> Rust third-party notices generated to $OutputThirdPartyNoticesFile"
    Write-Host "==> Rust third-party notice index generated to $OutputThirdPartyNoticesIndexFile"
}

Invoke-GenerateRustThirdPartyNotices
