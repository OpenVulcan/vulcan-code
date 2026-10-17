# Share native tar discovery and checked extraction between dependency installers.
# 在依赖安装器间共享原生 tar 查找与带退出码检查的解压逻辑。

# Get-AvailableTarPath resolves the current host's native tar application without requiring Windows names on Unix.
# Get-AvailableTarPath 解析当前宿主的原生 tar 程序，不在 Unix 上要求 Windows 命令名。
# Returns the application path, or throws when the required extractor is unavailable.
# 返回程序路径；缺少所需解压器时抛出异常。
function Get-AvailableTarPath {
    # WindowsHost selects the executable naming contract of the actual operating system.
    # WindowsHost 根据实际操作系统选择可执行程序命名契约。
    $WindowsHost = [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([Runtime.InteropServices.OSPlatform]::Windows)
    if ($WindowsHost) {
        # SystemTar retains the preferred Windows system extractor when it is installed.
        # SystemTar 在 Windows 系统解压器存在时继续优先使用它。
        $SystemTar = Join-Path $env:SystemRoot 'System32/tar.exe'
        if (Test-Path -LiteralPath $SystemTar -PathType Leaf) { return $SystemTar }
    }
    # TarName and TarCommand resolve only an application belonging to this platform's command contract.
    # TarName 与 TarCommand 只解析符合当前平台命令契约的应用程序。
    $TarName = if ($WindowsHost) { 'tar.exe' } else { 'tar' }
    $TarCommand = Get-Command -Name $TarName -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $TarCommand) { throw "$TarName is required to extract dependency archives." }
    return $TarCommand.Source
}

# Expand-DependencyTarArchive extracts one verified gzip tar into an existing directory and rejects native failures.
# Expand-DependencyTarArchive 将一个已校验的 gzip tar 解压到现有目录，并拒绝原生程序失败。
# ArchivePath is the verified archive; DestinationPath is the caller-owned extraction directory.
# ArchivePath 是已校验归档；DestinationPath 是调用方拥有的解压目录。
# Returns no value and throws before callers can publish files or installed markers after a tar failure.
# 无返回值；tar 失败时立即抛出异常，阻止调用方发布文件或安装标记。
function Expand-DependencyTarArchive {
    param(
        [Parameter(Mandatory = $true)][string]$ArchivePath,
        [Parameter(Mandatory = $true)][string]$DestinationPath
    )
    # TarPath is resolved only for tar archives; ZIP-only installation does not require tar.
    # TarPath 只在处理 tar 归档时解析；仅使用 ZIP 的安装不要求 tar。
    $TarPath = Get-AvailableTarPath
    & $TarPath -xzf $ArchivePath -C $DestinationPath
    if ($LASTEXITCODE -ne 0) { throw "Dependency archive extraction failed with exit code ${LASTEXITCODE}: $ArchivePath" }
}
