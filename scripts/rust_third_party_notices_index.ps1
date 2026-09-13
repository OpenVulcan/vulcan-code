# Get-RustThirdPartyNoticeSectionIndex parses only the repository-owned about.hbs format and preserves every body variant.
# Get-RustThirdPartyNoticeSectionIndex 仅解析仓库持有的 about.hbs 格式，并保留每份正文变体。
# MarkdownPath identifies the generated notice document; the result is a versioned, hash-bound read-only catalog.
# MarkdownPath 指向生成声明文档；返回带版本和哈希绑定的只读目录。
function Get-RustThirdPartyNoticeSectionIndex {
    param([Parameter(Mandatory)][string]$MarkdownPath)

    # Lines preserves one-based line coordinates used by the body reader.
    # Lines 保留正文读取器使用的从一开始的行坐标。
    $Lines = [System.IO.File]::ReadAllLines($MarkdownPath)
    # Groups merges equal declared titles without merging their copyright-specific bodies.
    # Groups 合并相同声明标题，但不合并版权内容不同的正文。
    $Groups = [System.Collections.Generic.List[object]]::new()
    # GroupsByTitle uses exact title identity rather than inferred SPDX mappings.
    # GroupsByTitle 使用精确标题身份，不推测 SPDX 对应关系。
    $GroupsByTitle = [System.Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
    # LineIndex advances beyond each complete fenced body.
    # LineIndex 每次推进到完整围栏正文之后。
    $LineIndex = 0
    while ($LineIndex -lt $Lines.Length) {
        if (-not $Lines[$LineIndex].StartsWith("### ")) { $LineIndex++; continue }
        # Title retains cargo-about's declared license name, including an explicitly empty unknown name.
        # Title 保留 cargo-about 声明的许可证名称，包括明确为空的未知名称。
        $Title = $Lines[$LineIndex].Substring(4).Trim()
        # Dependencies records the exact name/version/source fields emitted by about.hbs.
        # Dependencies 记录 about.hbs 输出的精确名称、版本与来源字段。
        $Dependencies = [System.Collections.Generic.List[object]]::new()
        $LineIndex++
        while ($LineIndex -lt $Lines.Length -and $Lines[$LineIndex].Trim() -ne 'Used by:') {
            if ($Lines[$LineIndex].StartsWith("### ")) { throw "Missing Used by section for license '$Title'." }
            $LineIndex++
        }
        $LineIndex++
        while ($LineIndex -lt $Lines.Length -and $Lines[$LineIndex].Trim() -ne '```text') {
            # DependencyLine is validated against the exact checked-in template rather than parsed heuristically by the UI.
            # DependencyLine 按签入模板的精确格式验证，界面不进行启发式解析。
            $DependencyLine = $Lines[$LineIndex].Trim()
            if ($DependencyLine.Length -gt 0) {
                if ($DependencyLine -notmatch '^- ([^\s]+) ([^\s]+) - (.+)$') {
                    throw "Invalid about.hbs dependency line $($LineIndex + 1): $DependencyLine"
                }
                $Dependencies.Add([pscustomobject]@{ name = $Matches[1]; version = $Matches[2]; source = $Matches[3] })
            }
            $LineIndex++
        }
        if ($LineIndex -ge $Lines.Length) { throw "Missing license body for '$Title'." }
        # StartLine and EndLine address the full original body without dropping copyright variants.
        # StartLine 与 EndLine 定位完整原始正文，不丢弃版权变体。
        $StartLine = $LineIndex + 2
        $LineIndex++
        while ($LineIndex -lt $Lines.Length -and $Lines[$LineIndex].Trim() -ne '```') { $LineIndex++ }
        if ($LineIndex -ge $Lines.Length) { throw "Unclosed license body for '$Title'." }
        $EndLine = $LineIndex
        if ($EndLine -lt $StartLine) { throw "Empty license body for '$Title'." }
        if (-not $GroupsByTitle.ContainsKey($Title)) {
            # Group stores one declared type and all of its independently readable sections.
            # Group 保存一个声明类型及其全部可独立读取的正文段。
            $Group = [pscustomobject]@{
                id = "rust-license-$($Groups.Count + 1)"
                title = $Title
                status = $(if ([string]::IsNullOrWhiteSpace($Title)) { "unknown" } else { "declared" })
                sections = [System.Collections.Generic.List[object]]::new()
            }
            $Groups.Add($Group)
            $GroupsByTitle.Add($Title, $Group)
        }
        # SelectedGroup is the unique owner of this declared title.
        # SelectedGroup 是此声明标题的唯一归属分组。
        $SelectedGroup = $GroupsByTitle[$Title]
        $SelectedGroup.sections.Add([pscustomobject]@{
            id = "$($SelectedGroup.id)-section-$($SelectedGroup.sections.Count + 1)"
            dependencies = @($Dependencies.ToArray())
            start_line = $StartLine
            end_line = $EndLine
        })
        $LineIndex++
    }
    if ($Groups.Count -eq 0) { throw "The notice document contains no license sections." }
    # NormalizedMarkdown makes the digest independent of Git checkout newline conversion.
    # NormalizedMarkdown 使摘要不受 Git 检出换行转换影响。
    $NormalizedMarkdown = [System.IO.File]::ReadAllText($MarkdownPath).Replace("`r`n", "`n").Replace("`r", "`n")
    # Digest binds the index to the complete UTF-8 document with LF newlines.
    # Digest 将索引绑定到使用 LF 换行的完整 UTF-8 文档。
    $Digest = [System.Security.Cryptography.SHA256]::Create()
    try { $SourceHash = [BitConverter]::ToString($Digest.ComputeHash([Text.Encoding]::UTF8.GetBytes($NormalizedMarkdown))).Replace("-", "").ToLowerInvariant() }
    finally { $Digest.Dispose() }
    return [pscustomobject]@{
        schema_version = 2
        scope = "rust-desktop-dependencies"
        source_file = [System.IO.Path]::GetFileName($MarkdownPath)
        source_sha256 = $SourceHash
        licenses = @($Groups.ToArray())
    }
}
