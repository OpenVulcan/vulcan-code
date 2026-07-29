<#
.SYNOPSIS
Performs a read-only CLIProxyAPI protocol compatibility difference audit.
执行只读的 CLIProxyAPI 协议兼容差异审计。

.DESCRIPTION
Compares a reviewed CLIProxyAPI baseline with the local checkout HEAD and writes a Markdown report.
比较已审核的 CLIProxyAPI 基线与本地检出 HEAD，并写入 Markdown 报告。

The script never fetches, checks out, resets, or modifies the reference repository.
脚本不会拉取、切换、重置或修改参考仓库。
#>
[CmdletBinding()]
param(
    # Local CLIProxyAPI checkout inspected without mutation.
    # 以只读方式检查的本地 CLIProxyAPI 检出目录。
    [Parameter()]
    [string]$RepositoryPath = "D:\openvulcan\third_git\CLIProxyAPI",

    # Last reviewed upstream commit used as the comparison base.
    # 用作比较基线的最近已审核上游提交。
    [Parameter()]
    [string]$BaselineCommit = "8423cce2d1004e80948a9e2c60ee69354c0aabc3",

    # Markdown report path owned by Vulcan Code.
    # 由 Vulcan Code 管理的 Markdown 报告路径。
    [Parameter()]
    [string]$ReportPath = "docs/protocol-compatibility/cliproxy-audit-latest.md"
)

$ErrorActionPreference = "Stop"

# Resolved reference checkout prevents ambiguous relative-path inspection.
# 解析后的参考检出目录避免含糊的相对路径检查。
$resolvedRepositoryPath = (Resolve-Path -LiteralPath $RepositoryPath).Path

# Current reference HEAD identifies the exact upper comparison boundary.
# 当前参考仓库 HEAD 标识精确的比较上界。
$headCommit = (& git -C $resolvedRepositoryPath rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($headCommit)) {
    throw "CLIProxyAPI HEAD could not be resolved from '$resolvedRepositoryPath'."
}

# Baseline verification rejects stale or mistyped commit identifiers before diffing.
# 基线校验在比较前拒绝失效或拼写错误的提交标识。
& git -C $resolvedRepositoryPath cat-file -e "$BaselineCommit^{commit}"
if ($LASTEXITCODE -ne 0) {
    throw "CLIProxyAPI baseline '$BaselineCommit' is not available in '$resolvedRepositoryPath'."
}

# Working-tree status is evidence only and is never altered by this audit.
# 工作树状态仅作为证据，审计不会修改它。
$workingTreeStatus = @(& git -C $resolvedRepositoryPath status --short)
if ($LASTEXITCODE -ne 0) {
    throw "CLIProxyAPI working-tree status could not be inspected."
}

# Changed files are collected with rename-aware status labels.
# 变更文件使用支持重命名的状态标签收集。
$changedFiles = @(& git -C $resolvedRepositoryPath diff --name-status --find-renames "$BaselineCommit..$headCommit")
if ($LASTEXITCODE -ne 0) {
    throw "CLIProxyAPI changes could not be compared from '$BaselineCommit' to '$headCommit'."
}

# Protocol-relevant paths form a reviewed, closed audit boundary.
# 协议相关路径构成经过审核的封闭审计边界。
$protocolPathPattern = '^(internal/translator/|sdk/translator/|test/|sdk/api/handlers/|internal/runtime/executor/|sdk/cliproxy/usage/)'

