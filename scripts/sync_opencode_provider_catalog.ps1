# Synchronizes models.dev provider catalog entries into local Vulcan provider resources.
# 将 models.dev 的供应商目录条目同步到本地 Vulcan provider 资源目录。
param(
    [string]$ProviderDirectory,
    [string]$ReportPath,
    [string]$RemoteCatalogUrl = "https://models.dev/api.json",
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Provider identifiers intentionally excluded from local resource synchronization.
# 主动从本地资源同步中排除的供应商标识集合。
$Script:ExcludedProviderIds = @(
    "opencode",
    "opencode-go"
)

# Provider identifiers that still require manual runtime support review after resource sync.
# 资源同步后仍需人工评估运行时支持的供应商标识集合。
$Script:ManualReviewProviderIds = @(
    "amazon-bedrock",
    "cloudflare-ai-gateway",
    "github-copilot",
    "gitlab",
    "google-vertex",
    "google-vertex-anthropic",
    "sap-ai-core",
    "v0",
    "vercel"
)

# Runtime overrides keyed by exact provider identifier.
# 按精确 provider 标识声明的运行时覆盖表。
$Script:RuntimeOverridesByProviderId = @{
    "openai" = "openai"
    "anthropic" = "anthropic"
    "azure" = "azure-openai"
    "google" = "google-gemini"
    "xai" = "xai"
    "mistral" = "mistral"
    "groq" = "groq"
    "cohere" = "cohere"
    "togetherai" = "together-ai"
    "perplexity" = "perplexity"
    "openrouter" = "openrouter"
    "aihubmix" = "openai-compatible"
    "cerebras" = "openai-compatible"
    "deepinfra" = "openai-compatible"
    "venice" = "openai-compatible"
    "amazon-bedrock" = "openai-compatible"
    "cloudflare-ai-gateway" = "openai-compatible"
    "github-copilot" = "openai-compatible"
    "gitlab" = "openai-compatible"
    "google-vertex" = "openai-compatible"
    "google-vertex-anthropic" = "openai-compatible"
    "sap-ai-core" = "openai-compatible"
    "v0" = "openai-compatible"
    "vercel" = "openai-compatible"
}

# Runtime overrides keyed by remote npm package name.
# 按远端 npm package 名称声明的运行时覆盖表。
$Script:RuntimeOverridesByNpmPackage = @{
    "@ai-sdk/openai-compatible" = "openai-compatible"
    "@ai-sdk/openai" = "openai"
    "@ai-sdk/anthropic" = "anthropic"
    "@ai-sdk/azure" = "azure-openai"
    "@ai-sdk/google" = "google-gemini"
    "@ai-sdk/xai" = "xai"
    "@ai-sdk/mistral" = "mistral"
    "@ai-sdk/groq" = "groq"
    "@ai-sdk/cohere" = "cohere"
    "@ai-sdk/togetherai" = "together-ai"
    "@ai-sdk/perplexity" = "perplexity"
    "@openrouter/ai-sdk-provider" = "openrouter"
    "@ai-sdk/cerebras" = "openai-compatible"
    "@ai-sdk/deepinfra" = "openai-compatible"
    "@ai-sdk/google-vertex" = "openai-compatible"
    "@ai-sdk/google-vertex/anthropic" = "openai-compatible"
    "@ai-sdk/gateway" = "openai-compatible"
    "@aihubmix/ai-sdk-provider" = "openai-compatible"
    "ai-gateway-provider" = "openai-compatible"
    "gitlab-ai-provider" = "openai-compatible"
    "venice-ai-sdk-provider" = "openai-compatible"
    "@jerome-benoit/sap-ai-provider-v2" = "openai-compatible"
}

# Explicit base URL overrides used when models.dev metadata omits a concrete API endpoint.
# 当 models.dev 元数据缺少具体 API 地址时使用的显式基址覆盖表。
$Script:ConcreteBaseUrlOverrides = @{
    "aihubmix" = "https://aihubmix.com/v1"
    "cerebras" = "https://api.cerebras.ai/v1"
    "deepinfra" = "https://api.deepinfra.com/v1/openai"
    "venice" = "https://api.venice.ai/api/v1"
}

# Field overrides keyed by provider identifier for providers whose auth or endpoint shape does not map cleanly.
# 针对鉴权或端点形态无法直接映射的供应商所声明的字段覆盖表。
$Script:FieldOverridesByProviderId = @{
    "amazon-bedrock" = @()
    "cloudflare-ai-gateway" = @("baseUrl", "apiKey")
    "databricks" = @("baseUrl", "apiKey")
    "github-copilot" = @("apiKey")
    "gitlab" = @("apiKey")
    "google-vertex" = @("baseUrl")
    "google-vertex-anthropic" = @("baseUrl")
    "sap-ai-core" = @()
    "v0" = @("apiKey")
    "vercel" = @("apiKey")
}

# Model discovery overrides keyed by provider identifier.
# 按 provider 标识声明的模型发现策略覆盖表。
$Script:ModelDiscoveryOverridesByProviderId = @{
    "amazon-bedrock" = "none"
    "cloudflare-ai-gateway" = "none"
    "gitlab" = "none"
    "google-vertex" = "none"
    "google-vertex-anthropic" = "none"
    "sap-ai-core" = "none"
}

# Providers whose generated base URL should remain manually editable because the runtime needs extra routing state.
# 运行时仍需额外路由状态、因此生成基址必须保留人工可编辑能力的供应商集合。
$Script:ManualBaseUrlProviderIds = @(
    "amazon-bedrock",
    "cloudflare-ai-gateway",
    "databricks",
    "google-vertex",
    "google-vertex-anthropic",
    "sap-ai-core"
)

function Test-NonEmptyString {
    param(
        [AllowNull()]
        [object]$Value
    )

    # String candidate coerced from one arbitrary JSON value.
    # 从任意 JSON 值强制转换得到的字符串候选值。
    $text = if ($null -eq $Value) { "" } else { [string]$Value }
    return -not [string]::IsNullOrWhiteSpace($text)
}

function Get-ObjectPropertyValue {
    param(
        [AllowNull()]
        [object]$Object,
        [string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }

    # Named property selected from one arbitrary PowerShell object when it exists.
    # 从任意 PowerShell 对象中按名称选出的已存在属性值。
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }

    return $property.Value
}

function ConvertTo-PlainData {
    param(
        [AllowNull()]
        [object]$Value
    )

    if ($null -eq $Value) {
        return $null
    }

    if ($Value -is [string] -or $Value -is [bool] -or $Value -is [int] -or $Value -is [long] -or $Value -is [double] -or $Value -is [decimal]) {
        return $Value
    }

    if ($Value -is [System.Collections.IDictionary]) {
        # Ordered dictionary used to preserve serialized field order.
        # 用于保留序列化字段顺序的有序字典。
        $result = [ordered]@{}
        foreach ($entry in $Value.GetEnumerator()) {
            $result[[string]$entry.Key] = ConvertTo-PlainData -Value $entry.Value
        }
        return $result
    }

    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        # Materialized array copy used for JSON serialization stability.
        # 为保证 JSON 序列化稳定性而物化得到的数组副本。
        $items = @()
        foreach ($item in $Value) {
            $items += ,(ConvertTo-PlainData -Value $item)
        }
        return ,$items
    }

    # Ordered object rebuilt from one PSCustomObject property bag.
    # 从 PSCustomObject 属性包重建得到的有序对象。
    $objectResult = [ordered]@{}
    foreach ($property in $Value.PSObject.Properties) {
        $objectResult[[string]$property.Name] = ConvertTo-PlainData -Value $property.Value
    }
    return $objectResult
}

