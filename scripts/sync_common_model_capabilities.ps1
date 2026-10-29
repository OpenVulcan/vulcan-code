param(
    [string]$SourceUrl = "https://openrouter.ai/api/v1/models?output_modalities=all",
    [string]$OutputPath = "crates/vulcan-provider-adapters/resources/common_model_capabilities.json",
    [string]$ProviderManifestsPath = "crates/vulcan-runtime-assets/resources/provider-manifests/manifests",
    [switch]$SkipRemoteSync,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

# Normalize one public model identifier into the runtime comparison key.
# 将一个公共模型标识规范化为运行时比较键。 (CN)
function ConvertTo-NormalizedModelKey {
    param([AllowEmptyString()][string]$Value)

    return $Value.Trim().ToLowerInvariant()
}

# Convert one public or manifest modality into the closed Vulcan modality vocabulary.
# 将一个公共来源或 Manifest 模态转换为封闭的 Vulcan 模态词表。 (CN)
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

# Sorts, deduplicates, and drops empty values from one modality list.
# 对一组模态排序、去重并剔除空值。 (CN)
function Format-ModalityList {
    param([object[]]$Values)

    return @($Values | Where-Object { $_ } | Sort-Object -Unique)
}

# Reads one capability by field presence, including an explicitly empty list or null.
# 按字段存在性读取能力，保留显式空列表和 null。
# Parameters identify the default object, optional rule object, and exact field name.
# 参数指定默认对象、可选规则对象与精确字段名。
# Returns the declared field value or null when neither declaration owns that field.
# 返回已声明字段值；两层都未持有该字段时返回 null。
function Get-DeclaredCapabilityValue {
    param(
        [AllowNull()][object]$Defaults,
        [AllowNull()][object]$Specific,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -ne $Specific -and $null -ne $Specific.PSObject.Properties[$Name]) {
        return $Specific.PSObject.Properties[$Name].Value
    }
    if ($null -ne $Defaults -and $null -ne $Defaults.PSObject.Properties[$Name]) {
        return $Defaults.PSObject.Properties[$Name].Value
    }
    return $null
}

# Builds one common record from exact default and rule declarations.
# 从精确默认值与规则声明构建一条公共记录。
# Parameters identify the canonical key, owning manifest, and optional matched rule.
# 参数指定规范键、所属清单与可选匹配规则。
# Returns declared limits, reasoning settings, and operation-derived modalities.
# 返回已声明上限、思考设置与根据操作派生的模态。
function New-ProviderCapabilityRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Key,
        [Parameter(Mandatory = $true)][object]$Manifest,
        [AllowNull()][object]$Rule
    )

    $defaults = $Manifest.model_defaults.capabilities
    $specific = if ($null -ne $Rule) { $Rule.capabilities } else { $null }
    $operations = @(Get-DeclaredCapabilityValue $defaults $specific "operations")
    $declaredInputs = @(Get-DeclaredCapabilityValue $defaults $specific "input_resources")
    $declaredOutputs = @(Get-DeclaredCapabilityValue $defaults $specific "output_resources")
    $contextWindow = Get-DeclaredCapabilityValue $defaults $specific "context_limit"
    if ($null -ne $contextWindow -and [long]$contextWindow -le 0) { $contextWindow = $null }
    $inputs = Format-ModalityList (@($(if ($operations.Count -gt 0) { "text" })) + @($declaredInputs | ForEach-Object { ConvertTo-VulcanModality ([string]$_) }))
    $outputs = Format-ModalityList (@($(if ($operations -contains "conversation_respond") { "text" })) + @($declaredOutputs | ForEach-Object { ConvertTo-VulcanModality ([string]$_) }))
    $tail = ($Key -split "/")[-1]
    return [pscustomobject]@{
        key = $Key
        aliases = if ($tail -ne $Key) { @($tail) } else { @() }
        context_window = $contextWindow
        output_limit = Get-DeclaredCapabilityValue $defaults $specific "output_limit"
        reasoning = Get-DeclaredCapabilityValue $defaults $specific "reasoning"
        reasoning_control = Get-DeclaredCapabilityValue $defaults $specific "reasoning_control"
        reasoning_efforts = @(Get-DeclaredCapabilityValue $defaults $specific "reasoning_efforts")
        input_modalities = $inputs
        output_modalities = $outputs
    }
}

