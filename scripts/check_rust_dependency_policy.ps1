#requires -Version 7.0

# check_rust_dependency_policy.ps1 verifies the repository-owned Rust dependency policy without mutating manifests or the lockfile.
# check_rust_dependency_policy.ps1 用于验证仓库持有的 Rust 依赖政策，且不会修改清单或锁文件。

[CmdletBinding()]
param()

# ErrorActionPreference makes every policy or child-command failure terminate the verification.
# ErrorActionPreference 使每个政策或子命令失败都会终止验证。
$ErrorActionPreference = "Stop"

# ScriptDir stores the stable scripts directory resolved from this file.
# ScriptDir 保存从当前文件解析出的稳定脚本目录。
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# ProjectDir stores the workspace root containing the authoritative Cargo manifest.
# ProjectDir 保存包含权威 Cargo 清单的工作区根目录。
$ProjectDir = Split-Path -Parent $ScriptDir

# RtkCommand resolves the mandatory repository shell-command proxy.
# RtkCommand 解析仓库强制使用的 Shell 命令代理。
$script:RtkCommand = Get-Command "rtk" -ErrorAction SilentlyContinue
if (-not $script:RtkCommand) {
    throw "rtk is required to verify Rust dependency policy. Run make deps tools first."
}

# Invoke-RtkChecked executes one RTK-prefixed command and fails closed on a non-zero exit code.
# Invoke-RtkChecked 执行一条带 RTK 前缀的命令，并在非零退出码时关闭失败。
# Arguments contains the exact RTK argument vector and CaptureOutput selects whether stdout is returned to the caller.
# Arguments 包含精确的 RTK 参数向量；CaptureOutput 用于选择是否把标准输出返回调用方。
# Returns captured command output when requested; otherwise returns no value.
# 请求捕获时返回命令输出；否则不返回值。
function Invoke-RtkChecked {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [switch]$CaptureOutput
    )

    Write-Host ("==> rtk {0}" -f ($Arguments -join " "))
    if ($CaptureOutput) {
        # CommandOutput stores the complete stdout required by structured callers such as Cargo metadata parsing.
        # CommandOutput 保存 Cargo 元数据解析等结构化调用方需要的完整标准输出。
        $CommandOutput = (& $script:RtkCommand.Source @Arguments | Out-String)
        if ($LASTEXITCODE -ne 0) {
            throw "Rust dependency verification command failed: rtk $($Arguments -join ' ')"
        }
        return $CommandOutput
    }

    & $script:RtkCommand.Source @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Rust dependency verification command failed: rtk $($Arguments -join ' ')"
    }
}

# Test-IsDependencySection identifies Cargo dependency sections whose declarations must inherit workspace authority.
# Test-IsDependencySection 识别依赖声明必须继承工作区权威的 Cargo 依赖区段。
# SectionName is the normalized TOML header text without surrounding brackets.
# SectionName 是移除外围方括号后的规范 TOML 区段文本。
# Returns true for normal, development, build, and target-specific dependency sections.
# 对普通、开发、构建和目标特定依赖区段返回 true。
function Test-IsDependencySection {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SectionName
    )

    if ($SectionName -eq "dependencies" -or
        $SectionName -eq "dev-dependencies" -or
        $SectionName -eq "build-dependencies") {
        return $true
    }

    return $SectionName -match '^target\..+\.(dependencies|dev-dependencies|build-dependencies)$'
}