function ConvertFrom-JsonCompat {
    param(
        [string]$Json
    )

    # ConvertFrom-Json command metadata used to detect legacy Windows PowerShell parameter support.
    # 用于检测旧版 Windows PowerShell 参数支持情况的 ConvertFrom-Json 命令元数据。
    $command = Get-Command ConvertFrom-Json
    if ($command.Parameters.ContainsKey("Depth")) {
        return $Json | ConvertFrom-Json -Depth 100
    }

    return $Json | ConvertFrom-Json
}

function Add-PropertyIfPresent {
    param(
        [System.Collections.Specialized.OrderedDictionary]$Target,
        [string]$Name,
        [AllowNull()]
        [object]$Value
    )

    if ($null -eq $Value) {
        return
    }

    if ($Value -is [string] -and [string]::IsNullOrWhiteSpace($Value)) {
        return
    }

    if ($Value -is [System.Collections.ICollection] -and $Value.Count -eq 0) {
        return
    }

    $Target[$Name] = $Value
}

function Normalize-UrlValue {
    param(
        [AllowNull()]
        [object]$Url
    )

    if (-not (Test-NonEmptyString -Value $Url)) {
        return $null
    }

    # Raw URL text trimmed from remote metadata.
    # 从远端元数据中裁剪得到的原始 URL 文本。
    $normalized = ([string]$Url).Trim()
    if ($normalized.Length -gt 1) {
        $normalized = $normalized.TrimEnd("/")
    }
    return $normalized
}

function Resolve-ProviderRuntime {
    param(
        [string]$ProviderId,
        [AllowNull()]
        [object]$ProviderNpm
    )

    if ($Script:RuntimeOverridesByProviderId.ContainsKey($ProviderId)) {
        return [string]$Script:RuntimeOverridesByProviderId[$ProviderId]
    }

    # Remote npm package name used to infer one compatible local runtime family.
    # 用于推导兼容本地运行时族的远端 npm 包名。
    $packageName = if ($null -eq $ProviderNpm) { "" } else { [string]$ProviderNpm }
    if ($Script:RuntimeOverridesByNpmPackage.ContainsKey($packageName)) {
        return [string]$Script:RuntimeOverridesByNpmPackage[$packageName]
    }

    return "openai-compatible"
}

