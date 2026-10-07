# Mark the compilation units of each successful make build and sweep every other unit from its Cargo profile.
# 标记每次成功 make 构建用到的编译单元，并从其 Cargo 模式目录清除其余全部单元。

# UnitPattern matches Cargo's 16-hex unit hash in deps files and in fingerprint and build directories.
# UnitPattern 匹配 deps 文件及 fingerprint、build 目录名中 Cargo 的 16 位十六进制单元哈希。
$script:UnitPattern = '^(?<name>.+?)-(?<hash>[0-9a-f]{16})(?:\..+)?$'
# CargoLockNames are the profile locks Cargo takes while it reads or writes the profile directory.
# CargoLockNames 是 Cargo 读写模式目录时持有的模式锁。
$script:CargoLockNames = @('.cargo-lock', '.cargo-build-lock', '.cargo-artifact-lock')
# CargoLockTimeout bounds how long cleanup waits for other Cargo commands to release the profile.
# CargoLockTimeout 限定清理等待其他 Cargo 命令释放模式目录的时长。
$script:CargoLockTimeout = [TimeSpan]::FromMinutes(10)

# Return the unit hash in a deps file or unit directory name, or $null when the name carries none.
# 返回 deps 文件或单元目录名中的单元哈希；名称不含哈希时返回 $null。
function Get-CargoUnitHash {
    param([string]$Name)
    if ($Name -match $script:UnitPattern) { return $Matches.hash }
    return $null
}

# Return the unit hash of the binary compilation that produced Executable, or $null when none matches.
# 返回产生 Executable 的二进制编译单元哈希；无匹配时返回 $null。
# Each compilation stamps invoked.timestamp before rustc starts; Cargo's profile lock keeps compilations
# of one binary apart, so the latest stamp no later than the executable's write time is its unit.
# 每次编译在 rustc 启动前写入 invoked.timestamp；Cargo 模式锁使同一二进制的编译互不重叠，因此不晚于
# 可执行文件写入时间的最新时间戳即为其单元。
function Find-CargoBinaryUnit {
    param([string]$FingerprintRoot, [string]$Target, [string]$Executable)
    $Written = [IO.File]::GetLastWriteTimeUtc($Executable)
    $Best = $null
    $BestStarted = [DateTime]::MinValue
    foreach ($Directory in [IO.Directory]::EnumerateDirectories($FingerprintRoot)) {
        $Hash = Get-CargoUnitHash ([IO.Path]::GetFileName($Directory))
        $Stamp = Join-Path $Directory 'invoked.timestamp'
        if ($null -eq $Hash -or -not [IO.File]::Exists((Join-Path $Directory "bin-$Target")) -or -not [IO.File]::Exists($Stamp)) { continue }
        $Started = [IO.File]::GetLastWriteTimeUtc($Stamp)
        if ($Started -le $Written -and $Started -gt $BestStarted) { $Best = $Hash; $BestStarted = $Started }
    }
    return $Best
}

# Hold every Cargo profile lock so no Cargo command reads or writes the profile while it is swept.
# 持有全部 Cargo 模式锁，使清理期间没有 Cargo 命令读写该模式目录。
# Locks are taken without blocking and all released on any miss, so cleanup never deadlocks with Cargo.
# 锁以非阻塞方式获取，任一失败即全部释放，因此清理不会与 Cargo 互相死锁。
# Returns the open lock streams; the caller disposes them. Throws when the timeout passes.
# 返回已打开的锁流，由调用方释放；超时抛出异常。
function Lock-CargoProfile {
    param([string]$ProfileRoot)
    $Deadline = [DateTime]::UtcNow + $script:CargoLockTimeout
    while ($true) {
        $Streams = [Collections.Generic.List[IO.FileStream]]::new()
        try {
            foreach ($LockName in $script:CargoLockNames) {
                $Stream = [IO.File]::Open((Join-Path $ProfileRoot $LockName), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete)
                $Streams.Add($Stream)
                $Stream.Lock(0, [long]::MaxValue)
            }
            return $Streams.ToArray()
        }
        catch [IO.IOException] {
            foreach ($Stream in $Streams) { $Stream.Dispose() }
            if ([DateTime]::UtcNow -gt $Deadline) { throw "Timed out waiting for other Cargo commands to release $ProfileRoot." }
            Start-Sleep -Milliseconds 500
        }
    }
}

