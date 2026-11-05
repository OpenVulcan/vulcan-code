# Install or remove the GitHub Actions runner of one self-hosted pool on Windows (docs/contracts/self-hosted-runners.md).
# The Windows counterpart of self-hosted-runner.sh; `node scripts/ci-runners.mjs register local windows-x64` runs it.
# 在 Windows 上安装或移除一个自托管池的 GitHub Actions 运行器（docs/contracts/self-hosted-runners.md）。
# 这是 self-hosted-runner.sh 的 Windows 版本；`node scripts/ci-runners.mjs register local windows-x64` 会运行它。
#
#   self-hosted-runner.ps1 install --repository <owner/name> --label <pool label> [--name <name>] [--dir <directory>] < registration-token
#   self-hosted-runner.ps1 remove [--dir <directory>] < removal-token
#   self-hosted-runner.ps1 run [--dir <directory>]
#
# `install` also writes start.cmd, which puts Git Bash first on PATH and starts run.cmd: workflow steps with
# `shell: bash` must get Git Bash, never the WSL launcher bash.exe that Windows may list earlier.
# `install` 同时写出 start.cmd，它将 Git Bash 放在 PATH 最前并启动 run.cmd：`shell: bash` 的工作流步骤必须
# 使用 Git Bash，绝不能使用 Windows 可能排在更前面的 WSL 启动器 bash.exe。
#
# The token comes on standard input so it never appears in a typed command or a log. The runner comes from its
# latest GitHub release and is checked against the SHA-256 GitHub publishes. No service is installed (a service
# would not get start.cmd's PATH), so the runner is online only while start.cmd runs; --manual is accepted for
# symmetry with the POSIX script. The job workspace is the short `<directory>\w`, since Windows paths are limited
# to 260 characters unless long paths are enabled.
# 令牌经标准输入传入，因此不会出现在输入的命令或日志中。运行器取自其最新 GitHub 发布，并按 GitHub 公布的
# SHA-256 校验。不安装服务（服务得不到 start.cmd 设置的 PATH），因此运行器只在 start.cmd 运行期间在线；
# 为与 POSIX 脚本一致接受 --manual。作业工作区为较短的 `<目录>\w`，因为未启用长路径时 Windows 路径限制为
# 260 个字符。
$ErrorActionPreference = 'Stop'

$command = if ($args.Count -gt 0) { [string]$args[0] } else { '' }
$options = @{ dir = Join-Path $HOME 'actions-runner\vulcan-code'; name = $env:COMPUTERNAME; manual = $false }
for ($index = 1; $index -lt $args.Count; $index++) {
    switch ([string]$args[$index]) {
        '--repository' { $options.repository = [string]$args[++$index] }
        '--label' { $options.label = [string]$args[++$index] }
        '--name' { $options.name = [string]$args[++$index] }
        '--dir' { $options.dir = [string]$args[++$index] }
        '--manual' { $options.manual = $true }
        default { throw "unknown argument $($args[$index])" }
    }
}
$runnerDir = [IO.Path]::GetFullPath($options.dir)

# readToken reads the one-line token from standard input.
# readToken 从标准输入读取单行令牌。
function readToken {
    $token = [Console]::In.ReadLine()
    if ([string]::IsNullOrWhiteSpace($token)) { throw 'the token is expected on standard input' }
    $token.Trim()
}

# configured reports whether a runner is configured in the runner directory.
# configured 判断运行器目录中是否已配置运行器。
function configured { Test-Path -LiteralPath (Join-Path $runnerDir '.runner') }

switch ($command) {
    'install' {
        if (-not $options.repository -or -not $options.label) { throw '--repository and --label are required' }
        $token = readToken
        if (configured) { throw "a runner is already configured in $runnerDir; remove it first" }
        if (-not (Get-Command pwsh -ErrorAction SilentlyContinue)) { throw 'pwsh (PowerShell 7) is required for make prune' }
        # git.exe lives in <Git>\cmd, or in <Git>\mingw64\bin when PATH comes from Git Bash; bash.exe in <Git>\bin.
        # git.exe 位于 <Git>\cmd，PATH 来自 Git Bash 时位于 <Git>\mingw64\bin；bash.exe 位于 <Git>\bin。
        $git = Get-Command git -ErrorAction SilentlyContinue
        $gitBin = $null
        for ($root = $(if ($git) { Split-Path $git.Source }); $root; $root = Split-Path $root) {
            if (Test-Path -LiteralPath (Join-Path $root 'bin\bash.exe')) { $gitBin = Join-Path $root 'bin'; break }
        }
        if (-not $gitBin) { throw 'Git for Windows with its bash.exe is required' }
        New-Item -ItemType Directory -Force -Path $runnerDir | Out-Null
        $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/actions/runner/releases/latest' `
            -Headers @{ Accept = 'application/vnd.github+json' }
        $asset = @($release.assets | Where-Object name -match '^actions-runner-win-x64-[0-9.]+\.zip$')[0]
        if (-not $asset -or $asset.digest -notmatch '^sha256:([0-9a-f]{64})$') { throw 'no verifiable runner asset for win-x64' }
        $expected = $Matches[1]
        $archive = Join-Path $runnerDir 'runner.zip'
        Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $archive
        if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
            throw "$($asset.browser_download_url) does not match its published SHA-256"
        }
        Expand-Archive -LiteralPath $archive -DestinationPath $runnerDir -Force
        Remove-Item -LiteralPath $archive
        $config = @('--unattended', '--url', "https://github.com/$($options.repository)", '--token', $token,
            '--name', $options.name, '--labels', $options.label, '--work', (Join-Path $runnerDir 'w'), '--replace')
        & (Join-Path $runnerDir 'config.cmd') @config
        if ($LASTEXITCODE -ne 0) { throw "config.cmd failed with $LASTEXITCODE" }
        Set-Content -LiteralPath (Join-Path $runnerDir 'start.cmd') -Encoding ascii -Value @(
            '@echo off', "set `"PATH=$gitBin;%PATH%`"", 'call "%~dp0run.cmd" %*')
        "Runner $($options.name) ($($options.label)) is installed in $runnerDir; start it with: $(Join-Path $runnerDir 'start.cmd')"
    }
    'remove' {
        $token = readToken
        if (-not (configured)) { throw "no runner is configured in $runnerDir" }
        & (Join-Path $runnerDir 'config.cmd') remove --token $token
        if ($LASTEXITCODE -ne 0) { throw "config.cmd remove failed with $LASTEXITCODE" }
        "Runner removed; $runnerDir keeps its work directory until you delete it."
    }
    'run' {
        if (-not (configured)) { throw "no runner is configured in $runnerDir" }
        & (Join-Path $runnerDir 'start.cmd')
        exit $LASTEXITCODE
    }
    default {
        throw 'usage: self-hosted-runner.ps1 install --repository <owner/name> --label <label> [--name <name>] [--dir <dir>] [--manual] | remove | run'
    }
}