function New-BaseUrlConfig {
    param(
        [string]$Kind,
        [AllowNull()]
        [string]$Value,
        [AllowNull()]
        [string]$Template,
        [AllowNull()]
        [string]$Parameter,
        [bool]$AllowOverride
    )

    # Structured base URL configuration serialized into one provider resource.
    # 序列化进单个 provider 资源文件的结构化基址配置。
    $config = [ordered]@{
        kind = $Kind
    }

    Add-PropertyIfPresent -Target $config -Name "value" -Value $Value
    Add-PropertyIfPresent -Target $config -Name "template" -Value $Template
    Add-PropertyIfPresent -Target $config -Name "parameter" -Value $Parameter
    $config["allowOverride"] = $AllowOverride
    return $config
}

function Resolve-ProviderBaseUrlConfig {
    param(
        [string]$ProviderId,
        [string]$Runtime,
        [AllowNull()]
        [object]$RemoteApi
    )

    if ($Script:ConcreteBaseUrlOverrides.ContainsKey($ProviderId)) {
        # Manually curated fixed endpoint used when the upstream catalog omits one.
        # 当上游目录未给出端点时使用的人工维护固定地址。
        $overrideUrl = [string]$Script:ConcreteBaseUrlOverrides[$ProviderId]
        return New-BaseUrlConfig -Kind "fixed" -Value $overrideUrl -Template $null -Parameter $null -AllowOverride $false
    }

    # Normalized remote API endpoint string extracted from models.dev metadata.
    # 从 models.dev 元数据中提取并规范化的远端 API 端点字符串。
    $remoteUrl = Normalize-UrlValue -Url $RemoteApi
    if (Test-NonEmptyString -Value $remoteUrl) {
        if ($remoteUrl -match "\$\{AZURE_RESOURCE_NAME\}") {
            # Azure resource-name template reused by the existing local provider schema.
            # 复用现有本地 provider schema 的 Azure 资源名模板。
            $template = $remoteUrl.Replace('${AZURE_RESOURCE_NAME}', '{resourceName}')
            return New-BaseUrlConfig -Kind "template" -Value $null -Template $template -Parameter "resourceName" -AllowOverride $false
        }

        if ($remoteUrl -match "\$\{CLOUDFLARE_ACCOUNT_ID\}") {
            # Cloudflare account-id template reused by the existing local provider schema.
            # 复用现有本地 provider schema 的 Cloudflare 账户模板。
            $template = $remoteUrl.Replace('${CLOUDFLARE_ACCOUNT_ID}', '{accountId}')
            return New-BaseUrlConfig -Kind "template" -Value $null -Template $template -Parameter "accountId" -AllowOverride $false
        }

        if ($remoteUrl -match "\$\{[A-Z0-9_]+\}") {
            # Manual base URL placeholder required because the current schema lacks a dedicated field for this variable.
            # 当前 schema 缺少该变量的专属字段，因此需要人工填写基址。
            return New-BaseUrlConfig -Kind "fixed" -Value $null -Template $null -Parameter $null -AllowOverride $true
        }

        return New-BaseUrlConfig -Kind "fixed" -Value $remoteUrl -Template $null -Parameter $null -AllowOverride $false
    }

    if ($Runtime -in @("anthropic", "google-gemini", "xai", "mistral", "groq", "cohere", "together-ai", "perplexity")) {
        return New-BaseUrlConfig -Kind "native" -Value $null -Template $null -Parameter $null -AllowOverride $false
    }

    if ($Runtime -eq "azure-openai") {
        return New-BaseUrlConfig -Kind "template" -Value $null -Template "https://{resourceName}.openai.azure.com/openai" -Parameter "resourceName" -AllowOverride $false
    }

    if ($ProviderId -in $Script:ManualBaseUrlProviderIds) {
        return New-BaseUrlConfig -Kind "fixed" -Value $null -Template $null -Parameter $null -AllowOverride $true
    }

    return New-BaseUrlConfig -Kind "fixed" -Value $null -Template $null -Parameter $null -AllowOverride $true
}

