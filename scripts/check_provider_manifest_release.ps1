param(
    # Source manifest directory that will be packaged and indexed.
    # 将被打包并生成 Index 的源 Manifest 目录。
    [string]$ManifestDirectory,

    # Optional previous release directory used for semantic diff output.
    # 用于语义差异输出的可选上一发布版本目录。
    [string]$PreviousManifestDirectory,

    # Optional retained artifact directory; omission uses and removes an isolated temporary directory.
    # 可选的保留工件目录；省略时使用并删除隔离临时目录。
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'

# Repository root resolved from this checked-in script.
# 从本仓库脚本解析出的仓库根目录。
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

# Default authoritative source manifest directory.
# 默认权威源 Manifest 目录。
$defaultManifestDirectory = Join-Path $repositoryRoot 'crates/vulcan-runtime-assets/resources/provider-manifests/manifests'

# Exact manifest directory selected by the caller or the repository default.
# 调用方选择或仓库默认的精确 Manifest 目录。
$manifestRoot = if ([string]::IsNullOrWhiteSpace($ManifestDirectory)) {
    $defaultManifestDirectory
} else {
    [System.IO.Path]::GetFullPath($ManifestDirectory)
}

if (-not (Test-Path -LiteralPath $manifestRoot -PathType Container)) {
    throw "Manifest directory does not exist: $manifestRoot"
}

# Whether the artifact directory is temporary and owned by this invocation.
# 工件目录是否为本次调用拥有的临时目录。
$ownsArtifactDirectory = [string]::IsNullOrWhiteSpace($OutputDirectory)

# Exact artifact directory for index, signature, and public key outputs.
# Index、签名与公钥输出使用的精确工件目录。
$artifactRoot = if ($ownsArtifactDirectory) {
    Join-Path ([System.IO.Path]::GetTempPath()) ("vulcan-provider-release-" + [System.Guid]::NewGuid().ToString('N'))
} else {
    [System.IO.Path]::GetFullPath($OutputDirectory)
}

[System.IO.Directory]::CreateDirectory($artifactRoot) | Out-Null

# Ephemeral private seed path outside every retained release artifact.
# 位于全部保留发布工件之外的临时私钥种子路径。
$privateSeedPath = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcan-provider-signing-seed-" + [System.Guid]::NewGuid().ToString('N') + '.txt')

# Canonical generated release artifact paths.
# 规范生成的发布工件路径。
$indexPath = Join-Path $artifactRoot 'provider-index-v1.json'
$signaturePath = Join-Path $artifactRoot 'provider-index-v1.sig'
$publicKeyPath = Join-Path $artifactRoot 'provider-index-v1.pub'

# Runs one RTK-prefixed command and fails at its exact non-zero exit code.
# 运行一条带 RTK 前缀的命令，并在其精确非零退出码处失败。
function Invoke-RtkChecked {
    param(
        # Argument vector passed directly to RTK without shell reconstruction.
        # 不经 Shell 重组而直接传给 RTK 的参数向量。
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    & rtk @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "RTK command failed with exit code ${LASTEXITCODE}: rtk $($Arguments -join ' ')"
    }
}

# Computes one lowercase SHA-256 digest through APIs shared by Windows PowerShell and PowerShell 7.
# 通过 Windows PowerShell 与 PowerShell 7 共有的 API 计算一个小写 SHA-256 摘要。
function Get-Sha256Hex {
    param(
        # Exact file path whose bytes are hashed.
        # 需要计算字节摘要的精确文件路径。
        [Parameter(Mandatory = $true)]
        [string]$LiteralPath
    )

    # The stream and algorithm are disposed deterministically so repeated release checks do not
    # retain file locks or unmanaged cryptographic resources.
    # 确定性释放流与算法，避免重复发布检查保留文件锁或非托管密码学资源。
    $stream = [System.IO.File]::OpenRead($LiteralPath)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $digest = $sha256.ComputeHash($stream)
        return [System.BitConverter]::ToString($digest).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

# Historical provider-named adapter files that must never return after generic migration.
# 通用化迁移完成后绝不能恢复的历史供应商命名 adapter 文件。
$forbiddenProviderAdapterFiles = @(
    'crates/vulcan-provider-adapters/src/alibaba_provider_tools.rs',
    'crates/vulcan-provider-adapters/src/minimax_provider_tools.rs',
    'crates/vulcan-provider-adapters/src/tavily_provider_tools.rs',
    'crates/vulcan-provider-adapters/src/tool_catalog.rs'
)

foreach ($relativePath in $forbiddenProviderAdapterFiles) {
    # Exact source path constrained below the repository root.
    # 约束在仓库根目录下的精确源码路径。
    $forbiddenPath = Join-Path $repositoryRoot $relativePath
    if (Test-Path -LiteralPath $forbiddenPath) {
        throw "Provider-named or dual-authority adapter source still exists: $relativePath"
    }
}

# Production provider-tool dispatch units that must remain supplier-neutral.
# 必须保持供应商无关的生产 Provider Tool 分发单元。
$providerToolDispatchFiles = @(
    'crates/vulcan-provider-adapters/src/provider_tool_execution.rs',
    'crates/vulcan-agent-runtime/src/provider_tool_bridge.rs'
)

foreach ($relativePath in $providerToolDispatchFiles) {
    # Exact production source text before any co-located Rust test module.
    # 同文件 Rust 测试模块之前的精确生产源码文本。
    $sourcePath = Join-Path $repositoryRoot $relativePath
    $sourceText = [System.IO.File]::ReadAllText($sourcePath)
    $testBoundary = $sourceText.IndexOf('#[cfg(test)]', [System.StringComparison]::Ordinal)
    $productionText = if ($testBoundary -ge 0) {
        $sourceText.Substring(0, $testBoundary)
    } else {
        $sourceText
    }
    if ([System.Text.RegularExpressions.Regex]::IsMatch(
        $productionText,
        '\b(alibaba|minimax|tavily)\b',
        [System.Text.RegularExpressions.RegexOptions]::IgnoreCase
    )) {
        throw "Supplier-specific production Provider Tool dispatch remains in: $relativePath"
    }
}

try {
    Invoke-RtkChecked -Arguments @('cargo', 'run', '-q', '-p', 'vulcan-provider-manifest-cli', '--', 'validate-all')
    Invoke-RtkChecked -Arguments @('cargo', 'run', '-q', '-p', 'vulcan-provider-manifest-cli', '--', 'validate', $manifestRoot)
    Invoke-RtkChecked -Arguments @('cargo', 'run', '-q', '-p', 'vulcan-provider-manifest-cli', '--', 'fixture-check')

    if (-not [string]::IsNullOrWhiteSpace($PreviousManifestDirectory)) {
        # Exact previous release directory used only for read-only semantic comparison.
        # 仅用于只读语义比较的精确上一发布版本目录。
        $previousRoot = [System.IO.Path]::GetFullPath($PreviousManifestDirectory)
        if (-not (Test-Path -LiteralPath $previousRoot -PathType Container)) {
            throw "Previous manifest directory does not exist: $previousRoot"
        }
        foreach ($currentFile in Get-ChildItem -LiteralPath $manifestRoot -Filter '*.json' -File | Sort-Object Name) {
            # Same-provider previous manifest candidate.
            # 同供应商上一版本 Manifest 候选。
            $previousFile = Join-Path $previousRoot $currentFile.Name
            if (Test-Path -LiteralPath $previousFile -PathType Leaf) {
                Invoke-RtkChecked -Arguments @('cargo', 'run', '-q', '-p', 'vulcan-provider-manifest-cli', '--', 'diff', $previousFile, $currentFile.FullName)
            } else {
                Write-Host "semantic diff: added provider manifest $($currentFile.Name)"
            }
        }
        foreach ($previousFile in Get-ChildItem -LiteralPath $previousRoot -Filter '*.json' -File | Sort-Object Name) {
            # Same-provider current manifest candidate used to report removals.
            # 用于报告移除项的同供应商当前 Manifest 候选。
            $currentFile = Join-Path $manifestRoot $previousFile.Name
            if (-not (Test-Path -LiteralPath $currentFile -PathType Leaf)) {
                Write-Host "semantic diff: removed provider manifest $($previousFile.Name)"
            }
        }
    } else {
        Write-Host 'semantic diff: skipped because no previous release directory was supplied'
    }

    Invoke-RtkChecked -Arguments @(
        'cargo', 'run', '-q', '-p', 'vulcan-provider-manifest-cli', '--',
        'build-index', $manifestRoot, '--revision', '1', '--key-id', 'release-check-key', '--out', $indexPath
    )

    # Cryptographically random temporary Ed25519 seed used only by this local gate.
    # 仅由本地门禁使用的密码学随机临时 Ed25519 种子。
    $seed = [byte[]]::new(32)
    # The instance API keeps the gate compatible with both Windows PowerShell and PowerShell 7.
    # 实例 API 使门禁同时兼容 Windows PowerShell 与 PowerShell 7。
    $randomNumberGenerator = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $randomNumberGenerator.GetBytes($seed)
    } finally {
        $randomNumberGenerator.Dispose()
    }

    # Lowercase seed encoding accepted by the CLI without storing a repository secret.
    # CLI 接受且不会形成仓库秘密的私钥种子小写编码。
    $seedHex = [System.BitConverter]::ToString($seed).Replace('-', '').ToLowerInvariant()
    [System.IO.File]::WriteAllText(
        $privateSeedPath,
        $seedHex,
        [System.Text.UTF8Encoding]::new($false)
    )

    Invoke-RtkChecked -Arguments @(
        'cargo', 'run', '-q', '-p', 'vulcan-provider-manifest-cli', '--',
        'sign-index', $indexPath, '--key-file', $privateSeedPath, '--out', $signaturePath,
        '--public-key-out', $publicKeyPath
    )

    # Detached signature generated from the exact index bytes.
    # 从精确 Index 字节生成的分离签名。
    $signatureHex = [System.IO.File]::ReadAllText($signaturePath).Trim()

    # Derived public key paired with the ephemeral signing seed.
    # 与临时签名种子配对的派生公钥。
    $publicKeyHex = [System.IO.File]::ReadAllText($publicKeyPath).Trim()

    Invoke-RtkChecked -Arguments @(
        'cargo', 'run', '-q', '-p', 'vulcan-provider-manifest-cli', '--',
        'verify-index', $indexPath, '--signature', $signatureHex, '--public-key', $publicKeyHex,
        '--key-id', 'release-check-key'
    )

    # Parsed generated index used for digest and file-set consistency checks.
    # 用于摘要与文件集合一致性检查的已解析生成 Index。
    $index = [System.IO.File]::ReadAllText($indexPath) | ConvertFrom-Json

    # Exact current manifest files keyed by file stem.
    # 按文件主名索引的精确当前 Manifest 文件。
    $manifestFiles = @{}
    foreach ($file in Get-ChildItem -LiteralPath $manifestRoot -Filter '*.json' -File) {
        $manifestFiles[$file.BaseName] = $file
    }

    # Exact index provider identifiers.
    # 精确 Index 供应商标识集合。
    $indexedProviderIds = @($index.providers.PSObject.Properties.Name | Sort-Object)
    if ($indexedProviderIds.Count -ne $manifestFiles.Count) {
        throw "Index provider count $($indexedProviderIds.Count) does not match manifest count $($manifestFiles.Count)."
    }
    foreach ($providerId in $indexedProviderIds) {
        if (-not $manifestFiles.ContainsKey($providerId)) {
            throw "Index references missing manifest provider: $providerId"
        }

        # Generated index entry pinned to one exact manifest file.
        # 固定到一份精确 Manifest 文件的生成 Index 条目。
        $entry = $index.providers.$providerId

        # Exact manifest file associated with the index provider id.
        # 与 Index 供应商标识关联的精确 Manifest 文件。
        $file = $manifestFiles[$providerId]

        # Lowercase SHA-256 computed independently by PowerShell.
        # 由 PowerShell 独立计算的小写 SHA-256。
        $actualDigest = Get-Sha256Hex -LiteralPath $file.FullName
        if ($entry.sha256 -ne $actualDigest) {
            throw "Digest mismatch for provider $providerId."
        }
        if ([int64]$entry.bytes -ne [int64]$file.Length) {
            throw "Byte-length mismatch for provider $providerId."
        }
        if ($entry.relative_url -ne "/$providerId.json") {
            throw "Relative URL mismatch for provider $providerId."
        }
    }

    Write-Host "provider manifest release gate passed: $($indexedProviderIds.Count) providers"
    if (-not $ownsArtifactDirectory) {
        Write-Host "release artifacts retained at $artifactRoot"
    }
} finally {
    if (Test-Path -LiteralPath $privateSeedPath -PathType Leaf) {
        Remove-Item -LiteralPath $privateSeedPath -Force
    }
    if ($ownsArtifactDirectory -and (Test-Path -LiteralPath $artifactRoot -PathType Container)) {
        # Resolve and constrain the recursive cleanup to the generated operating-system temp root.
        # 解析并把递归清理限制在生成的操作系统临时根目录内。
        $resolvedArtifactRoot = [System.IO.Path]::GetFullPath($artifactRoot)

        # Canonical operating-system temporary root used as the cleanup trust boundary.
        # 作为清理信任边界的规范操作系统临时根目录。
        $resolvedTempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
        if (-not $resolvedArtifactRoot.StartsWith($resolvedTempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove artifact directory outside the temporary root: $resolvedArtifactRoot"
        }
        Remove-Item -LiteralPath $resolvedArtifactRoot -Recurse -Force
    }
}
