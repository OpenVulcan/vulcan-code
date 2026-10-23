# shipped_rust_packages.ps1 reads the shared inventory of Cargo packages shipped in the distribution bin directory.
# shipped_rust_packages.ps1 读取发行物 bin 目录随包分发的 Cargo 包共享清单。

# ShippedRustPackages derives from the same inventory as Unix builds and publication validation.
# ShippedRustPackages 派生自与 Unix 构建、发布校验相同的共享清单。
$ShippedRustPackages = @((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'shipped-rust-packages.json') -Raw -Encoding UTF8 | ConvertFrom-Json).package)

# Get-ShippedRustPackageArguments expands the shipped package list into repeated Cargo `-p` selectors.
# Get-ShippedRustPackageArguments 把分发包列表展开为重复的 Cargo `-p` 选择参数。
function Get-ShippedRustPackageArguments {
    foreach ($PackageName in $ShippedRustPackages) {
        '-p'
        $PackageName
    }
}