function Resolve-ProviderFields {
    param(
        [string]$ProviderId,
        [string[]]$CredentialEnvKeys,
        [System.Collections.Specialized.OrderedDictionary]$BaseUrlConfig
    )

    if ($Script:FieldOverridesByProviderId.ContainsKey($ProviderId)) {
        return ,([string[]]$Script:FieldOverridesByProviderId[$ProviderId])
    }

    # Generated ordered field list exposed by the local provider configuration form.
    # 暴露给本地 provider 配置表单的生成后有序字段列表。
    $fields = New-Object System.Collections.Generic.List[string]

    if ([string]$BaseUrlConfig["parameter"] -eq "accountId") {
        $fields.Add("accountId")
    }

    if ([string]$BaseUrlConfig["parameter"] -eq "resourceName") {
        $fields.Add("resourceName")
    }

    if ([bool]$BaseUrlConfig["allowOverride"]) {
        $fields.Add("baseUrl")
    }

    # Whether one generic token-like credential input should be exposed as apiKey.
    # 是否应将通用 token 风格凭据输入暴露为 apiKey。
    $includeApiKeyField = $false
    foreach ($envKey in $CredentialEnvKeys) {
        if ($envKey -match "(?i)(API_KEY|TOKEN)$") {
            $includeApiKeyField = $true
            break
        }
    }

    if ($includeApiKeyField) {
        $fields.Add("apiKey")
    }

    # Distinct field array materialized in insertion order.
    # 以插入顺序物化得到的不重复字段数组。
    $distinctFields = New-Object System.Collections.Generic.List[string]
    foreach ($field in $fields) {
        if (-not $distinctFields.Contains($field)) {
            $distinctFields.Add($field)
        }
    }
    return ,([string[]]$distinctFields.ToArray())
}

function Resolve-ModelDiscoveryKind {
    param(
        [string]$ProviderId,
        [string]$Runtime
    )

    if ($Script:ModelDiscoveryOverridesByProviderId.ContainsKey($ProviderId)) {
        return [string]$Script:ModelDiscoveryOverridesByProviderId[$ProviderId]
    }

    if ($Runtime -in @("openai-compatible", "openai", "openrouter")) {
        return "openai-models"
    }

    return "none"
}

function Convert-RemoteInterleavedCapability {
    param(
        [AllowNull()]
        [object]$RemoteInterleaved
    )

    if ($null -eq $RemoteInterleaved) {
        return $null
    }

    if ($RemoteInterleaved -is [bool]) {
        return $RemoteInterleaved
    }

    # Structured interleaved reasoning field mapping copied from the upstream catalog payload.
    # 从上游目录载荷复制得到的结构化交错推理字段映射。
    $fieldMapping = [ordered]@{}
    # Interleaved reasoning field name read from the remote metadata payload.
    # 从远端元数据载荷中读取到的交错推理字段名。
    $fieldName = Get-ObjectPropertyValue -Object $RemoteInterleaved -Name "field"
    if (Test-NonEmptyString -Value $fieldName) {
        $fieldMapping["field"] = [string]$fieldName
    }
    if ($fieldMapping.Count -eq 0) {
        return $null
    }
    return $fieldMapping
}

function Convert-RemoteModelChoice {
    param(
        [string]$ModelId,
        [AllowNull()]
        [object]$RemoteModel
    )

    # Local model choice object rebuilt from one remote models.dev entry.
    # 从单个远端 models.dev 条目重建得到的本地模型项对象。
    # Human-readable model name resolved from the remote catalog entry.
    # 从远端目录条目中解析得到的模型显示名称。
    $modelName = Get-ObjectPropertyValue -Object $RemoteModel -Name "name"

    # Remote limit object carrying context, input, and output token boundaries.
    # 携带上下文、输入与输出 token 上限的远端 limit 对象。
    $remoteLimit = Get-ObjectPropertyValue -Object $RemoteModel -Name "limit"

    # Remote modality object carrying input and output modality arrays.
    # 携带输入与输出模态数组的远端 modalities 对象。
    $remoteModalities = Get-ObjectPropertyValue -Object $RemoteModel -Name "modalities"

    $modelChoice = [ordered]@{
        id = $ModelId
        name = [string]$modelName
    }

    Add-PropertyIfPresent -Target $modelChoice -Name "status" -Value (Get-ObjectPropertyValue -Object $RemoteModel -Name "status")
    Add-PropertyIfPresent -Target $modelChoice -Name "family" -Value (Get-ObjectPropertyValue -Object $RemoteModel -Name "family")
    Add-PropertyIfPresent -Target $modelChoice -Name "releaseDate" -Value (Get-ObjectPropertyValue -Object $RemoteModel -Name "release_date")
    Add-PropertyIfPresent -Target $modelChoice -Name "contextLimit" -Value (Get-ObjectPropertyValue -Object $remoteLimit -Name "context")
    Add-PropertyIfPresent -Target $modelChoice -Name "inputLimit" -Value (Get-ObjectPropertyValue -Object $remoteLimit -Name "input")
    Add-PropertyIfPresent -Target $modelChoice -Name "outputLimit" -Value (Get-ObjectPropertyValue -Object $remoteLimit -Name "output")
    Add-PropertyIfPresent -Target $modelChoice -Name "reasoning" -Value (Get-ObjectPropertyValue -Object $RemoteModel -Name "reasoning")
    Add-PropertyIfPresent -Target $modelChoice -Name "toolCall" -Value (Get-ObjectPropertyValue -Object $RemoteModel -Name "tool_call")
    Add-PropertyIfPresent -Target $modelChoice -Name "temperature" -Value (Get-ObjectPropertyValue -Object $RemoteModel -Name "temperature")
    Add-PropertyIfPresent -Target $modelChoice -Name "attachment" -Value (Get-ObjectPropertyValue -Object $RemoteModel -Name "attachment")

    # Interleaved reasoning capability converted from the upstream entry when present.
    # 远端条目存在时转换得到的交错推理能力。
    $interleavedCapability = Convert-RemoteInterleavedCapability -RemoteInterleaved (Get-ObjectPropertyValue -Object $RemoteModel -Name "interleaved")
    Add-PropertyIfPresent -Target $modelChoice -Name "interleaved" -Value $interleavedCapability

    # Input modality array copied from upstream metadata.
    # 从上游元数据复制得到的输入模态数组。
    $inputModalities = @()
    foreach ($inputModality in @(Get-ObjectPropertyValue -Object $remoteModalities -Name "input")) {
        $inputModalities += ,[string]$inputModality
    }
    Add-PropertyIfPresent -Target $modelChoice -Name "inputModalities" -Value $inputModalities

    # Output modality array copied from upstream metadata.
    # 从上游元数据复制得到的输出模态数组。
    $outputModalities = @()
    foreach ($outputModality in @(Get-ObjectPropertyValue -Object $remoteModalities -Name "output")) {
        $outputModalities += ,[string]$outputModality
    }
    Add-PropertyIfPresent -Target $modelChoice -Name "outputModalities" -Value $outputModalities
    return $modelChoice
}

