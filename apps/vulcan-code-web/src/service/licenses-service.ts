import { serviceClient } from "@/service/webui-service-runtime"

// LicenseDependency carries the exact fields produced by the checked-in cargo-about template.
// LicenseDependency 携带签入 cargo-about 模板产出的精确字段。
export interface LicenseDependency {
  // Exact resolved Cargo package name.
  // 精确解析的 Cargo 包名称。
  name: string
  // Exact resolved package version.
  // 精确解析的包版本。
  version: string
  // Declared repository or explicit crates.io template URL.
  // 声明仓库或模板显式生成的 crates.io URL。
  source: string
}

// LicenseSection identifies one original body variant and its actual dependents.
// LicenseSection 标识一份原始正文变体及其实际依赖项。
export interface LicenseSection {
  // Catalog-owned identity, never a local path.
  // 目录持有的身份，绝不是本地路径。
  id: string
  // Dependencies covered by this exact body.
  // 此精确正文覆盖的依赖。
  dependencies: LicenseDependency[]
  // Inclusive one-based source range.
  // 从一开始且包含两端的来源范围。
  start_line: number
  // Inclusive final source line.
  // 包含在内的来源末行。
  end_line: number
}

// LicenseGroup merges declared names while retaining all copyright-specific body variants.
// LicenseGroup 合并声明名称，同时保留全部版权特定正文变体。
export interface LicenseGroup {
  // Group identity within the source revision.
  // 来源版本内的分组身份。
  id: string
  // Cargo-about's declared title without inferred SPDX mapping.
  // cargo-about 声明标题，不推测 SPDX 对应关系。
  title: string
  // Explicit declaration state.
  // 明确的声明状态。
  status: "declared" | "unknown"
  // Every original body under the same declared title.
  // 同一声明标题下的每份原始正文。
  sections: LicenseSection[]
}

// LicenseCatalog mirrors the validated read-only service contract.
// LicenseCatalog 对齐已验证的只读服务合同。
export interface LicenseCatalog {
  // Supported artifact schema revision.
  // 支持的制品结构版本。
  schema_version: 2
  // Actual make dependency graph scope.
  // 实际 make 依赖图范围。
  scope: "rust-desktop-dependencies"
  // Fixed source artifact name.
  // 固定来源制品名称。
  source_file: "THIRD_PARTY_NOTICES.md"
  // Normalized Markdown digest used to reject stale body requests.
  // 用于拒绝陈旧正文请求的规范 Markdown 摘要。
  source_sha256: string
  // Declared license type groups.
  // 声明许可证类型分组。
  licenses: LicenseGroup[]
}

// LicenseBody carries one exact original body as plain text.
// LicenseBody 以纯文本携带一份精确原始正文。
export interface LicenseBody {
  // Selected catalog section identity.
  // 选中的目录段落身份。
  section_id: string
  // Complete original text.
  // 完整原始文本。
  text: string
}

// loadLicenseCatalog returns validated metadata without transferring every license body.
// loadLicenseCatalog 返回已验证元数据，不传输全部许可证正文。
export function loadLicenseCatalog(): Promise<LicenseCatalog> {
  return serviceClient.request<LicenseCatalog>("service_license_catalog")
}

// loadLicenseBody reads an indexed section only when its source revision still matches.
// loadLicenseBody 仅在来源版本仍匹配时读取索引内段落。
// Parameters identify the exact section and catalog revision; returns its complete original text.
// 参数标识精确段落和目录版本；返回完整原始文本。
export function loadLicenseBody(sectionId: string, expectedSha256: string): Promise<LicenseBody> {
  return serviceClient.request<LicenseBody>("service_license_body", { sectionId, expectedSha256 })
}