# Builds provider-owned records from every bundled manifest model rule.
# 从全部内置清单的模型规则构建供应商自有记录。 (CN)
#
# Bundled manifests declare only non-text resources; effective operations determine implicit text.
# 内置清单只声明非文本资源；生效 operation 决定隐含的文本模态。
function Get-ProviderConfiguredModelRecords {
    param([Parameter(Mandatory = $true)][string]$Directory)

    $records = [System.Collections.Generic.List[object]]::new()
    $keys = [System.Collections.Generic.HashSet[string]]::new()
    # Official providers own shared model facts before relays with the same model identifier.
    # 官方供应商优先于同模型标识的中转供应商持有公共模型事实。
    $manifestSources = foreach ($file in (Get-ChildItem -Path $Directory -Filter "*.json" -File)) {
        # Read as UTF-8 explicitly: a Windows PowerShell 5.1 host would otherwise decode a
        # BOM-less manifest with the ANSI code page and corrupt non-ASCII display names.
        # 显式以 UTF-8 读取：Windows PowerShell 5.1 主机否则会用 ANSI 代码页解码无 BOM 清单，
        # 破坏其中的非 ASCII 显示名。 (CN)
        $manifest = [System.IO.File]::ReadAllText($file.FullName, [System.Text.UTF8Encoding]::new($false)) | ConvertFrom-Json
        [pscustomobject]@{
            FileName = $file.Name
            Authority = if ($manifest.display.category -eq "official") { 0 } else { 1 }
            Manifest = $manifest
        }
    }
    foreach ($source in ($manifestSources | Sort-Object Authority, FileName)) {
        # The source object retains the already decoded authoritative manifest.
        # 来源对象保留已解码的权威清单。
        $manifest = $source.Manifest
        foreach ($rule in $manifest.model_rules) {
            # Only exact matchers name a model identity; prefix matchers are family selectors.
            # 只有 exact 匹配器才指向一个模型身份；prefix 匹配器是族选择器。 (CN)
            if ([string]$rule.matcher.kind -ne "exact") { continue }
            $key = ConvertTo-NormalizedModelKey ([string]$rule.matcher.value)
            if ([string]::IsNullOrWhiteSpace($key)) { continue }
            if (-not $keys.Add($key)) { continue }
            $records.Add((New-ProviderCapabilityRecord -Key $key -Manifest $manifest -Rule $rule))
        }

        # Catalog identities covered by a family rule inherit the same effective capability fields.
        # 族规则覆盖的目录身份继承相同的生效能力字段。
        foreach ($service in $manifest.services) {
            $catalog = $service.model_catalog
            if ($null -eq $catalog) { continue }
            foreach ($model in $catalog.models) {
                $key = ConvertTo-NormalizedModelKey ([string]$model.model_id)
                if ([string]::IsNullOrWhiteSpace($key)) { continue }
                if (-not $keys.Add($key)) { continue }
                $matchingRule = @($manifest.model_rules | Where-Object {
                    ($_.matcher.kind -eq "exact" -and (ConvertTo-NormalizedModelKey ([string]$_.matcher.value)) -eq $key) -or
                    ($_.matcher.kind -eq "prefix" -and $key.StartsWith((ConvertTo-NormalizedModelKey ([string]$_.matcher.value))))
                } | Sort-Object @{ Expression = { if ($_.matcher.kind -eq "exact") { 0 } else { 1 } } }, @{ Expression = { -([string]$_.matcher.value).Length } } | Select-Object -First 1)
                $records.Add((New-ProviderCapabilityRecord -Key $key -Manifest $manifest -Rule $matchingRule[0]))
            }
        }
    }

    return @($records)
}

# Builds records from the public upstream source response.
# 从公共上游来源响应构建记录。 (CN)
function Get-RemoteModelRecords {
    param([Parameter(Mandatory = $true)][object]$Response)

    if ($null -eq $Response.data -or $Response.data -isnot [System.Collections.IEnumerable]) {
        throw "OpenRouter model response does not contain a data array."
    }

    $records = [System.Collections.Generic.List[object]]::new()
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
        $inputs = Format-ModalityList (@($reportedInputs | ForEach-Object { ConvertTo-VulcanModality ([string]$_) }))
        $outputs = Format-ModalityList (@($reportedOutputs | ForEach-Object { ConvertTo-VulcanModality ([string]$_) }))
        # Missing upstream modality metadata remains unknown; only signed manifests supply text defaults.
        # 上游模态元数据缺失时维持未知；只有受签名清单提供文本默认值。

        $candidateAliases = [System.Collections.Generic.List[string]]::new()
        foreach ($candidate in @($model.canonical_slug, $model.name, ($key -split "/")[-1])) {
            $alias = ConvertTo-NormalizedModelKey ([string]$candidate)
            if ([string]::IsNullOrWhiteSpace($alias) -or $alias -eq $key) { continue }
            $candidateAliases.Add($alias)
        }

        $records.Add([pscustomobject]@{
            key = $key
            aliases = @($candidateAliases | Sort-Object -Unique)
            context_window = if ($null -ne $model.context_length -and [long]$model.context_length -gt 0) { [long]$model.context_length } else { $null }
            input_modalities = $inputs
            output_modalities = $outputs
        })
    }

    return @($records)
}

