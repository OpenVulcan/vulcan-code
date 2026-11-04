# Downloads the Vulcan Code setup program @@VERSION@@ for Windows x64, checks its SHA-256
# and Authenticode signature, and runs it; the setup program then verifies everything it installs with
# the release signing key. Runs in Windows PowerShell 5.1 and PowerShell 7.
#
#   irm https://static.openvulcan.com/vulcancode/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://static.openvulcan.com/vulcancode/install.ps1))) --mode terminal --yes
#   powershell -ExecutionPolicy Bypass -File install.ps1 --mode terminal --yes
#
# Every argument goes to vulcan-code-setup unchanged; --help lists them. This file is a template:
# scripts/entry-scripts.mjs fills in the version, the origins, the digest and the signer when a setup version
# is published. It stays ASCII so Windows PowerShell 5.1 reads it the same in every code page, and it
# works in a script block so nothing it sets leaks into the session that ran `iex`.

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    $Version = '@@VERSION@@'
    $Origins = @(@@ORIGINS@@)
    $Digest = '@@SHA256_WINDOWS_X64@@'
    $Signer = '@@SIGNER@@'

    # A 32-bit PowerShell on 64-bit Windows reports its own architecture in PROCESSOR_ARCHITECTURE.
    $Architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $Architecture) { $Architecture = $env:PROCESSOR_ARCHITECTURE }
    if ($Architecture -ne 'AMD64') {
        throw "Vulcan Code for Windows is built for x64; this computer reports $Architecture"
    }
    # Windows PowerShell 5.1 may not offer TLS 1.2 by default; it is added, nothing is removed.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $File = "vulcan-code-setup-$Version-windows-x64.exe"
    $Work = Join-Path ([IO.Path]::GetTempPath()) ('vulcan-code-setup-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $Work | Out-Null
    try {
        $Program = Join-Path $Work 'vulcan-code-setup.exe'
        # The origins are tried in order; the mirror serves the same verified objects.
        $Fetched = $false
        foreach ($Origin in $Origins) {
            try {
                Invoke-WebRequest -UseBasicParsing -Uri "$Origin/setup/$Version/$File" -OutFile $Program
                $Fetched = $true
                break
            } catch {
                Write-Warning "Cannot download from $Origin, trying the next origin"
            }
        }
        if (-not $Fetched) { throw "Cannot download $File" }
        $Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Program).Hash.ToLowerInvariant()
        if ($Actual -ne $Digest) {
            throw "$File does not match its published SHA-256 (expected $Digest, got $Actual)"
        }
        if ($Signer) {
            $Signature = Get-AuthenticodeSignature -LiteralPath $Program
            if ($Signature.Status -ne 'Valid' -or $Signature.SignerCertificate.Thumbprint -ne $Signer) {
                throw "$File is not signed by the Vulcan Code certificate ($($Signature.Status))"
            }
        }
        & $Program @args
        if ($LASTEXITCODE -ne 0) { throw "vulcan-code-setup exited with $LASTEXITCODE" }
    } finally {
        Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue
    }
} @args
