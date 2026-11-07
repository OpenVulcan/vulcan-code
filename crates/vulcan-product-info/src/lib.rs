//! Product identity and release notes, read at run time from the distribution resources.
//! 产品身份与更新记录，运行时从发行资源中读取。
//!
//! `resources/product.json` is the only source of the product version. Binaries never compile the
//! version in, so a release that changes nothing but the version and the web pages reuses every
//! binary of the previous release. Workspace crate versions are frozen and carry no meaning.
//! `resources/product.json` 是产品版本的唯一来源。二进制从不编译进版本号，因此一次只改版本号与网页的
//! 发版可以复用上一版的全部二进制。工作区 crate 版本已冻结，不承载任何含义。
//!
//! `resources/changelog/` holds the release notes the About area shows. Its shape (release order,
//! dates, one Markdown file per registered language, the top release equal to the product version)
//! is checked by `scripts/release-notes-check.mjs` in `make check`, not at build time.
//! `resources/changelog/` 保存“关于”区域展示的更新记录。其形状（发布顺序、日期、每种注册语言各一个
//! Markdown 文件、顶部发布等于产品版本）由 `make check` 中的 `scripts/release-notes-check.mjs`
//! 校验，不在构建时校验。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde::Deserialize;

/// File name of the product identity inside the resources directory.
/// 资源目录中产品身份文件的名称。
pub const PRODUCT_FILE_NAME: &str = "product.json";

/// Directory name of the release notes inside the resources directory.
/// 资源目录中更新记录目录的名称。
pub const CHANGELOG_DIR_NAME: &str = "changelog";

/// Index file listing every release inside the changelog directory.
/// 更新记录目录中列出全部发布的索引文件。
const RELEASES_FILE_NAME: &str = "releases.json";

/// Product identity shipped as `resources/product.json`.
/// 以 `resources/product.json` 发行的产品身份。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductInfo {
    /// Exact `x.y.z` product version.
    /// 精确的 `x.y.z` 产品版本。
    pub version: String,
    /// Installation epoch; a change requires a full installer instead of an incremental update.
    /// 安装纪元；变化时需要完整安装包，而不是增量更新。
    pub install_epoch: u32,
}

/// One release listed in `changelog/releases.json`.
/// `changelog/releases.json` 中列出的一个发布。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// Exact `x.y.z` version.
    /// 精确的 `x.y.z` 版本。
    pub version: String,
    /// Release day as `YYYY-MM-DD`; `None` while the newest release is unreleased.
    /// 以 `YYYY-MM-DD` 表示的发布日；最新发布尚未发布时为 `None`。
    pub date: Option<String>,
}

/// Shape of `changelog/releases.json`.
/// `changelog/releases.json` 的形状。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseIndex {
    releases: Vec<Release>,
}

/// Product identity of this process, loaded once.
/// 本进程的产品身份，只加载一次。
static PRODUCT: OnceLock<Result<ProductInfo, String>> = OnceLock::new();

/// Release list of this process, loaded once.
/// 本进程的发布列表，只加载一次。
static RELEASES: OnceLock<Result<Vec<Release>, String>> = OnceLock::new();

/// Returns the product identity of this distribution.
/// 返回本发行物的产品身份。
///
/// # Returns
/// 返回值
/// The parsed `product.json`, or why it is missing or invalid. A packaged build reads only its
/// own resources directory; a build without one reads the repository resources.
/// 解析后的 `product.json`，或其缺失、无效的原因。打包构建只读取自身的资源目录；没有资源目录的
/// 构建读取仓库资源。
pub fn product() -> Result<&'static ProductInfo, &'static str> {
    PRODUCT
        .get_or_init(|| load_product(&resources_directory().join(PRODUCT_FILE_NAME)))
        .as_ref()
        .map_err(String::as_str)
}

/// Returns the exact product version.
/// 返回精确的产品版本。
///
/// # Panics
/// 恐慌
/// When `product.json` is missing or invalid. Hosts call [`product`] during startup and stop with
/// its error, so a running process never reaches this panic.
/// `product.json` 缺失或无效时。宿主在启动时调用 [`product`] 并携带其错误停止，因此运行中的进程
/// 不会触发此恐慌。
pub fn version() -> &'static str {
    match product() {
        Ok(product) => &product.version,
        Err(error) => panic!("{error}"),
    }
}

