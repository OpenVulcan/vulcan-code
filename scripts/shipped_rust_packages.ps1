# shipped_rust_packages.ps1 reads the shared inventory of Cargo packages shipped in the distribution bin directory.
# shipped_rust_packages.ps1 读取发行物 bin 目录随包分发的 Cargo 包共享清单。

# ShippedRustPackages derives from the same inventory as Unix builds and publication validation.
# ShippedRustPackages 派生自与 Unix 构建、发布校验相同的共享清单。
$ShippedRustPackages = @((Get-Content -LiteralPath (Join-Path $PSScriptRoot 'shipped-rust-packages.json') -Raw -Encoding UTF8 | ConvertFrom-Json).package)

# Get-ShippedRustPackageArguments expands one kind (`binary` or `library`) into repeated Cargo `-p` selectors.
# Get-ShippedRustPackageArguments 把某一类（`binary` 或 `library`）展开为重复的 Cargo `-p` 选择参数。
# Libraries build in their own invocation; shipped-rust-packages.mjs `shippedCargoArgs` explains why.
# 库在独立调用中构建；原因见 shipped-rust-packages.mjs 的 `shippedCargoArgs`。
function Get-ShippedRustPackageArguments {
    param([ValidateSet('binary', 'library')][string]$Kind)
    $Output = (& node (Join-Path $PSScriptRoot 'shipped-rust-packages.mjs') cargo-args $Kind | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw "Could not read the shipped $Kind packages." }
    $Output -split ' ' | Where-Object { $_ }
}
