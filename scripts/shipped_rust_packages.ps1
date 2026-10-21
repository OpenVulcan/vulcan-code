# shipped_rust_packages.ps1 reads the shared inventory of Cargo packages shipped in the distribution bin directory.
# shipped_rust_packages.ps1 读取发行物 bin 目录随包分发的 Cargo 包共享清单。

# ShippedRustInventory is the same inventory used by Unix builds and publication validation.
# ShippedRustInventory 是与 Unix 构建、发布校验相同的共享清单。
$ShippedRustInventory = @(Get-Content -LiteralPath (Join-Path $PSScriptRoot 'shipped-rust-packages.json') -Raw -Encoding UTF8 | ConvertFrom-Json)

# ShippedRustPackages lists the packages the root Cargo workspace builds.
# ShippedRustPackages 列出根 Cargo 工作区构建的包。
$ShippedRustPackages = @($ShippedRustInventory | Where-Object { $null -eq $_.workspace } | ForEach-Object { $_.package })

# ShippedRustWorkspacePackages lists packages that live in their own Cargo workspace, with that workspace's
# directory relative to the repository root.
# ShippedRustWorkspacePackages 列出位于独立 Cargo 工作区的包，以及该工作区相对仓库根目录的路径。
$ShippedRustWorkspacePackages = @($ShippedRustInventory | Where-Object { $null -ne $_.workspace })

# Get-ShippedRustPackageArguments expands the root workspace package list into repeated Cargo `-p` selectors.
# Get-ShippedRustPackageArguments 把根工作区分发包列表展开为重复的 Cargo `-p` 选择参数。
function Get-ShippedRustPackageArguments {
    foreach ($PackageName in $ShippedRustPackages) {
        '-p'
        $PackageName
    }
}