/// Returns whether the running product version is at least `minimum`.
/// 返回运行中的产品版本是否不低于 `minimum`。
///
/// # Returns
/// 返回值
/// The comparison, or an error when `minimum` is not an exact `x.y.z` version.
/// 比较结果；`minimum` 不是精确的 `x.y.z` 版本时返回错误。
pub fn version_at_least(minimum: &str) -> Result<bool, String> {
    let minimum = parse_version(minimum)
        .ok_or_else(|| format!("`{minimum}` is not an exact x.y.z version"))?;
    let current = parse_version(version())
        .ok_or_else(|| format!("product version `{}` is not x.y.z", version()))?;
    Ok(current >= minimum)
}

/// Returns whether the running product version is at most `maximum`.
/// 返回运行中的产品版本是否不高于 `maximum`。
///
/// # Returns
/// 返回值
/// The comparison, or an error when `maximum` is not an exact `x.y.z` version.
/// 比较结果；`maximum` 不是精确的 `x.y.z` 版本时返回错误。
pub fn version_at_most(maximum: &str) -> Result<bool, String> {
    let maximum = parse_version(maximum)
        .ok_or_else(|| format!("`{maximum}` is not an exact x.y.z version"))?;
    let current = parse_version(version())
        .ok_or_else(|| format!("product version `{}` is not x.y.z", version()))?;
    Ok(current <= maximum)
}

/// Returns every release, newest first.
/// 按从新到旧返回全部发布。
///
/// # Returns
/// 返回值
/// The parsed `changelog/releases.json`, or why it is missing or invalid.
/// 解析后的 `changelog/releases.json`，或其缺失、无效的原因。
pub fn releases() -> Result<&'static [Release], &'static str> {
    RELEASES
        .get_or_init(|| {
            load_releases(
                &resources_directory()
                    .join(CHANGELOG_DIR_NAME)
                    .join(RELEASES_FILE_NAME),
            )
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(String::as_str)
}

/// Reads the Markdown notes of `version` in the language `code`.
/// 读取 `version` 在 `code` 语言下的 Markdown 说明。
///
/// # Returns
/// 返回值
/// The notes, which may contain closed `ui-*` answer component fences, or the read error.
/// 说明（可包含已闭合的 `ui-*` 回答组件围栏），或读取错误。
pub fn release_notes(version: &str, code: &str) -> Result<String, String> {
    if parse_version(version).is_none()
        || !code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(format!("no release notes for `{version}` in `{code}`"));
    }
    let path = resources_directory()
        .join(CHANGELOG_DIR_NAME)
        .join(version)
        .join(format!("{code}.md"));
    fs::read_to_string(&path)
        .map_err(|error| format!("failed to read release notes {}: {error}", path.display()))
}

/// Resources directory of this distribution, or of the repository when none is packaged.
/// 本发行物的资源目录；未打包时为仓库资源目录。
fn resources_directory() -> PathBuf {
    vulcan_runtime_assets::resolve_app_resources_directory()
}

/// Parses and validates one `product.json` file.
/// 解析并校验一个 `product.json` 文件。
fn load_product(path: &Path) -> Result<ProductInfo, String> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read product identity {}: {error}",
            path.display()
        )
    })?;
    let product: ProductInfo = serde_json::from_str(&text)
        .map_err(|error| format!("invalid product identity {}: {error}", path.display()))?;
    if parse_version(&product.version).is_none() {
        return Err(format!(
            "invalid product identity {}: `{}` is not an exact x.y.z version",
            path.display(),
            product.version
        ));
    }
    Ok(product)
}

/// Parses one `changelog/releases.json` file.
/// 解析一个 `changelog/releases.json` 文件。
fn load_releases(path: &Path) -> Result<Vec<Release>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read release index {}: {error}", path.display()))?;
    let index: ReleaseIndex = serde_json::from_str(&text)
        .map_err(|error| format!("invalid release index {}: {error}", path.display()))?;
    if index.releases.is_empty() {
        return Err(format!("release index {} lists no release", path.display()));
    }
    Ok(index.releases)
}

/// Parses an exact `x.y.z` version into comparable numbers.
/// 将精确的 `x.y.z` 版本解析为可比较的数值。
fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.').map(|part| {
        (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    });
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}

#[cfg(test)]
mod tests;