function Convert-RemoteProviderDefinition {
    param(
        [string]$ProviderId,
        [AllowNull()]
        [object]$RemoteProvider
    )

    # Resolved runtime family for the current provider entry.
    # 当前 provider 条目的解析后运行时族。
    # Remote provider npm package name read from the current models.dev entry.
    # 从当前 models.dev 条目读取到的远端 provider npm 包名。
    $providerNpm = Get-ObjectPropertyValue -Object $RemoteProvider -Name "npm"

    # Remote provider API endpoint string read from the current models.dev entry.
    # 从当前 models.dev 条目读取到的远端 provider API 端点字符串。
    $providerApi = Get-ObjectPropertyValue -Object $RemoteProvider -Name "api"

    # Remote provider environment variable list read from the current models.dev entry.
    # 从当前 models.dev 条目读取到的远端 provider 环境变量列表。
    $providerEnv = Get-ObjectPropertyValue -Object $RemoteProvider -Name "env"

    # Remote provider model map read from the current models.dev entry.
    # 从当前 models.dev 条目读取到的远端 provider 模型映射。
    $providerModels = Get-ObjectPropertyValue -Object $RemoteProvider -Name "models"

    $runtime = Resolve-ProviderRuntime -ProviderId $ProviderId -ProviderNpm $providerNpm

    # Structured base URL definition synthesized from upstream provider metadata and local overrides.
    # 结合上游 provider 元数据与本地覆盖规则合成得到的结构化基址定义。
    $baseUrl = Resolve-ProviderBaseUrlConfig -ProviderId $ProviderId -Runtime $runtime -RemoteApi $providerApi

    # Credential environment-variable list copied from upstream metadata.
    # 从上游元数据复制得到的凭据环境变量列表。
    $credentialEnvKeys = @()
    foreach ($envKey in @($providerEnv)) {
        $credentialEnvKeys += ,[string]$envKey
    }

    # Ordered form-field list inferred for the current provider definition.
    # 为当前 provider 定义推导出的有序表单字段列表。
    $fields = Resolve-ProviderFields -ProviderId $ProviderId -CredentialEnvKeys $credentialEnvKeys -BaseUrlConfig $baseUrl

    # Model discovery kind inferred from the runtime family and local overrides.
    # 根据运行时族与本地覆盖规则推导得到的模型发现策略。
    $modelDiscoveryKind = Resolve-ModelDiscoveryKind -ProviderId $ProviderId -Runtime $runtime

    # Embedded model catalog converted from the upstream provider record.
    # 从上游 provider 记录转换得到的内置模型目录。
    $models = @()
    foreach ($modelProperty in $providerModels.PSObject.Properties) {
        $models += ,(Convert-RemoteModelChoice -ModelId ([string]$modelProperty.Name) -RemoteModel $modelProperty.Value)
    }

    # Generated local provider resource definition.
    # 生成得到的本地 provider 资源定义对象。
    $definition = [ordered]@{
        id = $ProviderId
        name = [string](Get-ObjectPropertyValue -Object $RemoteProvider -Name "name")
        runtime = $runtime
    }

    Add-PropertyIfPresent -Target $definition -Name "credentialEnvKeys" -Value $credentialEnvKeys
    Add-PropertyIfPresent -Target $definition -Name "fields" -Value $fields
    $definition["baseUrl"] = $baseUrl
    $definition["modelDiscovery"] = [ordered]@{ kind = $modelDiscoveryKind }
    $definition["models"] = $models
    return $definition
}

