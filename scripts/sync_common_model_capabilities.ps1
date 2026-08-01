param(
    [string]$SourceUrl = "https://openrouter.ai/api/v1/models?output_modalities=all",
    [string]$OutputPath = "crates/vulcan-provider-adapters/resources/common_model_capabilities.json",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

# Normalize one public model identifier into the runtime comparison key.
# 将一个公共模型标识规范化为运行时比较键。 (CN)
function ConvertTo-NormalizedModelKey {
    param([AllowEmptyString()][string]$Value)

    return $Value.Trim().ToLowerInvariant()
}

# Convert one OpenRouter modality into the closed Vulcan modality vocabulary.
# 将一个 OpenRouter 模态转换为封闭的 Vulcan 模态词表。 (CN)
function ConvertTo-VulcanModality {
    param([AllowEmptyString()][string]$Value)

    switch ((ConvertTo-NormalizedModelKey $Value)) {
        "text" { return "text" }
        "image" { return "image" }
        "audio" { return "audio" }
        "video" { return "video" }
        "file" { return "file" }
        default { return $null }
    }
}

# Build a deterministic offline capability catalog from the public source response.
# 从公共来源响应构建确定性的离线能力目录。 (CN)
function New-CommonModelCapabilityCatalog {
    param([Parameter(Mandatory = $true)][object]$Response)

    if ($null -eq $Response.data -or $Response.data -isnot [System.Collections.IEnumerable]) {
        throw "OpenRouter model response does not contain a data array."
    }

    $records = [System.Collections.Generic.List[object]]::new()
    $aliasOwners = @{}
    $canonicalKeys = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($model in $Response.data) {
        $key = ConvertTo-NormalizedModelKey ([string]$model.id)
        if ([string]::IsNullOrWhiteSpace($key)) {
            continue
        }
        if (-not $canonicalKeys.Add($key)) {
            throw "OpenRouter model response contains duplicate model identifier '$key'."
        }
        $reportedInputs = @($model.architecture.input_modalities)
        $reportedOutputs = @($model.architecture.output_modalities)
        $inputs = @($reportedInputs | ForEach-Object { ConvertTo-VulcanModality ([string]$_) } | Where-Object { $null -ne $_ } | Sort-Object -Unique)
        $outputs = @($reportedOutputs | ForEach-Object { ConvertTo-VulcanModality ([string]$_) } | Where-Object { $null -ne $_ } | Sort-Object -Unique)
        # Missing modality metadata receives the documented conservative fallback; an explicit
        # unsupported-only modality list remains empty so it cannot be mislabeled as text.
        # 缺失模态元数据时使用文档化的保守回退；明确的仅非支持模态列表保持为空，避免被误标为文本。 (CN)
        if ($reportedInputs.Count -eq 0) { $inputs = @("text") }
        if ($reportedOutputs.Count -eq 0) { $outputs = @("text") }

        $candidateAliases = [System.Collections.Generic.List[string]]::new()
        foreach ($candidate in @($model.canonical_slug, $model.name, ($key -split "/")[-1])) {
            $alias = ConvertTo-NormalizedModelKey ([string]$candidate)
            if ([string]::IsNullOrWhiteSpace($alias) -or $alias -eq $key) { continue }
            $candidateAliases.Add($alias)
            if (-not $aliasOwners.ContainsKey($alias)) { $aliasOwners[$alias] = [System.Collections.Generic.HashSet[string]]::new() }
            [void]$aliasOwners[$alias].Add($key)
        }

        $records.Add([pscustomobject]@{
            key = $key
            aliases = @($candidateAliases | Sort-Object -Unique)
            context_window = if ($null -ne $model.context_length -and [long]$model.context_length -gt 0) { [long]$model.context_length } else { 200000 }
            input_modalities = $inputs
            output_modalities = $outputs
        })
    }

    $models = foreach ($record in ($records | Sort-Object key)) {
        $uniqueAliases = @(
            $record.aliases |
                Where-Object { $aliasOwners[$_].Count -eq 1 -and -not $canonicalKeys.Contains($_) } |
                Sort-Object -Unique
        )
        [ordered]@{
            key = $record.key
            aliases = $uniqueAliases
            context_window = $record.context_window
            input_modalities = $record.input_modalities
            output_modalities = $record.output_modalities
        }
    }

    return [ordered]@{
        schema_version = 1
        sources = @("openrouter:/api/v1/models?output_modalities=all", "vulcan:provider-specific-capabilities")
        fallback = [ordered]@{
            context_window = 200000
            input_modalities = @("text")
            output_modalities = @("text")
        }
        models = @($models)
    }
}

$response = Invoke-RestMethod -Method Get -Uri $SourceUrl -Headers @{ "User-Agent" = "VulcanCode-CapabilityCatalog/1.0" }
$catalog = New-CommonModelCapabilityCatalog -Response $response
if ($DryRun) {
    Write-Output ("Validated {0} common model capability records." -f $catalog.models.Count)
    return
}

$resolvedOutputPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    [System.IO.Path]::GetFullPath($OutputPath)
} else {
    [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputPath))
}
[System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($resolvedOutputPath)) | Out-Null
$temporaryPath = "$resolvedOutputPath.tmp"
$json = $catalog | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText(
    $temporaryPath,
    $json + [System.Environment]::NewLine,
    [System.Text.UTF8Encoding]::new($false)
)
Move-Item -LiteralPath $temporaryPath -Destination $resolvedOutputPath -Force
Write-Output ("Wrote {0} common model capability records to {1}." -f $catalog.models.Count, $resolvedOutputPath)