# Reads the records already present in a published catalog so a sync never drops them.
# 读取已发布目录中现有的记录，使同步永不丢弃它们。 (CN)
function Get-ExistingModelRecords {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return @()
    }
    $document = [System.IO.File]::ReadAllText($Path, [System.Text.UTF8Encoding]::new($false)) | ConvertFrom-Json
    return @($document.models | ForEach-Object {
        [pscustomobject]@{
            key = [string]$_.key
            aliases = @($_.aliases)
            context_window = if ($null -ne $_.context_window) { [long]$_.context_window } else { $null }
            output_limit = $_.output_limit
            reasoning = $_.reasoning
            reasoning_control = $_.reasoning_control
            reasoning_efforts = @($_.reasoning_efforts)
            input_modalities = @($_.input_modalities)
            output_modalities = @($_.output_modalities)
        }
    })
}

# Cross references provider records with the upstream source and returns the merged inventory.
# 用上游来源交叉参照供应商记录，并返回合并后的清单。 (CN)
#
# Provider declarations stay authoritative. The upstream source only fills fields a manifest
# leaves empty and contributes aliases that improve suffix matching for custom providers; models
# that exist only upstream are excluded, so the catalog never becomes a pull of upstream's list.
# 供应商声明保持权威。上游来源只填充 Manifest 留空的字段，并提供可改善自定义供应商后缀匹配的
# 别名；仅存在于上游的模型被排除，因此目录不会变成上游清单的拉取结果。
function Merge-RemoteCapabilityMetadata {
    param(
        [Parameter(Mandatory = $true)][object[]]$ProviderRecords,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$RemoteRecords
    )

    $byKey = @{}
    $byTail = @{}
    foreach ($record in $RemoteRecords) {
        if (-not $byKey.ContainsKey($record.key)) { $byKey[$record.key] = $record }
        $tail = ($record.key -split "/")[-1]
        if (-not $byTail.ContainsKey($tail)) { $byTail[$tail] = $record } else { $byTail[$tail] = $null }
    }

    $records = [System.Collections.Generic.List[object]]::new()
    $matchedKeys = [System.Collections.Generic.HashSet[string]]::new()
    $enrichedCount = 0
    foreach ($record in $ProviderRecords) {
        $tail = ($record.key -split "/")[-1]
        $reference = $null
        if ($byKey.ContainsKey($record.key)) {
            $reference = $byKey[$record.key]
        } elseif ($byTail.ContainsKey($tail)) {
            $reference = $byTail[$tail]
        }
        $contextWindow = $record.context_window
        $inputs = @($record.input_modalities)
        $outputs = @($record.output_modalities)
        $aliases = @($record.aliases)
        if ($null -ne $reference) {
            [void]$matchedKeys.Add($reference.key)
            $enrichedCount++
            # A remote token window is not proof of a media-generation endpoint's input budget.
            # 远端 Token 窗口不是媒体生成入口输入预算的证明。
            if ($null -eq $contextWindow -and @($outputs).Count -eq 1 -and (@($outputs) -contains "text") -and $null -ne $reference.context_window -and [long]$reference.context_window -gt 0) { $contextWindow = [long]$reference.context_window }
            if ($inputs.Count -eq 0) { $inputs = @($reference.input_modalities) }
            if ($outputs.Count -eq 0) { $outputs = @($reference.output_modalities) }
            $aliases = @(@($record.aliases) + @($reference.aliases) | Sort-Object -Unique)
        }
        $records.Add([pscustomobject]@{
            key = $record.key
            aliases = $aliases
            context_window = $contextWindow
            output_limit = $record.output_limit
            reasoning = $record.reasoning
            reasoning_control = $record.reasoning_control
            reasoning_efforts = @($record.reasoning_efforts)
            input_modalities = Format-ModalityList $inputs
            output_modalities = Format-ModalityList $outputs
        })
    }

    return [pscustomobject]@{
        records = @($records)
        enriched = $enrichedCount
        excluded = ($RemoteRecords.Count - $matchedKeys.Count)
    }
}