function Get-LocalProviderIndex {
    param(
        [string]$Directory
    )

    # Local provider index keyed by provider identifier.
    # 按 provider 标识建立索引的本地资源映射表。
    $index = @{}
    foreach ($file in Get-ChildItem -Path $Directory -Filter "*.json" | Sort-Object Name) {
        # Decoded local provider definition loaded from one JSON resource file.
        # 从单个 JSON 资源文件载入并解码得到的本地 provider 定义。
        $definition = ConvertFrom-JsonCompat -Json (Get-Content -Raw -Path $file.FullName -Encoding UTF8)
        $providerId = [string]$definition.id
        $index[$providerId] = [pscustomobject]@{
            Path = $file.FullName
            FileName = $file.Name
            Definition = $definition
        }
    }
    return $index
}

function Get-RemoteProviderIndex {
    param(
        [string]$CatalogUrl
    )

    # Remote provider catalog decoded from the current models.dev snapshot.
    # 从当前 models.dev 快照解码得到的远端 provider 目录。
    $remoteCatalog = Invoke-RestMethod -Uri $CatalogUrl -Headers @{ "User-Agent" = "vulcan-code-provider-sync/1.0" }

    # Remote provider index keyed by provider identifier.
    # 按 provider 标识建立索引的远端 provider 映射表。
    $index = @{}
    foreach ($property in $remoteCatalog.PSObject.Properties) {
        $index[[string]$property.Name] = $property.Value
    }
    return $index
}

function Merge-ProviderModels {
    param(
        [AllowNull()]
        [object]$ExistingDefinition,
        [System.Collections.Specialized.OrderedDictionary]$GeneratedDefinition,
        [AllowNull()]
        [System.Collections.Generic.List[string]]$AddedModelIds
    )

    # Existing model identifier set used to preserve all local-only model metadata and ordering.
    # 用于保留全部本地模型元数据与顺序的既有模型标识集合。
    $existingModelIds = New-Object System.Collections.Generic.HashSet[string]([System.StringComparer]::Ordinal)

    # Merged model array reused for the final provider definition.
    # 供最终 provider 定义复用的合并后模型数组。
    $mergedModels = @()
    foreach ($existingModel in @($ExistingDefinition.models)) {
        $mergedModels += ,(ConvertTo-PlainData -Value $existingModel)
        [void]$existingModelIds.Add([string]$existingModel.id)
    }

    foreach ($generatedModel in @($GeneratedDefinition["models"])) {
        if ($existingModelIds.Contains([string]$generatedModel["id"])) {
            continue
        }

        $mergedModels += ,$generatedModel
        if ($null -ne $AddedModelIds) {
            $AddedModelIds.Add([string]$generatedModel["id"]) | Out-Null
        }
    }

    return $mergedModels
}

function Merge-ProviderDefinition {
    param(
        [AllowNull()]
        [object]$ExistingDefinition,
        [System.Collections.Specialized.OrderedDictionary]$GeneratedDefinition
    )

    # Added model identifier list collected while merging one existing provider definition.
    # 合并单个既有 provider 定义时收集到的新增模型标识列表。
    $addedModelIds = New-Object System.Collections.Generic.List[string]

    # Existing root object converted into ordered plain data so custom local fields stay intact.
    # 将既有根对象转换为有序普通数据结构，以保留本地自定义字段。
    $mergedDefinition = ConvertTo-PlainData -Value $ExistingDefinition
    $mergedDefinition["models"] = Merge-ProviderModels -ExistingDefinition $ExistingDefinition -GeneratedDefinition $GeneratedDefinition -AddedModelIds $addedModelIds

    return [pscustomobject]@{
        Definition = $mergedDefinition
        AddedModelIds = @($addedModelIds)
    }
}

function Write-JsonFile {
    param(
        [string]$Path,
        [AllowNull()]
        [object]$Data
    )

    # Pretty-printed JSON payload written to one resource file.
    # 写入单个资源文件的格式化 JSON 载荷。
    $json = $Data | ConvertTo-Json -Depth 100
    [System.IO.File]::WriteAllText($Path, "$json`n", [System.Text.UTF8Encoding]::new($false))
}

