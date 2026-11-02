# Sign and verify Windows files with the pinned Certum cloud certificate on a disposable runner.
# 在一次性 Runner 上使用固定的 Certum 云证书签名并验证 Windows 文件。
# Requires PowerShell 7 (ProcessStartInfo.ArgumentList). The certificate is pinned by CERTUM_CERT_SHA1 alone.
# 需要 PowerShell 7（ProcessStartInfo.ArgumentList）。证书只由 CERTUM_CERT_SHA1 固定。
param(
    # Phase selects installing SimplySign, signing then verifying, or verifying already signed files.
    # Phase 选择安装 SimplySign、签名后验证，或只验证已签名文件。
    [Parameter(Mandatory)][ValidateSet('Install', 'Sign', 'Verify')][string]$Phase,
    # EvidenceDirectory receives credential-free JSON reports.
    # EvidenceDirectory 接收不含凭据的 JSON 报告。
    [Parameter(Mandatory)][string]$EvidenceDirectory,
    # Files names the exact targets; links and missing paths are rejected rather than resolved.
    # Files 指定精确目标；链接与缺失路径直接拒绝，不做解析。
    [string[]]$Files = @(),
    # Description is shown in the Windows elevation prompt for the signed file.
    # Description 显示在 Windows 提权提示中。
    [string]$Description = 'Vulcan Code'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Invoke a native tool with bounded execution; captured output is never logged automatically.
# 限时运行原生工具，捕获的输出绝不自动写入日志。
function Invoke-BoundedTool {
    param([string]$Executable, [string[]]$Arguments, [int]$TimeoutSeconds = 120)
    $start = [System.Diagnostics.ProcessStartInfo]::new($Executable)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [System.Diagnostics.Process]::Start($start)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    try {
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.Kill($true)
            $process.WaitForExit()
            throw 'native_tool_timeout'
        }
        return [pscustomobject]@{ ExitCode = $process.ExitCode; Stdout = $stdout.Result; Stderr = $stderr.Result }
    } finally {
        $process.Dispose()
    }
}

# Return currently valid Certum code-signing certificates in CurrentUser\My that have a private key.
# 返回当前用户个人存储中有效、带私钥的 Certum 代码签名证书。
function Get-CertumSigningCertificates {
    $now = Get-Date
    return @(Get-ChildItem Cert:\CurrentUser\My | Where-Object {
        $_.HasPrivateKey -and $_.NotBefore -le $now -and $_.NotAfter -gt $now -and
        $_.Issuer -match 'Certum' -and
        @($_.EnhancedKeyUsageList | Where-Object ObjectId -eq '1.3.6.1.5.5.7.3.3').Count -gt 0
    })
}

# These pinned values come from the official Certum download page and verified MSI metadata.
# 以下固定值来自 Certum 官方下载页面和已验证的 MSI 元数据。
$installerUrl = 'https://files.certum.eu/software/SimplySignDesktop/Windows/9.4.4.92/SimplySignDesktop-9.4.4.92-64-bit-en.msi'
$installerSha256 = '8ec420fc27798b86078b7bd02fe7152097e1b3005bab51820eaca8e57df84da3'
$installerSigner = 'AC9C643063CD501E851A7B6A9762E295FBABB012'
# The timestamp server is occasionally unavailable; each file gets this many attempts this far apart.
# 时间戳服务器偶尔不可用；每个文件按此次数与间隔重试。
$signAttempts = 6
$signRetrySeconds = 10
$desktop = Join-Path $env:ProgramFiles 'Certum\SimplySign Desktop\SimplySignDesktop.exe'
$evidence = [IO.Path]::GetFullPath($EvidenceDirectory)
New-Item -ItemType Directory -Path $evidence -Force | Out-Null

