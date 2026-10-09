# shipped_rust_packages.ps1 defines the Cargo packages whose binaries ship in the distribution bin directory.
# shipped_rust_packages.ps1 定义其二进制随发行物 bin 目录分发的 Cargo 包。

# ShippedRustPackages is the single source for distribution builds and third-party notice coverage.
# ShippedRustPackages 是发行构建与第三方声明覆盖范围的唯一来源。
$ShippedRustPackages = @(
    'vulcan-code-service',
    'vulcan-terminal-host',
    'vulcan-judge-host',
    'vulcan-code-tui'
)

# Get-ShippedRustPackageArguments expands the shipped package list into repeated Cargo `-p` selectors.
# Get-ShippedRustPackageArguments 把分发包列表展开为重复的 Cargo `-p` 选择参数。
function Get-ShippedRustPackageArguments {
    foreach ($PackageName in $ShippedRustPackages) {
        '-p'
        $PackageName
    }
}