# Remove one file or directory tree and return the bytes it held; failures warn and return 0.
# 删除一个文件或目录树并返回其占用字节；失败时发出警告并返回 0。
function Remove-CargoUnitPath {
    param([string]$Path)
    try {
        if ([IO.File]::Exists($Path)) {
            $Bytes = ([IO.FileInfo]::new($Path)).Length
            [IO.File]::Delete($Path)
            return $Bytes
        }
        # Options keep size accounting inside the tree; Cargo unit directories never hold links.
        # Options 使大小统计停留在目录树内；Cargo 单元目录从不包含链接。
        $Options = [IO.EnumerationOptions]::new()
        $Options.RecurseSubdirectories = $true
        $Options.AttributesToSkip = [IO.FileAttributes]::ReparsePoint
        $Bytes = 0L
        foreach ($File in [IO.DirectoryInfo]::new($Path).EnumerateFiles('*', $Options)) { $Bytes += $File.Length }
        [IO.Directory]::Delete($Path, $true)
        return $Bytes
    }
    catch {
        Write-Warning "Could not remove unused Cargo artifact '$Path': $_"
        return 0L
    }
}

# Sweep one Cargo profile down to the units the last successful build used.
# 将一个 Cargo 模式目录清理到最近一次成功构建用到的单元。
# ProfileDirectory is the executable's parent; Artifacts holds every compiler-artifact, fresh ones included;
# ProfileDirectory 是可执行文件的父目录；Artifacts 包含全部 compiler-artifact，含复用制品；
# OutDirs holds every build-script-executed out_dir; BuildStartedUtc is when Cargo started. Returns no value.
# OutDirs 包含全部 build-script-executed 的 out_dir；BuildStartedUtc 为 Cargo 启动时间。无返回值。
function Invoke-CargoArtifactSweep {
    param([string]$ProfileDirectory, [object[]]$Artifacts, [string[]]$OutDirs, [DateTime]$BuildStartedUtc)

    $ProfileRoot = [IO.Path]::GetFullPath($ProfileDirectory)
    $DepsRoot = Join-Path $ProfileRoot 'deps'
    $FingerprintRoot = Join-Path $ProfileRoot '.fingerprint'
    $BuildRoot = Join-Path $ProfileRoot 'build'
    foreach ($Root in @($ProfileRoot, $DepsRoot, $FingerprintRoot, $BuildRoot, (Join-Path $ProfileRoot 'incremental'))) {
        if ((Test-Path -LiteralPath $Root) -and ((Get-Item -LiteralPath $Root -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Refusing artifact cleanup through a reparse point: $Root"
        }
    }

    # Live holds the hash of every unit this build used; Built and Reused hold crate names.
    # Live 保存本次构建用到的每个单元哈希；Built 与 Reused 保存 crate 名称。
    $Live = [Collections.Generic.HashSet[string]]::new()
    $Built = [Collections.Generic.HashSet[string]]::new()
    $Reused = [Collections.Generic.HashSet[string]]::new()
    foreach ($Artifact in $Artifacts) {
        $CrateName = ([string]$Artifact.target.name).Replace('-', '_')
        [void]$Built.Add($CrateName)
        if ($Artifact.fresh) { [void]$Reused.Add($CrateName) }
        foreach ($Filename in $Artifact.filenames) {
            $Hash = Get-CargoUnitHash ([IO.Path]::GetFileName([string]$Filename))
            if ($null -ne $Hash) { [void]$Live.Add($Hash); continue }
            # A compiled build script lives in its unit directory under build, which carries the hash.
            # 编译后的构建脚本位于 build 下自身的单元目录中，由该目录携带哈希。
            $Parent = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath([string]$Filename))
            if ([string]::Equals([IO.Path]::GetDirectoryName($Parent), $BuildRoot, [StringComparison]::OrdinalIgnoreCase)) {
                $Hash = Get-CargoUnitHash ([IO.Path]::GetFileName($Parent))
                if ($null -ne $Hash) { [void]$Live.Add($Hash); continue }
            }
            if ([string]$Filename -ne [string]$Artifact.executable) { continue }
            # Cargo names binaries without a unit hash on some targets, so the unit is found through its
            # fingerprint: the latest compilation of this binary that started before the executable was written.
            # Cargo 在部分目标上为二进制命名时不带单元哈希，因此通过指纹找到单元：即在可执行文件写入前开始的、
            # 该二进制最近一次编译。
            $Unit = Find-CargoBinaryUnit -FingerprintRoot $FingerprintRoot -Target ([string]$Artifact.target.name) -Executable ([string]$Filename)
            if ($null -eq $Unit) {
                Write-Warning "Artifact cleanup skipped: cannot identify the unit behind $Filename."
                return
            }
            [void]$Live.Add($Unit)
        }
    }
    foreach ($OutDir in $OutDirs) {
        $Hash = Get-CargoUnitHash ([IO.Path]::GetFileName([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($OutDir))))
        if ($null -ne $Hash) { [void]$Live.Add($Hash) }
    }

    # Removed counts entries and bytes per area for the summary line.
    # Removed 按区域统计条目数与字节数，用于汇总输出。
    $Removed = [ordered]@{}
    $Account = {
        param([string]$Area, [string]$Path)
        if (-not $Removed.Contains($Area)) { $Removed[$Area] = @(0, 0L) }
        $Removed[$Area] = @(($Removed[$Area][0] + 1), ($Removed[$Area][1] + (Remove-CargoUnitPath $Path)))
    }
    if (Test-Path -LiteralPath $DepsRoot) {
        foreach ($Entry in [IO.Directory]::EnumerateFiles($DepsRoot)) {
            $Hash = Get-CargoUnitHash ([IO.Path]::GetFileName($Entry))
            if ($null -ne $Hash -and -not $Live.Contains($Hash)) { & $Account 'deps' $Entry }
        }
    }
    foreach ($Area in @('.fingerprint', 'build')) {
        $AreaRoot = Join-Path $ProfileRoot $Area
        if (-not (Test-Path -LiteralPath $AreaRoot)) { continue }
        foreach ($Entry in [IO.Directory]::EnumerateDirectories($AreaRoot)) {
            $Hash = Get-CargoUnitHash ([IO.Path]::GetFileName($Entry))
            if ($null -ne $Hash -and -not $Live.Contains($Hash)) { & $Account $Area $Entry }
        }
    }
    # Rustc names incremental directories by crate with its own hash: crates this build did not use go
    # entirely, and crates whose every unit it recompiled keep only the directories written during this
    # build. A crate name shared by a reused unit, such as a fresh library beside a rebuilt binary, keeps all.
    # rustc 以 crate 名加自有哈希命名增量目录：本次构建未用到的 crate 全部删除，全部单元都重新编译的 crate
    # 只保留本次构建期间写入的目录。与复用单元同名的 crate（如重建的二进制旁未变的库）全部保留。
    $IncrementalRoot = Join-Path $ProfileRoot 'incremental'
    if (Test-Path -LiteralPath $IncrementalRoot) {
        foreach ($Entry in [IO.DirectoryInfo]::new($IncrementalRoot).EnumerateDirectories()) {
            $Separator = $Entry.Name.LastIndexOf('-')
            if ($Separator -le 0) { continue }
            $CrateName = $Entry.Name.Substring(0, $Separator)
            if (-not $Built.Contains($CrateName)) { & $Account 'incremental' $Entry.FullName; continue }
            if ($Reused.Contains($CrateName)) { continue }
            $Newest = @($Entry) + @($Entry.EnumerateFileSystemInfos()) | ForEach-Object { $_.LastWriteTimeUtc } | Measure-Object -Maximum
            if ($Newest.Maximum -lt $BuildStartedUtc) { & $Account 'incremental' $Entry.FullName }
        }
    }

    $Summary = if ($Removed.Count -eq 0) { 'nothing unused' }
    else { ($Removed.Keys | ForEach-Object { '{0} {1} ({2:N1} GB)' -f $_, $Removed[$_][0], ($Removed[$_][1] / 1GB) }) -join ', ' }
    Write-Host "==> Artifact cleanup ($([IO.Path]::GetFileName($ProfileRoot))): $Summary."
}

# Execute Cargo with structured output and, after success, sweep the profile down to the units it used.
# 使用结构化输出执行 Cargo，成功后将模式目录清理到本次用到的单元。
# BuildArguments is the existing make build argument list; returns ExitCode and compiler Artifacts.
# BuildArguments 是现有 make 构建参数列表；返回 ExitCode 和编译器 Artifacts。
function Invoke-TrackedCargoBuild {
    param([string[]]$BuildArguments)
    # Metadata supplies the authoritative target directory, including environment overrides.
    # Metadata 提供权威 target 目录，包括环境变量覆盖。
    $MetadataJson = & cargo metadata --format-version 1 --no-deps
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed before tracked build.' }
    $Metadata = $MetadataJson | ConvertFrom-Json
    # TargetRoot is canonicalized before opening the shared make build lock.
    # TargetRoot 在打开共享 make 构建锁前进行规范化。
    $TargetRoot = [IO.Path]::GetFullPath([string]$Metadata.target_directory)
    New-Item -ItemType Directory -Path $TargetRoot -Force | Out-Null
    # BuildLock serializes make builds and cleanup across both debug and release.
    # BuildLock 将 debug 和 release 的 make 构建及清理串行化。
    $BuildLock = [IO.File]::Open((Join-Path $TargetRoot '.vulcan-make.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        # Artifacts includes fresh records so reused units stay marked.
        # Artifacts 包含 fresh 记录，使复用的单元保持标记。
        $Artifacts = [Collections.Generic.List[object]]::new()
        # OutDirs marks the build-script run units, which compile nothing of their own.
        # OutDirs 标记构建脚本运行单元，它们自身不产生编译制品。
        $OutDirs = [Collections.Generic.List[string]]::new()
        # BuildFinished requires Cargo's explicit successful completion before cleanup.
        # BuildFinished 要求 Cargo 明确成功结束后才能清理。
        $BuildFinished = $false
        # ProtocolValid prevents a partially parsed output stream from authorizing deletion.
        # ProtocolValid 防止仅部分解析成功的输出流授权删除。
        $ProtocolValid = $true
        # BuildStartedUtc separates incremental directories written by this build from older ones.
        # BuildStartedUtc 区分本次构建写入的增量目录与更早的目录。
        $BuildStartedUtc = [DateTime]::UtcNow
        & cargo @BuildArguments --message-format=json | ForEach-Object {
            # Line is one stdout record; Cargo permits non-JSON output from procedural macros.
            # Line 是一条标准输出记录；Cargo 允许过程宏输出非 JSON 文本。
            $Line = [string]$_
            if (-not $Line.StartsWith('{')) { Write-Host $Line; return }
            # Message follows Cargo's documented reason-tagged JSON protocol.
            # Message 遵循 Cargo 文档规定的 reason 标签 JSON 协议。
            $Message = $null
            try { $Message = $Line | ConvertFrom-Json -ErrorAction Stop }
            catch { $ProtocolValid = $false; Write-Host $Line; return }
            switch ($Message.reason) {
                'compiler-artifact' { $Artifacts.Add($Message) }
                'compiler-message' { Write-Host $Message.message.rendered }
                'build-finished' { $BuildFinished = [bool]$Message.success }
                'build-script-executed' { $OutDirs.Add([string]$Message.out_dir) }
                default { Write-Host $Line }
            }
        }
        # BuildExitCode is captured before any subsequent command can replace it.
        # BuildExitCode 在后续命令可能覆盖退出码之前进行捕获。
        $BuildExitCode = $LASTEXITCODE
        if ($BuildExitCode -eq 0 -and $BuildFinished) {
            # ShellArtifacts identifies the exact main executable through Cargo target metadata.
            # ShellArtifacts 通过 Cargo 目标元数据识别精确的主程序可执行文件。
            $ShellArtifacts = @($Artifacts | Where-Object { $_.target.name -eq 'vulcan-code-service' -and $_.target.kind -contains 'bin' })
            if ($ShellArtifacts.Count -ne 1 -or [string]::IsNullOrWhiteSpace($ShellArtifacts[0].executable)) { throw 'Cargo did not report exactly one Tauri executable.' }
            try {
                if (-not $ProtocolValid) { throw 'Cargo output contained malformed JSON; nothing was removed.' }
                $ProfileDirectory = [IO.Path]::GetDirectoryName($ShellArtifacts[0].executable)
                $CargoLocks = Lock-CargoProfile -ProfileRoot $ProfileDirectory
                try { Invoke-CargoArtifactSweep -ProfileDirectory $ProfileDirectory -Artifacts $Artifacts.ToArray() -OutDirs $OutDirs.ToArray() -BuildStartedUtc $BuildStartedUtc }
                finally { foreach ($Lock in $CargoLocks) { $Lock.Dispose() } }
            }
            catch { Write-Warning "Build succeeded, but artifact cleanup was skipped or incomplete: $_" }
        }
        elseif ($BuildExitCode -eq 0) { throw 'Cargo did not report a successful build-finished event; cleanup skipped.' }
        return @{ ExitCode = $BuildExitCode; Artifacts = $Artifacts.ToArray() }
    }
    finally { $BuildLock.Dispose() }
}
