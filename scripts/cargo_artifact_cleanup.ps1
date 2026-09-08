# Track Cargo-reported files across successful make builds without scanning internal caches.
# 跨成功的 make 构建跟踪 Cargo 报告的文件，不扫描内部缓存。

# Update the successful artifact baseline and delete retired files from this profile's deps directory.
# 更新成功构建的制品基线，并删除此模式 deps 目录中退役的文件。
# ProfileDirectory is the executable's parent; Artifacts contains every compiler-artifact, including fresh ones.
# ProfileDirectory 是可执行文件的父目录；Artifacts 包含全部 compiler-artifact，包括复用制品。
# TargetDirectory contains the shared make manifests; callers must hold its make lock. Returns no value.
# TargetDirectory 保存共享 make 清单；调用方必须持有其 make 锁。此函数无返回值。
function Update-CargoArtifactBaseline {
    param([string]$ProfileDirectory, [object[]]$Artifacts, [string]$TargetDirectory)

    # Comparison follows the host filesystem's normal path case convention.
    # Comparison 遵循宿主文件系统通常的路径大小写约定。
    $Comparison = if ([System.IO.Path]::DirectorySeparatorChar -eq '\') { [StringComparer]::OrdinalIgnoreCase } else { [StringComparer]::Ordinal }
    # CurrentFiles retains all reported paths so fresh dependencies and other profiles remain protected.
    # CurrentFiles 保留全部报告路径，保护复用依赖和其他模式的制品。
    $CurrentFiles = [Collections.Generic.HashSet[string]]::new($Comparison)
    foreach ($Artifact in $Artifacts) {
        foreach ($Filename in $Artifact.filenames) {
            [void]$CurrentFiles.Add([IO.Path]::GetFullPath($Filename))
        }
    }
    # ProfileRoot and DepsRoot constrain deletion to direct files in the actual executable profile.
    # ProfileRoot 和 DepsRoot 将删除限定为实际可执行文件模式下的直接制品文件。
    $ProfileRoot = [IO.Path]::GetFullPath($ProfileDirectory)
    $DepsRoot = Join-Path $ProfileRoot 'deps'
    # StateRoot isolates this helper's versioned manifests from Cargo-owned state.
    # StateRoot 将本工具的版本化清单与 Cargo 自有状态隔离。
    $StateRoot = Join-Path $TargetDirectory '.vulcan-make-artifacts'
    New-Item -ItemType Directory -Path $StateRoot -Force | Out-Null
    # ProfileHash identifies the complete path, including an explicit target triple.
    # ProfileHash 标识完整路径，包括显式目标三元组。
    $Hasher = [Security.Cryptography.SHA256]::Create()
    try { $ProfileHash = [BitConverter]::ToString($Hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($ProfileRoot))).Replace('-', '') }
    finally { $Hasher.Dispose() }
    # ManifestPath stores only the most recent successful make build for this profile path.
    # ManifestPath 仅保存此模式路径最近一次成功的 make 构建。
    $ManifestPath = Join-Path $StateRoot "$ProfileHash.json"
    # PreviousFiles contains only previously recorded file identities, never inferred filename matches.
    # PreviousFiles 仅包含先前记录的文件身份，不根据文件名推测匹配。
    $PreviousFiles = @()
    if (Test-Path -LiteralPath $ManifestPath) {
        # PreviousState must match this schema and profile before it can authorize deletion.
        # PreviousState 必须匹配本版本结构和模式才能授权删除。
        $PreviousState = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
        if ($PreviousState.version -ne 1 -or -not $Comparison.Equals($PreviousState.profile, $ProfileRoot)) {
            throw "Artifact baseline schema or profile mismatch: $ManifestPath"
        }
        $PreviousFiles = @($PreviousState.files)
    }
    # ProtectedFiles includes the other successful make profiles, including shared host dependencies.
    # ProtectedFiles 包含其他成功 make 模式的文件，包括共享宿主依赖。
    $ProtectedFiles = [Collections.Generic.HashSet[string]]::new($CurrentFiles, $Comparison)
    foreach ($OtherManifest in Get-ChildItem -LiteralPath $StateRoot -Filter '*.json' -File) {
        if ($Comparison.Equals($OtherManifest.FullName, $ManifestPath)) { continue }
        # OtherState is validated before any destructive work begins.
        # OtherState 在任何删除操作开始前进行校验。
        $OtherState = Get-Content -LiteralPath $OtherManifest.FullName -Raw | ConvertFrom-Json
        if ($OtherState.version -ne 1) { throw "Unsupported artifact baseline: $($OtherManifest.FullName)" }
        foreach ($ProtectedFile in $OtherState.files) { [void]$ProtectedFiles.Add([string]$ProtectedFile.path) }
    }
    # NextFiles records identity metadata so externally rebuilt files are not deleted as stale.
    # NextFiles 记录身份元数据，避免将被外部构建更新的文件当作旧文件删除。
    $NextFiles = [Collections.Generic.List[object]]::new()
    foreach ($Filename in $CurrentFiles) {
        if (Test-Path -LiteralPath $Filename -PathType Leaf) {
            # FileInfo captures the successful build's exact size and write time.
            # FileInfo 捕获成功构建后的精确文件大小和写入时间。
            $FileInfo = Get-Item -LiteralPath $Filename
            $NextFiles.Add(@{ path = $Filename; length = $FileInfo.Length; ticks = $FileInfo.LastWriteTimeUtc.Ticks })
        }
    }
    # RemovedBytes and RemovedCount report only completed file deletions.
    # RemovedBytes 和 RemovedCount 仅统计已完成的文件删除。
    $RemovedBytes = 0L
    $RemovedCount = 0
    foreach ($PreviousFile in $PreviousFiles) {
        # RetiredPath must be a direct child of deps and absent from all retained build manifests.
        # RetiredPath 必须是 deps 的直接子文件，且不在任何保留构建清单中。
        $RetiredPath = [IO.Path]::GetFullPath([string]$PreviousFile.path)
        if ($ProtectedFiles.Contains($RetiredPath) -or -not $Comparison.Equals([IO.Path]::GetDirectoryName($RetiredPath), $DepsRoot)) { continue }
        if (-not (Test-Path -LiteralPath $RetiredPath -PathType Leaf)) { continue }
        # Ancestor rejects junctions and symbolic links before touching a deletion candidate.
        # Ancestor 在处理待删除文件前排除联接目录和符号链接。
        $Ancestor = Get-Item -LiteralPath $RetiredPath -Force
        while ($null -ne $Ancestor) {
            if (($Ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Refusing artifact cleanup through a reparse point: $($Ancestor.FullName)" }
            $Ancestor = if ($Ancestor -is [IO.FileInfo]) { $Ancestor.Directory } else { $Ancestor.Parent }
        }
        # RetiredInfo must still match the recorded identity; changed files belong to another build.
        # RetiredInfo 必须仍匹配记录身份；已变化文件属于其他构建。
        $RetiredInfo = Get-Item -LiteralPath $RetiredPath
        if ($RetiredInfo.Length -ne $PreviousFile.length -or $RetiredInfo.LastWriteTimeUtc.Ticks -ne $PreviousFile.ticks) { continue }
        try {
            Remove-Item -LiteralPath $RetiredPath -Force -ErrorAction Stop
            $RemovedBytes += $RetiredInfo.Length
            $RemovedCount++
        }
        catch {
            # Retain failed candidates so a later successful build retries them.
            # 保留失败候选，使后续成功构建可以重试。
            $NextFiles.Add($PreviousFile)
            Write-Warning "Could not remove retired artifact '$RetiredPath': $_"
        }
    }
    # TemporaryManifest prevents partial JSON from becoming the next deletion baseline.
    # TemporaryManifest 防止不完整 JSON 成为下一次删除基线。
    $TemporaryManifest = "$ManifestPath.tmp"
    @{ version = 1; profile = $ProfileRoot; files = @($NextFiles.ToArray()) } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $TemporaryManifest -Encoding UTF8
    Move-Item -LiteralPath $TemporaryManifest -Destination $ManifestPath -Force
    Write-Host "==> Artifact cleanup ($([IO.Path]::GetFileName($ProfileRoot))): $RemovedCount files, $RemovedBytes bytes removed."
}

# Execute Cargo with structured output and clean tracked retired artifacts only after success.
# 使用结构化输出执行 Cargo，仅在成功后清理已跟踪的退役制品。
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
        # Artifacts includes fresh records so incremental cache hits remain in the baseline.
        # Artifacts 包含 fresh 记录，使增量缓存命中仍被保留在基线中。
        $Artifacts = [Collections.Generic.List[object]]::new()
        # BuildFinished requires Cargo's explicit successful completion before cleanup.
        # BuildFinished 要求 Cargo 明确成功结束后才能清理。
        $BuildFinished = $false
        # ProtocolValid prevents a partially parsed output stream from authorizing deletion.
        # ProtocolValid 防止仅部分解析成功的输出流授权删除。
        $ProtocolValid = $true
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
                'build-script-executed' { }
                default { Write-Host $Line }
            }
        }
        # BuildExitCode is captured before any subsequent command can replace it.
        # BuildExitCode 在后续命令可能覆盖退出码之前进行捕获。
        $BuildExitCode = $LASTEXITCODE
        if ($BuildExitCode -eq 0 -and $BuildFinished) {
            # ShellArtifacts identifies the exact main executable through Cargo target metadata.
            # ShellArtifacts 通过 Cargo 目标元数据识别精确的主程序可执行文件。
            $ShellArtifacts = @($Artifacts | Where-Object { $_.target.name -eq 'vulcan-code-tauri' -and $_.target.kind -contains 'bin' })
            if ($ShellArtifacts.Count -ne 1 -or [string]::IsNullOrWhiteSpace($ShellArtifacts[0].executable)) { throw 'Cargo did not report exactly one Tauri executable.' }
            try {
                if (-not $ProtocolValid) { throw 'Cargo output contained malformed JSON; preserving the previous artifact baseline.' }
                Update-CargoArtifactBaseline -ProfileDirectory ([IO.Path]::GetDirectoryName($ShellArtifacts[0].executable)) -Artifacts $Artifacts.ToArray() -TargetDirectory $TargetRoot
            }
            catch { Write-Warning "Build succeeded, but artifact cleanup was skipped or incomplete: $_" }
        }
        elseif ($BuildExitCode -eq 0) { throw 'Cargo did not report a successful build-finished event; cleanup skipped.' }
        return @{ ExitCode = $BuildExitCode; Artifacts = $Artifacts.ToArray() }
    }
    finally { $BuildLock.Dispose() }
}