# Relevant changes exclude unrelated server, packaging, and presentation files.
# 相关变更排除无关的服务、打包与展示文件。
$relevantChanges = @($changedFiles | Where-Object {
    # Changed path is the final tab-delimited field, including rename targets.
    # 变更路径是制表符分隔记录的最后字段，包含重命名目标。
    $changedPath = ($_ -split "`t")[-1].Replace('\', '/')
    $changedPath -match $protocolPathPattern
})

# Category rules convert paths into review queues without inferring runtime compatibility.
# 分类规则将路径转换为审核队列，但不推测运行时兼容结论。
$categoryRules = [ordered]@{
    "协议翻译器" = '^(internal/translator/|sdk/translator/)'
    "流式与错误处理" = '^sdk/api/handlers/'
    "用量与缓存统计" = '^(sdk/cliproxy/usage/|test/usage_)'
    "思考与签名" = '(thinking|signature|reasoning)'
    "内置工具" = '(builtin_tools|web_search|code_interpreter|image_generation)'
    "行为测试夹具" = '^test/'
}

# Report lines are assembled deterministically for review-friendly diffs.
# 报告行以确定顺序组装，便于审核差异。
$reportLines = [System.Collections.Generic.List[string]]::new()
$reportLines.Add("# CLIProxyAPI 协议兼容差异审计")
$reportLines.Add("")
$reportLines.Add("- 审计模式：只读本地比较")
$reportLines.Add("- 参考仓库：``$resolvedRepositoryPath``")
$reportLines.Add("- 已审核基线：``$BaselineCommit``")
$reportLines.Add("- 当前 HEAD：``$headCommit``")
$reportLines.Add("- 协议相关变更数：$($relevantChanges.Count)")
$reportLines.Add("- 参考仓库未提交变更数：$($workingTreeStatus.Count)")
$reportLines.Add("")
$reportLines.Add("## 结论")
$reportLines.Add("")
if ($BaselineCommit -eq $headCommit -and $workingTreeStatus.Count -eq 0) {
    $reportLines.Add("当前参考仓库与已审核基线一致，没有需要转化为 Vulcan Code 测试夹具的新差异。")
} elseif ($relevantChanges.Count -eq 0) {
    $reportLines.Add("参考仓库 HEAD 已变化，但封闭协议审计路径内没有检测到变更。仍需人工确认路径边界是否需要扩展。")
} else {
    $reportLines.Add("检测到协议相关差异。下列条目只表示需要人工审核，不代表 Vulcan Code 已缺失或应直接复制实现。")
}
$reportLines.Add("")
$reportLines.Add("## 协议相关文件")
$reportLines.Add("")
if ($relevantChanges.Count -eq 0) {
    $reportLines.Add("- 无")
} else {
    foreach ($change in $relevantChanges) {
        $reportLines.Add("- ``$change``")
    }
}
$reportLines.Add("")
$reportLines.Add("## 分类审核队列")
$reportLines.Add("")
foreach ($category in $categoryRules.Keys) {
    # Category matches remain path- and filename-based evidence only.
    # 分类匹配仅保留基于路径和文件名的证据。
    $matches = @($relevantChanges | Where-Object {
        $changedPath = ($_ -split "`t")[-1].Replace('\', '/')
        $changedPath -match $categoryRules[$category]
    })
    $reportLines.Add("### $category")
    $reportLines.Add("")
    if ($matches.Count -eq 0) {
        $reportLines.Add("- 无")
    } else {
        foreach ($match in $matches) {
            $reportLines.Add("- ``$match``")
        }
    }
    $reportLines.Add("")
}
$reportLines.Add("## 参考仓库工作树")
$reportLines.Add("")
if ($workingTreeStatus.Count -eq 0) {
    $reportLines.Add("- 干净")
} else {
    $reportLines.Add("参考仓库存在未提交内容；这些内容不会进入基线差异，但必须在人工审核时单独处理：")
    foreach ($status in $workingTreeStatus) {
        $reportLines.Add("- ``$status``")
    }
}
$reportLines.Add("")
$reportLines.Add("## 人工处理规则")
$reportLines.Add("")
$reportLines.Add("1. 先阅读变更对应的上游测试、请求转换和响应转换，确认真实协议事实。")
$reportLines.Add("2. 将确认的边界转化为 Vulcan Code 自有 Rust 夹具；不得复制 Go 运行时结构。")
$reportLines.Add("3. 仅在公共契约无法表达事实时扩展封闭类型；不得加入任意 JSON Path 或猜测性回退。")
$reportLines.Add("4. 通过协议包、适配器、运行时和 WebUI 分层验证后，才更新本脚本的已审核基线。")

# Absolute report path is resolved from the Vulcan Code working directory when needed.
# 绝对报告路径在需要时从 Vulcan Code 工作目录解析。
$resolvedReportPath = if ([System.IO.Path]::IsPathRooted($ReportPath)) {
    $ReportPath
} else {
    Join-Path (Get-Location).Path $ReportPath
}

# Report directory is created only in the caller-owned Vulcan Code tree.
# 报告目录仅在调用方拥有的 Vulcan Code 工作树中创建。
$reportDirectory = Split-Path -Parent $resolvedReportPath
New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
[System.IO.File]::WriteAllLines(
    $resolvedReportPath,
    $reportLines,
    [System.Text.UTF8Encoding]::new($false)
)

Write-Output "CLIProxyAPI protocol audit report: $resolvedReportPath"