function Assert-ArrayPropertyShape {
    param(
        [AllowNull()]
        [object]$Object,
        [string]$PropertyName,
        [string]$ContextLabel
    )

    $value = Get-ObjectPropertyValue -Object $Object -Name $PropertyName
    if ($null -eq $value) {
        return
    }

    if ($value -is [string] -or -not ($value -is [System.Collections.IEnumerable])) {
        throw "$ContextLabel property '$PropertyName' must stay an array."
    }
}

function Assert-ProviderDefinitionShape {
    param(
        [AllowNull()]
        [object]$Definition,
        [string]$ContextLabel
    )

    Assert-ArrayPropertyShape -Object $Definition -PropertyName "credentialEnvKeys" -ContextLabel $ContextLabel
    Assert-ArrayPropertyShape -Object $Definition -PropertyName "fields" -ContextLabel $ContextLabel
    Assert-ArrayPropertyShape -Object $Definition -PropertyName "models" -ContextLabel $ContextLabel

    $models = Get-ObjectPropertyValue -Object $Definition -Name "models"
    $modelIndex = 0
    foreach ($model in @($models)) {
        $modelContext = "$ContextLabel model[$modelIndex]"
        Assert-ArrayPropertyShape -Object $model -PropertyName "inputModalities" -ContextLabel $modelContext
        Assert-ArrayPropertyShape -Object $model -PropertyName "outputModalities" -ContextLabel $modelContext
        Assert-ArrayPropertyShape -Object $model -PropertyName "aiTools" -ContextLabel $modelContext
        $modelIndex += 1
    }
}

function Write-SyncReport {
    param(
        [string]$Path,
        [object[]]$CreatedProviders,
        [object[]]$UpdatedProviders,
        [object[]]$ManualReviewProviders
    )

    # In-memory Markdown lines used to build the synchronization report.
    # 用于构建同步报告的内存 Markdown 行集合。
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("# Provider Sync Report") | Out-Null
    $lines.Add("") | Out-Null
    $lines.Add('- Source: `https://models.dev/api.json`') | Out-Null
    $lines.Add('- Excluded providers: `opencode`, `opencode-go`') | Out-Null
    $lines.Add("- Generated at: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')") | Out-Null
    $lines.Add("") | Out-Null
    $lines.Add("## Created Providers") | Out-Null
    $lines.Add("") | Out-Null

    foreach ($createdProvider in $CreatedProviders) {
        $lines.Add(('- ``{0}`` -> `{1}` (`{2}` models)' -f $createdProvider.Id, $createdProvider.FileName, $createdProvider.ModelCount)) | Out-Null
    }

    if ($CreatedProviders.Count -eq 0) {
        $lines.Add("- None") | Out-Null
    }

    $lines.Add("") | Out-Null
    $lines.Add("## Updated Providers") | Out-Null
    $lines.Add("") | Out-Null

    foreach ($updatedProvider in $UpdatedProviders) {
        $lines.Add(('- ``{0}`` +`{1}` models' -f $updatedProvider.Id, $updatedProvider.AddedModelCount)) | Out-Null
        foreach ($addedModelId in $updatedProvider.AddedModelIds) {
            $lines.Add(('  - ``{0}``' -f $addedModelId)) | Out-Null
        }
    }

    if ($UpdatedProviders.Count -eq 0) {
        $lines.Add("- None") | Out-Null
    }

    $lines.Add("") | Out-Null
    $lines.Add("## Manual Review Providers") | Out-Null
    $lines.Add("") | Out-Null

    foreach ($manualReviewProvider in $ManualReviewProviders) {
        $lines.Add(('- ``{0}`` (`{1}`)' -f $manualReviewProvider.Id, $manualReviewProvider.Note)) | Out-Null
    }

    if ($ManualReviewProviders.Count -eq 0) {
        $lines.Add("- None") | Out-Null
    }

    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($Path)) | Out-Null
    # Materialized string array emitted to disk by the Markdown report writer.
    # 由 Markdown 报告写入器输出到磁盘的物化字符串数组。
    $lineArray = $lines.ToArray()
    [System.IO.File]::WriteAllLines($Path, $lineArray, [System.Text.UTF8Encoding]::new($false))
}

# Repository-root path resolved from the current script location.
# 根据当前脚本位置解析得到的仓库根目录路径。
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

if (-not (Test-NonEmptyString -Value $ProviderDirectory)) {
    # Default provider resource directory used by the local GUI application.
    # 本地 GUI 应用默认使用的 provider 资源目录。
    $ProviderDirectory = Join-Path $repositoryRoot "apps\vulcan-code-gui\resources\providers"
}

if (-not (Test-NonEmptyString -Value $ReportPath)) {
    # Default synchronization report path emitted under the local temporary work area.
    # 输出到本地临时工作区下的默认同步报告路径。
    $ReportPath = Join-Path $repositoryRoot ".vulcan\temp\provider-sync-report-$(Get-Date -Format 'yyyyMMdd-HHmmss').md"
}