# Builds the deterministic catalog document from merged records.
# 由合并后的记录构建确定性目录文档。 (CN)
function New-CommonModelCapabilityDocument {
    param([Parameter(Mandatory = $true)][object[]]$Records)

    $canonicalKeys = [System.Collections.Generic.HashSet[string]]::new()
    foreach ($record in $Records) { [void]$canonicalKeys.Add($record.key) }
    $aliasOwners = @{}
    foreach ($record in $Records) {
        foreach ($alias in $record.aliases) {
            if (-not $aliasOwners.ContainsKey($alias)) { $aliasOwners[$alias] = [System.Collections.Generic.List[string]]::new() }
            $aliasOwners[$alias].Add($record.key)
        }
    }

    $models = foreach ($record in ($Records | Sort-Object key)) {
        $uniqueAliases = @(
            $record.aliases |
                Where-Object { $aliasOwners[$_].Count -eq 1 -and -not $canonicalKeys.Contains($_) } |
                Sort-Object -Unique
        )
        $entry = [ordered]@{
            key = $record.key
            aliases = $uniqueAliases
        }
        if ($null -ne $record.context_window -and [long]$record.context_window -gt 0) {
            $entry.context_window = [long]$record.context_window
        } elseif (@($record.output_modalities).Count -eq 1 -and (@($record.output_modalities) -contains "text")) {
            $entry.context_window = 200000
        }
        $entry.input_modalities = @($record.input_modalities)
        $entry.output_modalities = @($record.output_modalities)
        if ($null -ne $record.output_limit) { $entry.output_limit = [long]$record.output_limit }
        if ($null -ne $record.reasoning) { $entry.reasoning = [string]$record.reasoning }
        if ($null -ne $record.reasoning_control) { $entry.reasoning_control = [string]$record.reasoning_control }
        if (@($record.reasoning_efforts).Count -gt 0) { $entry.reasoning_efforts = @($record.reasoning_efforts) }
        $entry
    }

    return [ordered]@{
        schema_version = 2
        sources = @(
            "vulcan:bundled-provider-manifests",
            "vulcan:provider-specific-capabilities",
            "openrouter:/api/v1/models?output_modalities=all (cross reference for context and modality metadata only)"
        )
        fallback = [ordered]@{
            context_window = 200000
            input_modalities = @("text")
            output_modalities = @("text")
        }
        models = @($models)
    }
}

$resolvedOutputPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    [System.IO.Path]::GetFullPath($OutputPath)
} else {
    [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputPath))
}

# The catalog inventory is owned by us: every bundled manifest model rule becomes a record, and
# the upstream source is consulted only as a cross reference. Upstream-only models are excluded.
# 目录清单由我们拥有：每条内置清单模型规则都成为一条记录，上游来源仅作为交叉参照；仅存在于
# 上游的模型被排除。
$providerRecords = Get-ProviderConfiguredModelRecords -Directory $ProviderManifestsPath
$remoteRecords = @()
if (-not $SkipRemoteSync) {
    $response = Invoke-RestMethod -Method Get -Uri $SourceUrl -Headers @{ "User-Agent" = "VulcanCode-CapabilityCatalog/1.0" }
    $remoteRecords = @(Get-RemoteModelRecords -Response $response)
}
$crossReference = Merge-RemoteCapabilityMetadata -ProviderRecords $providerRecords -RemoteRecords $remoteRecords
$catalog = New-CommonModelCapabilityDocument -Records $crossReference.records
Write-Output ("Inventoried {0} provider-configured record(s); cross-referenced {1} against {2} upstream record(s) and excluded {3} upstream-only model(s)." -f $providerRecords.Count, $crossReference.enriched, $remoteRecords.Count, $crossReference.excluded)

# Published records outside our own inventory are reported so the shrink stays visible.
# 我们自有清单之外的已发布记录会被报告出来，使收缩过程可见。
$catalogKeys = [System.Collections.Generic.HashSet[string]]::new()
foreach ($model in $catalog.models) { [void]$catalogKeys.Add([string]$model.key) }
$outsideKeys = @((Get-ExistingModelRecords -Path $resolvedOutputPath) | Where-Object { -not $catalogKeys.Contains($_.key) })
if ($outsideKeys.Count -gt 0) {
    Write-Output ("Removing {0} published record(s) that no bundled manifest declares." -f $outsideKeys.Count)
}
if ($DryRun) {
    return
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
