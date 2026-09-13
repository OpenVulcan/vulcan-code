# Test the shared indexer with synthetic notices and the checked-in authoritative document.
# 使用合成声明与签入权威文档验证共享索引器。
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "rust_third_party_notices_index.ps1")

# FixturePath is one isolated temporary file; no runtime output or user configuration is touched.
# FixturePath 是单个隔离临时文件，不接触运行时产物或用户配置。
$FixturePath = [IO.Path]::GetTempFileName()
try {
    # Fixture preserves two copyright variants under one title and an explicitly unknown title.
    # Fixture 保留同一标题下的两份版权变体，以及明确未知的标题。
    $Fixture = @'
# Notices
### MIT License
Used by:
- alpha 1.0.0 - https://example.invalid/alpha
```text
Copyright Alpha
MIT body A
```

### MIT License
Used by:
- beta 2.0.0 - https://example.invalid/beta
```text
Copyright Beta
MIT body B
```

### UNKNOWN_TITLE
Used by:
- unknown-package 3.0.0 - https://example.invalid/unknown
```text
Unidentified original notice
```
'@
    $Fixture = $Fixture.Replace("UNKNOWN_TITLE", "")
    [IO.File]::WriteAllText($FixturePath, $Fixture, [Text.UTF8Encoding]::new($false))
    # Index is derived by the exact same function used by make licenses.
    # Index 由 make licenses 使用的同一个函数生成。
    $Index = Get-RustThirdPartyNoticeSectionIndex -MarkdownPath $FixturePath
    if ($Index.schema_version -ne 2 -or $Index.licenses.Count -ne 2) { throw "Unexpected schema/groups" }
    if ($Index.licenses[0].sections.Count -ne 2) { throw "A same-title body variant was lost" }
    if ($Index.licenses[0].sections[1].dependencies[0].name -ne "beta") { throw "Incorrect body dependency association" }
    if ($Index.licenses[1].status -ne "unknown") { throw "Unknown license state was lost" }
    # SourceLines reconstructs every body using the exact server's one-based inclusive contract.
    # SourceLines 按服务端从一开始且包含两端的精确合同重建每份正文。
    $SourceLines = [IO.File]::ReadAllLines($FixturePath)
    $Second = $Index.licenses[0].sections[1]
    $SecondBody = $SourceLines[($Second.start_line - 1)..($Second.end_line - 1)] -join "`n"
    if ($SecondBody -ne "Copyright Beta`nMIT body B") { throw "Body line range is incorrect" }
    # Newline-only changes must not invalidate a semantically identical checked-out artifact.
    # 仅换行变化不得使语义相同的检出制品失效。
    [IO.File]::WriteAllText($FixturePath, $Fixture.Replace("`r`n", "`n").Replace("`n", "`r`n"), [Text.UTF8Encoding]::new($false))
    $CrLfIndex = Get-RustThirdPartyNoticeSectionIndex -MarkdownPath $FixturePath
    if ($CrLfIndex.source_sha256 -ne $Index.source_sha256) { throw "Checkout newline conversion changed the catalog revision" }
    [IO.File]::WriteAllText($FixturePath, $Fixture.Replace("- beta 2.0.0 -", "- beta-invalid-format"), [Text.UTF8Encoding]::new($false))
    # InvalidTemplateRejected prevents silent heuristic interpretation of a changed generator format.
    # InvalidTemplateRejected 防止对改变的生成器格式进行静默启发式解释。
    $InvalidTemplateRejected = $false
    try { Get-RustThirdPartyNoticeSectionIndex -MarkdownPath $FixturePath | Out-Null } catch { $InvalidTemplateRejected = $true }
    if (-not $InvalidTemplateRejected) { throw "Malformed template output was accepted" }

    # SourcePath checks production data without regenerating dependency notices.
    # SourcePath 验证生产数据，不重新生成依赖声明。
    $SourcePath = Join-Path (Split-Path $PSScriptRoot -Parent) "crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES.md"
    $ProductionIndex = Get-RustThirdPartyNoticeSectionIndex -MarkdownPath $SourcePath
    $SectionCount = ($ProductionIndex.licenses | ForEach-Object { $_.sections.Count } | Measure-Object -Sum).Sum
    $HeadingCount = @([IO.File]::ReadAllLines($SourcePath) | Where-Object { $_.StartsWith("### ") }).Count
    if ($SectionCount -ne $HeadingCount) { throw "Production index dropped a source notice" }
    # ExpectedJson verifies the actual checked-in index is exactly the shared generator's current result.
    # ExpectedJson 验证实际签入索引与当前共享生成器结果精确一致。
    $ExpectedJson = (($ProductionIndex | ConvertTo-Json -Depth 10) -replace "`r`n", "`n") + "`n"
    $ActualJson = [IO.File]::ReadAllText((Join-Path (Split-Path $SourcePath -Parent) "THIRD_PARTY_NOTICES_INDEX.json")).Replace("`r`n", "`n")
    if ($ActualJson -ne $ExpectedJson) { throw "Checked-in index differs from the shared generator result" }
    Write-Host "PASS: grouping, body variants, structured dependencies, unknown state, exact ranges, newline identity, malformed input, production $SectionCount bodies."
}
finally {
    Remove-Item -LiteralPath $FixturePath
}