if ($Phase -eq 'Install') {
    # Install only the exact official binary whose hash and Authenticode signature were checked.
    # 仅安装哈希与 Authenticode 签名均通过校验的指定官方安装包。
    $installer = Join-Path $env:RUNNER_TEMP 'SimplySignDesktop-9.4.4.92.msi'
    Invoke-WebRequest -Uri $installerUrl -OutFile $installer
    if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne $installerSha256) {
        throw 'simplysign_installer_hash_mismatch'
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $installer
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $installerSigner) {
        throw 'simplysign_installer_signature_invalid'
    }
    $installed = Invoke-BoundedTool -Executable 'msiexec.exe' -Arguments @('/i', $installer, '/qn', '/norestart') -TimeoutSeconds 240
    if ($installed.ExitCode -notin @(0, 3010)) { throw "simplysign_install_failed_$($installed.ExitCode)" }
    if (-not (Test-Path -LiteralPath $desktop -PathType Leaf) -or
        -not (Test-Path -LiteralPath "$env:WINDIR\System32\SimplySignPKCS.dll" -PathType Leaf)) {
        throw 'simplysign_installed_files_missing'
    }
    # The installer starts the client; close it so the later login is the only session.
    # 安装程序会启动客户端；先关闭，使之后的登录成为唯一会话。
    $null = Invoke-BoundedTool -Executable $desktop -Arguments @('/close') -TimeoutSeconds 15
    $closeDeadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $existing = @(Get-Process -Name SimplySignDesktop -ErrorAction SilentlyContinue |
            Where-Object SessionId -eq ([System.Diagnostics.Process]::GetCurrentProcess().SessionId))
        if ($existing.Count -eq 0) { break }
        Start-Sleep -Seconds 1
    } while ([DateTime]::UtcNow -lt $closeDeadline)
    if ($existing.Count -ne 0) { throw 'simplysign_close_before_login_failed' }
    [pscustomobject]@{
        installerSha256 = $installerSha256
        installerSigner = $installerSigner
        version = (Get-Item -LiteralPath $desktop).VersionInfo.FileVersion
        installerExitCode = $installed.ExitCode
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'simplysign-install.json') -Encoding utf8
    Write-Output 'Verified official SimplySign Desktop installation completed'
    exit 0
}

