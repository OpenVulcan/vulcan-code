# configure_controller_profile.ps1 writes the build-profile-specific VLDB endpoint into the generated runtime configuration.
# configure_controller_profile.ps1 将构建模式对应的 VLDB 端点写入生成的运行时配置。
param(
    # Release selects the release VLDB endpoint instead of the debug endpoint.
    # Release 用于选择 Release VLDB 端点而不是 Debug 端点。
    [switch]$Release,
    # ConfigPath optionally redirects the generated configuration path for focused verification.
    # ConfigPath 可选地重定向生成配置路径，供定向验证使用。
    [string]$ConfigPath = ""
)

# ErrorActionPreference makes every configuration read or write failure terminate the make workflow.
# ErrorActionPreference 使任何配置读取或写入失败都终止 make 工作流。
$ErrorActionPreference = "Stop"

# ProjectDir stores the repository root for the default generated configuration path.
# ProjectDir 保存仓库根目录，用于解析默认生成配置路径。
$ProjectDir = Split-Path $PSScriptRoot -Parent

# EffectiveConfigPath is the exact generated Controller configuration updated by this invocation.
# EffectiveConfigPath 是本次调用需要更新的精确 Controller 生成配置路径。
$EffectiveConfigPath = $ConfigPath
if ([string]::IsNullOrWhiteSpace($EffectiveConfigPath)) {
    $EffectiveConfigPath = Join-Path $ProjectDir "output\config\controller.json"
}

# ControllerEndpoint is selected exclusively from the requested Cargo build profile.
# ControllerEndpoint 仅根据请求的 Cargo 构建模式进行选择。
$ControllerEndpoint = if ($Release) {
    "http://127.0.0.1:19801"
}
else {
    "http://127.0.0.1:19802"
}

# Configuration stores the existing JSON object so unrelated Controller settings remain unchanged.
# Configuration 保存现有 JSON 对象，确保无关的 Controller 设置保持不变。
$Configuration = [pscustomobject]@{}
if (Test-Path -LiteralPath $EffectiveConfigPath -PathType Leaf) {
    # ConfigurationText is decoded as UTF-8 JSON before any field is changed.
    # ConfigurationText 在修改任何字段前按 UTF-8 JSON 解码。
    $ConfigurationText = [System.IO.File]::ReadAllText($EffectiveConfigPath, [System.Text.Encoding]::UTF8)
    $Configuration = $ConfigurationText | ConvertFrom-Json
    if ($null -eq $Configuration -or $Configuration -isnot [System.Management.Automation.PSCustomObject]) {
        throw "Controller configuration must contain one JSON object: $EffectiveConfigPath"
    }
}

# EndpointProperty identifies whether the existing object already owns the authoritative endpoint field.
# EndpointProperty 标识现有对象是否已经包含权威 endpoint 字段。
$EndpointProperty = $Configuration.PSObject.Properties["endpoint"]
if ($null -ne $EndpointProperty -and [string]$EndpointProperty.Value -eq $ControllerEndpoint) {
    Write-Host "==> Controller profile already configured: $ControllerEndpoint"
    exit 0
}
if ($null -ne $EndpointProperty) {
    $EndpointProperty.Value = $ControllerEndpoint
}
else {
    $Configuration | Add-Member -NotePropertyName "endpoint" -NotePropertyValue $ControllerEndpoint
}

# ConfigDirectory stores the parent directory created before the atomic write.
# ConfigDirectory 保存原子写入前需要创建的父目录。
$ConfigDirectory = Split-Path $EffectiveConfigPath -Parent
New-Item -ItemType Directory -Path $ConfigDirectory -Force | Out-Null

# SerializedConfiguration is stable indented UTF-8 JSON terminated by one newline.
# SerializedConfiguration 是以单个换行结尾的稳定缩进 UTF-8 JSON。
$SerializedConfiguration = ($Configuration | ConvertTo-Json -Depth 32) + [Environment]::NewLine

# TemporaryConfigPath stays beside the destination so the final replacement remains on one filesystem.
# TemporaryConfigPath 与目标文件位于同一目录，确保最终替换发生在同一文件系统。
$TemporaryConfigPath = Join-Path $ConfigDirectory (".controller.{0}.{1}.tmp" -f $PID, [Guid]::NewGuid().ToString("N"))
# BackupConfigPath gives File.Replace an explicit same-directory backup and is deleted after the atomic replacement.
# BackupConfigPath 为 File.Replace 提供显式的同目录备份，并在原子替换后删除。
$BackupConfigPath = Join-Path $ConfigDirectory (".controller.{0}.{1}.bak" -f $PID, [Guid]::NewGuid().ToString("N"))
try {
    # Utf8WithoutBom preserves the repository runtime JSON encoding contract.
    # Utf8WithoutBom 保持仓库运行时 JSON 的无 BOM UTF-8 编码约定。
    $Utf8WithoutBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($TemporaryConfigPath, $SerializedConfiguration, $Utf8WithoutBom)
    if (Test-Path -LiteralPath $EffectiveConfigPath -PathType Leaf) {
        [System.IO.File]::Replace($TemporaryConfigPath, $EffectiveConfigPath, $BackupConfigPath)
    }
    else {
        [System.IO.File]::Move($TemporaryConfigPath, $EffectiveConfigPath)
    }
}
finally {
    if (Test-Path -LiteralPath $TemporaryConfigPath -PathType Leaf) {
        Remove-Item -LiteralPath $TemporaryConfigPath -Force
    }
    if (Test-Path -LiteralPath $BackupConfigPath -PathType Leaf) {
        Remove-Item -LiteralPath $BackupConfigPath -Force
    }
}

Write-Host "==> Controller profile configured: $ControllerEndpoint"
exit 0
