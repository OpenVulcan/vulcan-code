# Verify forced Core restart ownership using disposable real Windows processes, never the user's Core.
# 使用可丢弃的真实 Windows 进程验证强制核心重启归属，绝不操作用户核心进程。
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'distribution_processes.ps1')

# ScriptPath identifies the production functions; AST extraction avoids running its Cargo and publication steps.
# ScriptPath 标识生产函数；通过语法树提取避免执行其中的编译及发布步骤。
$ScriptPath = Join-Path $PSScriptRoot 'core_debug_cycle.ps1'
$ParseTokens = $null
$ParseErrors = $null
$Ast = [Management.Automation.Language.Parser]::ParseFile($ScriptPath, [ref]$ParseTokens, [ref]$ParseErrors)
if ($ParseErrors.Count -ne 0) { throw ($ParseErrors | Out-String) }
foreach ($Name in @('Get-RunningOutputCore', 'Stop-OutputCoreForRestart', 'Assert-NoForeignCoreOwner')) {
    # Function is selected by exact AST identity, without evaluating top-level production code.
    # Function 通过精确语法树身份选择，不执行生产顶层代码。
    $Function = $Ast.Find({ param($Node) $Node -is [Management.Automation.Language.FunctionDefinitionAst] -and $Node.Name -eq $Name }, $true)
    if ($null -eq $Function) { throw "Missing production function: $Name" }
    . ([scriptblock]::Create($Function.Extent.Text))
}

# TestRoot is a unique temporary directory; every copied executable and process belongs to this test.
# TestRoot 是唯一临时目录；全部复制程序与进程归本测试所有。
$TemporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$TestRoot = Join-Path $TemporaryRoot ('vulcan-core-restart-test-' + [Guid]::NewGuid().ToString('N'))
$OwnedProcess = $null
$ForeignProcess = $null
try {
    New-Item -ItemType Directory -Path (Join-Path $TestRoot 'owned'), (Join-Path $TestRoot 'foreign') | Out-Null
    # Ping is an OS-owned standalone executable renamed only inside the test's private directories.
    # Ping 是操作系统提供的独立程序，仅在测试私有目录中重命名。
    $OutputCorePath = Join-Path $TestRoot 'owned/vulcan-code-service.exe'
    $ForeignPath = Join-Path $TestRoot 'foreign/vulcan-code-service.exe'
    Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32/ping.exe') -Destination $OutputCorePath
    Copy-Item -LiteralPath $OutputCorePath -Destination $ForeignPath
    $OwnedProcess = Start-Process -FilePath $OutputCorePath -ArgumentList '-t', '127.0.0.1' -WindowStyle Hidden -PassThru
    $ForeignProcess = Start-Process -FilePath $ForeignPath -ArgumentList '-t', '127.0.0.1' -WindowStyle Hidden -PassThru
    # Discovery intentionally names a different same-name executable, which must block restart admission.
    # 发现记录刻意指向同名但不同路径的可执行程序，必须阻止重启准入。
    $ExistingDescription = @{ discoveryPath = (Join-Path $TestRoot 'discovery.json') }
    @{ pid = $ForeignProcess.Id } | ConvertTo-Json | Set-Content -LiteralPath $ExistingDescription.discoveryPath -Encoding UTF8
    $Rejected = $false
    try { Assert-NoForeignCoreOwner } catch {
        if ($_.Exception.Message -notlike 'A different Core executable owns*') { throw }
        $Rejected = $true
    }
    if (-not $Rejected) { throw 'Foreign discovery owner was accepted.' }
    @{ pid = $OwnedProcess.Id } | ConvertTo-Json | Set-Content -LiteralPath $ExistingDescription.discoveryPath -Encoding UTF8
    Assert-NoForeignCoreOwner
    # The selected process list must contain only the disposable exact-path owner.
    # 选中进程列表必须只包含可丢弃且路径精确匹配的持有者。
    $Selected = @(Get-RunningOutputCore)
    if ($Selected.Count -ne 1 -or $Selected[0].Id -ne $OwnedProcess.Id) { throw 'Exact-path process selection failed.' }
    Stop-OutputCoreForRestart
    if (-not $OwnedProcess.WaitForExit(1000)) { throw 'Owned process remained alive.' }
    $ForeignProcess.Refresh()
    if ($ForeignProcess.HasExited) { throw 'Foreign same-name process was terminated.' }
    Stop-OutputCoreForRestart
    Write-Host 'PASS: script syntax, foreign owner rejection, exact-path forced stop, peer preservation, idle restart.'
}
finally {
    # Cleanup only the two process objects created above, then the verified test-owned directory.
    # 仅清理上面创建的两个进程对象，再清理已验证归属的测试目录。
    foreach ($Process in @($OwnedProcess, $ForeignProcess)) {
        if ($null -ne $Process) {
            $Process.Refresh()
            if (-not $Process.HasExited) { Stop-Process -InputObject $Process -Force; $Process.WaitForExit() }
            $Process.Dispose()
        }
    }
    $ResolvedTestRoot = [IO.Path]::GetFullPath($TestRoot)
    if (-not $ResolvedTestRoot.StartsWith($TemporaryRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing cleanup outside temporary root: $ResolvedTestRoot"
    }
    if (Test-Path -LiteralPath $ResolvedTestRoot) { Remove-Item -LiteralPath $ResolvedTestRoot -Recurse -Force }
}