# Get-ManifestDependencyPolicyViolations finds member declarations that bypass workspace dependency authority.
# Get-ManifestDependencyPolicyViolations 查找绕过工作区依赖权威的成员声明。
# ManifestPath is one Cargo member manifest validated by cargo metadata before this textual policy check.
# ManifestPath 是在文本政策检查前已经由 cargo metadata 验证过的成员 Cargo 清单。
# Returns human-readable violations containing exact file and line ownership.
# 返回包含精确文件与行归属的人类可读违规记录。
function Get-ManifestDependencyPolicyViolations {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ManifestPath
    )

    # ManifestLines stores the UTF-8 Cargo manifest content in source order.
    # ManifestLines 按源码顺序保存 UTF-8 Cargo 清单内容。
    $ManifestLines = Get-Content -LiteralPath $ManifestPath -Encoding utf8

    # CurrentSection stores the active TOML section name while scanning declarations.
    # CurrentSection 保存扫描声明时当前活动的 TOML 区段名称。
    $CurrentSection = ""

    # Violations accumulates every authoritative dependency-policy failure for this manifest.
    # Violations 累积当前清单的全部权威依赖政策失败。
    $Violations = [System.Collections.Generic.List[string]]::new()

    for ($LineIndex = 0; $LineIndex -lt $ManifestLines.Count; $LineIndex++) {
        # SourceLine stores the exact line used for section and declaration matching.
        # SourceLine 保存用于区段和声明匹配的精确行。
        $SourceLine = $ManifestLines[$LineIndex]

        # TrimmedLine stores the whitespace-trimmed source without rewriting the original diagnostic text.
        # TrimmedLine 保存去除外围空白后的源码，同时不改写原始诊断文本。
        $TrimmedLine = $SourceLine.Trim()

        # SectionMatch captures one complete TOML header.
        # SectionMatch 捕获一个完整的 TOML 区段头。
        $SectionMatch = [regex]::Match($TrimmedLine, '^\[(?<name>[^\]]+)\]$')
        if ($SectionMatch.Success) {
            $CurrentSection = $SectionMatch.Groups['name'].Value
            continue
        }

        if (-not (Test-IsDependencySection -SectionName $CurrentSection)) {
            continue
        }
        if ([string]::IsNullOrWhiteSpace($TrimmedLine) -or $TrimmedLine.StartsWith("#")) {
            continue
        }

        # DependencyMatch captures the owning dependency key and its declaration value.
        # DependencyMatch 捕获依赖所属键及其声明值。
        $DependencyMatch = [regex]::Match(
            $TrimmedLine,
            '^(?<name>[A-Za-z0-9_-]+)(?<workspace>\.workspace)?\s*=\s*(?<value>.+)$'
        )
        if (-not $DependencyMatch.Success) {
            continue
        }

        # DependencyName stores the exact Cargo dependency key used by this member.
        # DependencyName 保存当前成员使用的精确 Cargo 依赖键。
        $DependencyName = $DependencyMatch.Groups['name'].Value

        # WorkspaceHack is generated by cargo-hakari and intentionally retains its managed version-plus-path syntax.
        # workspace-hack 由 cargo-hakari 生成，并刻意保留其受管理的版本加路径语法。
        if ($DependencyName -eq "workspace-hack") {
            continue
        }

        # UsesDottedWorkspace detects Cargo's canonical dependency.workspace inheritance syntax.
        # UsesDottedWorkspace 检测 Cargo 规范的 dependency.workspace 继承语法。
        $UsesDottedWorkspace = $DependencyMatch.Groups['workspace'].Success

        # DeclarationValue stores the exact inline value used to detect table-based workspace inheritance.
        # DeclarationValue 保存用于检测内联表工作区继承的精确声明值。
        $DeclarationValue = $DependencyMatch.Groups['value'].Value

        # UsesInlineWorkspace detects workspace = true on the declaration's owning line, including multiline feature tables.
        # UsesInlineWorkspace 检测声明所属行上的 workspace = true，包括多行 Feature 表。
        $UsesInlineWorkspace = $DeclarationValue -match '\bworkspace\s*=\s*true\b'
        if ($UsesDottedWorkspace -or $UsesInlineWorkspace) {
            continue
        }

        # DisplayPath stores a stable workspace-relative diagnostic path whenever possible.
        # DisplayPath 尽可能保存稳定的工作区相对诊断路径。
        $DisplayPath = [System.IO.Path]::GetRelativePath($ProjectDir, $ManifestPath)
        $Violations.Add(
            ("{0}:{1}: dependency '{2}' must inherit from [workspace.dependencies]" -f
                $DisplayPath,
                ($LineIndex + 1),
                $DependencyName)
        )
    }

    return $Violations
}