$thumbprint = "$env:CERTUM_CERT_SHA1".Trim().ToUpperInvariant()
if ($thumbprint -notmatch '^[0-9A-F]{40}$') { throw 'CERTUM_CERT_SHA1_must_be_a_sha1_thumbprint' }
if ($Files.Count -eq 0) { throw 'signing_targets_required' }
foreach ($file in $Files) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw 'signing_target_missing' }
    if ((Get-Item -LiteralPath $file).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'signing_target_is_link' }
}
$sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$signTools = @(Get-ChildItem -LiteralPath $sdkRoot -Directory |
    Where-Object Name -match '^\d+\.\d+\.\d+\.\d+$' |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64\signtool.exe' } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf })
if ($signTools.Count -eq 0) { throw 'windows_sdk_signtool_missing' }
$signTool = $signTools[0]

$report = [ordered]@{ kind = 'authenticode'; phase = $Phase; status = 'failed'; stage = 'credentials'; files = @() }
$client = $null
$email = $null
$otp = $null
try {
    if ($Phase -eq 'Sign') {
        # Credentials are confined to one login process; signtool and other children inherit no seed.
        # 凭据只用于一次登录；signtool 等子进程不继承动态口令种子。
        $email = $env:CERTUM_TOTP_EMAIL
        if ([string]::IsNullOrWhiteSpace($email) -or [string]::IsNullOrWhiteSpace($env:CERTUM_TOTP_SECRET)) {
            throw 'certum_credentials_unavailable'
        }
        $report.stage = 'login'
        $otp = & node (Join-Path $PSScriptRoot 'release-signing.mjs') totp
        if ($LASTEXITCODE -ne 0 -or $otp -notmatch '^\d{6}$') { throw 'certum_totp_failed' }
        Write-Output "::add-mask::$otp"
        Remove-Item Env:CERTUM_TOTP_SECRET -ErrorAction SilentlyContinue
        Remove-Item Env:CERTUM_TOTP_EMAIL -ErrorAction SilentlyContinue
        $start = [System.Diagnostics.ProcessStartInfo]::new($desktop)
        $start.UseShellExecute = $false
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        foreach ($argument in @('/autologin', $email, $otp)) { $start.ArgumentList.Add($argument) }
        $client = [System.Diagnostics.Process]::Start($start)
        $null = $client.StandardOutput.ReadToEndAsync()
        $null = $client.StandardError.ReadToEndAsync()
        $email = $null
        $otp = $null

        # Certificate readiness, not process startup, shows that the cloud login succeeded.
        # 以证书就绪而不是进程启动判断云登录成功。
        $deadline = [DateTime]::UtcNow.AddSeconds(90)
        do {
            $certificates = @(Get-CertumSigningCertificates | Where-Object Thumbprint -eq $thumbprint)
            if ($certificates.Count -eq 1) { break }
            Start-Sleep -Seconds 2
        } while ([DateTime]::UtcNow -lt $deadline)
        if ($certificates.Count -ne 1) { throw 'pinned_certum_certificate_not_ready' }
        $report.certificateExpiresUtc = $certificates[0].NotAfter.ToUniversalTime().ToString('o')
    }
    foreach ($file in $Files) {
        if ($Phase -eq 'Sign') {
            $report.stage = 'sign'
            $attempt = 0
            do {
                $attempt++
                $signed = Invoke-BoundedTool -Executable $signTool -Arguments @(
                    'sign', '/sha1', $thumbprint, '/fd', 'SHA256',
                    '/tr', 'http://time.certum.pl', '/td', 'SHA256', '/d', $Description, $file
                ) -TimeoutSeconds 120
                if ($signed.ExitCode -eq 0) { break }
                if ($attempt -lt $signAttempts) {
                    Write-Output "signtool attempt $attempt of $signAttempts failed for $([IO.Path]::GetFileName($file)); retrying in $signRetrySeconds s"
                    Start-Sleep -Seconds $signRetrySeconds
                }
            } while ($attempt -lt $signAttempts)
            if ($signed.ExitCode -ne 0) { throw 'signtool_sign_failed' }
        }
        # Verify the trusted chain, the exact pinned signer and a timestamp before anything is packaged.
        # 打包前验证可信证书链、精确的固定签名者与时间戳。
        $report.stage = 'verify'
        $verified = Invoke-BoundedTool -Executable $signTool -Arguments @('verify', '/pa', '/all', '/tw', $file)
        if ($verified.ExitCode -ne 0) { throw 'signtool_verify_failed' }
        $signature = Get-AuthenticodeSignature -LiteralPath $file
        if ($signature.Status -ne 'Valid' -or $null -eq $signature.TimeStamperCertificate -or
            $signature.SignerCertificate.Thumbprint -ne $thumbprint) {
            throw 'signature_identity_or_timestamp_invalid'
        }
        $report.files += [ordered]@{
            name = [IO.Path]::GetFileName($file)
            sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
            timestampSigner = $signature.TimeStamperCertificate.Thumbprint
        }
    }
    $report.status = 'passed'
    $report.stage = 'complete'
    Write-Output "Certum signatures verified: $($Files.Count)"
} catch {
    # Only stable codes produced here are reported; client diagnostics never leave this script.
    # 只报告本脚本生成的稳定错误码；客户端诊断信息不离开本脚本。
    $message = $_.Exception.Message
    $report.error = if ($message -match '^[A-Za-z0-9_]+$') { $message } else { 'unexpected_certum_signing_error' }
    throw $report.error
} finally {
    Remove-Item Env:CERTUM_TOTP_SECRET -ErrorAction SilentlyContinue
    Remove-Item Env:CERTUM_TOTP_EMAIL -ErrorAction SilentlyContinue
    $email = $null
    $otp = $null
    if ($null -ne $client) {
        try { $null = Invoke-BoundedTool -Executable $desktop -Arguments @('/close') -TimeoutSeconds 10 } catch { }
        if (-not $client.HasExited) { $client.Kill($true) }
        $client.Dispose()
    }
    $report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $evidence "authenticode-$($Phase.ToLowerInvariant()).json") -Encoding utf8
}