# Local provider index loaded from the current resource directory.
# 从当前资源目录载入的本地 provider 索引。
$localProviderIndex = Get-LocalProviderIndex -Directory $ProviderDirectory

# Remote provider index loaded from the latest models.dev catalog snapshot.
# 从最新 models.dev 目录快照载入的远端 provider 索引。
$remoteProviderIndex = Get-RemoteProviderIndex -CatalogUrl $RemoteCatalogUrl

# Created provider summary entries collected during the current run.
# 当前执行过程中收集到的新建 provider 摘要条目。
$createdProviders = New-Object System.Collections.Generic.List[object]

# Updated provider summary entries collected during the current run.
# 当前执行过程中收集到的已更新 provider 摘要条目。
$updatedProviders = New-Object System.Collections.Generic.List[object]

# Manual-review summary entries collected during the current run.
# 当前执行过程中收集到的人工复核摘要条目。
$manualReviewProviders = New-Object System.Collections.Generic.List[object]

foreach ($providerId in @($remoteProviderIndex.Keys | Sort-Object)) {
    if ($providerId -in $Script:ExcludedProviderIds) {
        continue
    }

    # Current remote provider entry selected from the latest upstream catalog.
    # 从最新上游目录中选出的当前远端 provider 条目。
    $remoteProvider = $remoteProviderIndex[$providerId]

    # Generated local-format provider definition converted from the remote entry.
    # 从远端条目转换得到的本地格式 provider 定义。
    $generatedDefinition = Convert-RemoteProviderDefinition -ProviderId $providerId -RemoteProvider $remoteProvider

    if ($localProviderIndex.ContainsKey($providerId)) {
        # Existing local provider record reused for additive model synchronization.
        # 用于增量模型同步的既有本地 provider 记录。
        $existingRecord = $localProviderIndex[$providerId]

        # Merge result that preserves all existing local-only fields while appending missing upstream models.
        # 在追加缺失上游模型的同时保留全部既有本地字段的合并结果。
        $mergeResult = Merge-ProviderDefinition -ExistingDefinition $existingRecord.Definition -GeneratedDefinition $generatedDefinition

        if ($mergeResult.AddedModelIds.Count -gt 0) {
            Assert-ProviderDefinitionShape -Definition $mergeResult.Definition -ContextLabel $providerId
            if (-not $DryRun) {
                Write-JsonFile -Path $existingRecord.Path -Data $mergeResult.Definition
            }

            $updatedProviders.Add([pscustomobject]@{
                Id = $providerId
                AddedModelCount = $mergeResult.AddedModelIds.Count
                AddedModelIds = @($mergeResult.AddedModelIds)
            }) | Out-Null
        }
    } else {
        # New provider resource file path created for one provider absent from the local catalog.
        # 为本地目录中缺失的 provider 创建的新资源文件路径。
        $newFilePath = Join-Path $ProviderDirectory "$providerId.json"
        Assert-ProviderDefinitionShape -Definition $generatedDefinition -ContextLabel $providerId
        if (-not $DryRun) {
            Write-JsonFile -Path $newFilePath -Data $generatedDefinition
        }

        $createdProviders.Add([pscustomobject]@{
            Id = $providerId
            FileName = [System.IO.Path]::GetFileName($newFilePath)
            ModelCount = @($generatedDefinition["models"]).Count
        }) | Out-Null
    }

    if ($providerId -in $Script:ManualReviewProviderIds) {
        # Manual-review note describing why the generated provider still needs runtime-specific follow-up.
        # 描述该生成 provider 仍需运行时专项跟进原因的人工复核说明。
        $manualReviewNote = "runtime or auth flow still needs host-specific support"
        $manualReviewProviders.Add([pscustomobject]@{
            Id = $providerId
            Note = $manualReviewNote
        }) | Out-Null
    }
}

Write-SyncReport -Path $ReportPath -CreatedProviders $createdProviders.ToArray() -UpdatedProviders $updatedProviders.ToArray() -ManualReviewProviders $manualReviewProviders.ToArray()

# Final provider count currently present in the local resource directory after synchronization.
# 同步完成后当前本地资源目录中的最终 provider 数量。
$finalProviderCount = (Get-ChildItem -Path $ProviderDirectory -Filter "*.json" | Measure-Object).Count

# Human-readable synchronization summary emitted to the terminal.
# 输出到终端的人类可读同步摘要。
$summary = [pscustomobject]@{
    provider_directory = $ProviderDirectory
    created_provider_count = $createdProviders.Count
    updated_provider_count = $updatedProviders.Count
    manual_review_provider_count = $manualReviewProviders.Count
    final_provider_count = $finalProviderCount
    report_path = $ReportPath
    dry_run = [bool]$DryRun
}

$summary | ConvertTo-Json -Depth 10
