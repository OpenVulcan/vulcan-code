//! Integration-test suite for this crate.
//! 本 crate 的集成测试套件。

// Links every integration-test module into one binary so the crate pays one link step instead of one per file.
// 将全部集成测试模块链接进一个二进制，使本 crate 只付出一次链接而非每个文件一次。
// Each module keeps its own file, so relative fixture paths and per-module lint allowances stay unchanged.
// 每个模块保留自己的文件，因此相对夹具路径与模块级 lint 放行保持不变。
// `#[path]` makes each file a mod-rs module, so nested `#[path]` modules still resolve beside it.
// `#[path]` 使每个文件成为 mod-rs 模块，嵌套的 `#[path]` 模块仍在其旁解析。

#[path = "noise_semantics.rs"]
mod noise_semantics;
#[path = "noise_source_numeric.rs"]
mod noise_source_numeric;
#[path = "pii_semantics.rs"]
mod pii_semantics;
#[path = "profile_policy.rs"]
mod profile_policy;
#[path = "rank_only_policy.rs"]
mod rank_only_policy;
#[path = "resource_contract.rs"]
mod resource_contract;
#[path = "rrf_width.rs"]
mod rrf_width;
#[path = "text_cleaner_parity.rs"]
mod text_cleaner_parity;
#[path = "text_policy.rs"]
mod text_policy;