Push-Location $ProjectDir
try {
    # MetadataJson stores Cargo's authoritative member and manifest topology.
    # MetadataJson 保存 Cargo 权威的成员与清单拓扑。
    $MetadataJson = Invoke-RtkChecked -Arguments @(
        "cargo", "metadata", "--format-version", "1", "--locked", "--no-deps"
    ) -CaptureOutput

    # Metadata stores the parsed workspace model used to select real member manifests.
    # Metadata 保存用于选择真实成员清单的已解析工作区模型。
    $Metadata = $MetadataJson | ConvertFrom-Json

    # RootManifestPath points at the authoritative workspace manifest that owns Resolver and Rust policy.
    # RootManifestPath 指向持有 Resolver 与 Rust 政策的权威工作区清单。
    $RootManifestPath = Join-Path $ProjectDir "Cargo.toml"

    # RootManifestText stores the complete root manifest for fail-closed policy parsing.
    # RootManifestText 保存根清单完整内容，供关闭失败式政策解析使用。
    $RootManifestText = Get-Content -LiteralPath $RootManifestPath -Raw -Encoding utf8
    if ($RootManifestText -notmatch '(?m)^resolver\s*=\s*"3"\s*$') {
        throw "Root Cargo.toml must declare workspace resolver = `"3`"."
    }

    # WorkspacePackageSectionMatch captures only the authoritative workspace.package section.
    # WorkspacePackageSectionMatch 仅捕获权威的 workspace.package 区段。
    $WorkspacePackageSectionMatch = [regex]::Match(
        $RootManifestText,
        '(?ms)^\[workspace\.package\]\s*\r?\n(?<body>.*?)(?=^\[|\z)'
    )
    if (-not $WorkspacePackageSectionMatch.Success) {
        throw "Root Cargo.toml is missing [workspace.package]."
    }

    # WorkspaceRustVersionMatch captures the single Rust support baseline owned by the workspace.
    # WorkspaceRustVersionMatch 捕获工作区持有的唯一 Rust 支持基线。
    $WorkspaceRustVersionMatch = [regex]::Match(
        $WorkspacePackageSectionMatch.Groups['body'].Value,
        '(?m)^rust-version\s*=\s*"(?<version>[^"]+)"\s*$'
    )
    if (-not $WorkspaceRustVersionMatch.Success) {
        throw "Root Cargo.toml must declare [workspace.package].rust-version."
    }

    # WorkspaceRustVersion stores the exact version shared by the toolchain and every member.
    # WorkspaceRustVersion 保存工具链与全部成员共享的准确版本。
    $WorkspaceRustVersion = $WorkspaceRustVersionMatch.Groups['version'].Value

    # RustToolchainPath points at the repository-owned fixed toolchain contract.
    # RustToolchainPath 指向仓库持有的固定工具链契约。
    $RustToolchainPath = Join-Path $ProjectDir "rust-toolchain.toml"
    if (-not (Test-Path -LiteralPath $RustToolchainPath)) {
        throw "Missing repository Rust toolchain contract: $RustToolchainPath"
    }

    # RustToolchainText stores the complete toolchain contract for channel and component validation.
    # RustToolchainText 保存完整工具链契约，供频道与组件验证使用。
    $RustToolchainText = Get-Content -LiteralPath $RustToolchainPath -Raw -Encoding utf8

    # RustToolchainChannelMatch captures the fixed toolchain channel declared by the repository.
    # RustToolchainChannelMatch 捕获仓库声明的固定工具链频道。
    $RustToolchainChannelMatch = [regex]::Match(
        $RustToolchainText,
        '(?m)^channel\s*=\s*"(?<channel>[^"]+)"\s*$'
    )
    if (-not $RustToolchainChannelMatch.Success -or
        $RustToolchainChannelMatch.Groups['channel'].Value -ne $WorkspaceRustVersion) {
        throw "rust-toolchain.toml channel must match workspace rust-version $WorkspaceRustVersion."
    }
    if ($RustToolchainText -notmatch '(?m)^profile\s*=\s*"minimal"\s*$') {
        throw 'rust-toolchain.toml must use profile = "minimal".'
    }
    # RequiredComponent iterates over every mandatory component in the fixed minimal toolchain.
    # RequiredComponent 遍历固定最小工具链中的每个必需组件。
    foreach ($RequiredComponent in @("clippy", "rustfmt")) {
        if ($RustToolchainText -notmatch ('(?m)^components\s*=.*"{0}"' -f [regex]::Escape($RequiredComponent))) {
            throw "rust-toolchain.toml must include component '$RequiredComponent'."
        }
    }

    # WorkspaceMemberIds provides constant-time ownership checks for metadata packages.
    # WorkspaceMemberIds 为元数据包提供常量时间的工作区归属检查。
    $WorkspaceMemberIds = @{}
    foreach ($WorkspaceMemberId in $Metadata.workspace_members) {
        $WorkspaceMemberIds[$WorkspaceMemberId] = $true
    }

    # RustPolicyViolations accumulates missing inheritance and resolved-version drift across all members.
    # RustPolicyViolations 累积全部成员缺失继承与解析版本漂移问题。
    $RustPolicyViolations = [System.Collections.Generic.List[string]]::new()
    # Package iterates over Cargo's authoritative package records for member-level Rust policy checks.
    # Package 遍历 Cargo 权威包记录，执行成员级 Rust 政策检查。
    foreach ($Package in $Metadata.packages) {
        if (-not $WorkspaceMemberIds.ContainsKey($Package.id)) {
            continue
        }

        # MemberManifestText stores the member package section inspected for explicit workspace inheritance.
        # MemberManifestText 保存用于检查显式工作区继承的成员包清单文本。
        $MemberManifestText = Get-Content -LiteralPath $Package.manifest_path -Raw -Encoding utf8
        if ($MemberManifestText -notmatch '(?m)^rust-version\.workspace\s*=\s*true\s*$') {
            # MemberDisplayPath stores the stable workspace-relative member manifest path.
            # MemberDisplayPath 保存稳定的工作区相对成员清单路径。
            $MemberDisplayPath = [System.IO.Path]::GetRelativePath($ProjectDir, $Package.manifest_path)
            $RustPolicyViolations.Add("${MemberDisplayPath}: package must declare rust-version.workspace = true")
        }
        if ([string]$Package.rust_version -ne $WorkspaceRustVersion) {
            $RustPolicyViolations.Add(
                ("{0}: resolved rust-version '{1}' does not match workspace baseline '{2}'" -f
                    $Package.name,
                    $Package.rust_version,
                    $WorkspaceRustVersion)
            )
        }
    }

    if ($RustPolicyViolations.Count -gt 0) {
        Write-Error ("Rust toolchain policy violations:`n{0}" -f ($RustPolicyViolations -join "`n"))
        exit 1
    }

    Write-Host ("==> Resolver 3 and Rust {0} policy are consistent across all workspace members." -f $WorkspaceRustVersion)

    # PolicyViolations accumulates bypasses across every non-generated workspace member.
    # PolicyViolations 累积全部非生成工作区成员中的绕过行为。
    $PolicyViolations = [System.Collections.Generic.List[string]]::new()
    foreach ($Package in $Metadata.packages) {
        if (-not $WorkspaceMemberIds.ContainsKey($Package.id) -or $Package.name -eq "workspace-hack") {
            continue
        }
        foreach ($Violation in (Get-ManifestDependencyPolicyViolations -ManifestPath $Package.manifest_path)) {
            $PolicyViolations.Add($Violation)
        }
    }

    if ($PolicyViolations.Count -gt 0) {
        Write-Error ("Rust dependency policy violations:`n{0}" -f ($PolicyViolations -join "`n"))
        exit 1
    }

    Write-Host "==> Workspace dependency declarations inherit the root authority."
    Invoke-RtkChecked -Arguments @("cargo", "hakari", "generate", "--diff")
    Invoke-RtkChecked -Arguments @("cargo", "hakari", "manage-deps", "--dry-run")
    Invoke-RtkChecked -Arguments @("cargo", "hakari", "verify")
    Invoke-RtkChecked -Arguments @("cargo", "deny", "check", "--hide-inclusion-graph")
    Invoke-RtkChecked -Arguments @(
        "proxy", "pwsh", "-NoProfile", "-File",
        (Join-Path $ScriptDir "generate_rust_third_party_notices.ps1"),
        "-CheckOnly"
    )
    Invoke-RtkChecked -Arguments @("cargo", "fmt", "--all", "--", "--check")
    Invoke-RtkChecked -Arguments @("cargo", "check", "--workspace", "--all-targets", "--locked")
    Write-Host "==> Rust dependency policy verification completed."
}
finally {
    Pop-Location
}
