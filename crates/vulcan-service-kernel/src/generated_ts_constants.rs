//! Renders the Rust-owned protocol bounds that TypeScript clients enforce, so each bound has one definition.
//! 渲染 TypeScript 客户端执行的 Rust 自有协议边界值，使每个边界值只有一处定义。
//!
//! Run `VULCAN_UPDATE_GENERATED_TS=1 cargo test -p vulcan-service-kernel generated_ts_constants`
//! after changing any bound below; without the variable the test fails when a generated file is stale.
//! 修改下列任一边界值后运行 `VULCAN_UPDATE_GENERATED_TS=1 cargo test -p vulcan-service-kernel generated_ts_constants`；
//! 未设置该变量时，生成文件过期会使测试失败。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Environment variable that switches the freshness test into regeneration mode.
/// 将新鲜度测试切换为重新生成模式的环境变量。
const UPDATE_VARIABLE: &str = "VULCAN_UPDATE_GENERATED_TS";

/// One exported TypeScript constant and the Rust authority it is rendered from.
/// 一个导出的 TypeScript 常量及其渲染来源的 Rust 权威值。
enum Constant {
    /// Unsigned integer bound.
    /// 无符号整数边界值。
    Number(&'static str, u64, &'static str, &'static str),
    /// Fixed string identifier.
    /// 固定字符串标识。
    Text(&'static str, &'static str, &'static str, &'static str),
}

/// Renders one generated module with its bilingual header and documented constants.
/// 渲染一个带双语文件头与文档化常量的生成模块。
///
/// `quote` is the target project's string quote style; `constants` are emitted in order.
/// `quote` 为目标项目的字符串引号风格；`constants` 按顺序输出。
/// Returns the exact file text, ending with one newline.
/// 返回精确文件文本，以一个换行结尾。
fn render(quote: char, constants: &[Constant]) -> String {
    let mut text = String::from(
        "// Generated from Rust by vulcan-service-kernel `generated_ts_constants`; do not edit.\n\
         // 由 vulcan-service-kernel `generated_ts_constants` 从 Rust 生成；请勿手动编辑。\n",
    );
    for constant in constants {
        let (name, value, english, chinese) = match constant {
            Constant::Number(name, value, english, chinese) => {
                (*name, value.to_string(), *english, *chinese)
            }
            Constant::Text(name, value, english, chinese) => {
                (*name, format!("{quote}{value}{quote}"), *english, *chinese)
            }
        };
        let _ = write!(
            text,
            "\n/** {english}\n * {chinese} */\nexport const {name} = {value};\n"
        );
    }
    text
}

/// Renders the WebUI module consumed by conversation, resource and terminal clients.
/// 渲染会话、资源与终端客户端使用的 WebUI 模块。
fn web_module() -> String {
    render(
        '"',
        &[
            Constant::Number(
                "CLIENT_ACTION_ID_MAX_BYTES",
                vulcan_session_core::v1::CLIENT_ACTION_ID_MAX_BYTES as u64,
                "Maximum UTF-8 byte length of one V1 client action identity.",
                "单个 V1 客户端动作身份的最大 UTF-8 字节长度。",
            ),
            Constant::Number(
                "SESSION_RESOURCE_LIST_V1_MAX_ITEMS",
                vulcan_service_api::SESSION_RESOURCE_LIST_V1_MAX_ITEMS as u64,
                "Maximum resource references accepted by one V1 resource list request.",
                "单个 V1 资源列表请求接受的最大资源引用数。",
            ),
            Constant::Number(
                "PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES",
                crate::project_terminals::PROJECT_TERMINAL_INPUT_BATCH_MAX_BYTES as u64,
                "Service hard limit on the bytes of one project terminal input batch.",
                "服务端对单个项目终端输入批次字节数的硬限制。",
            ),
            Constant::Number(
                "PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS",
                crate::project_terminals::PROJECT_TERMINAL_INPUT_BATCH_MAX_EVENTS as u64,
                "Service hard limit on the input events carried by one project terminal batch.",
                "服务端对单个项目终端批次承载输入事件数的硬限制。",
            ),
            Constant::Number(
                "ANSWER_COMPONENT_MAX_ROWS",
                vulcan_session_core::v1::answer_components::ANSWER_COMPONENT_MAX_ROWS as u64,
                "Most data rows one ui-* answer component renders; larger fences stay code.",
                "单个 ui-* 回答组件渲染的最多数据行数；更大的围栏保持为代码。",
            ),
            Constant::Number(
                "ANSWER_CHART_MAX_SERIES",
                vulcan_session_core::v1::answer_components::ANSWER_CHART_MAX_SERIES as u64,
                "Most value series one ui-chart carries, equal to the chart palette size.",
                "单个 ui-chart 承载的最多数值系列数，等于图表配色数量。",
            ),
            Constant::Text(
                "DEFAULT_ANSWER_MODE",
                vulcan_session_core::v1::config::AnswerMode::default().wire_name(),
                "System default answer mode for users who never chose one.",
                "从未选择回答模式的用户所用的系统默认回答模式。",
            ),
        ],
    )
}

/// Renders the desktop browser-host module shared by the host and its control client.
/// 渲染宿主及其控制客户端共用的桌面浏览器宿主模块。
fn desktop_browser_host_module() -> String {
    use crate::browser::host;
    render(
        '\'',
        &[
            Constant::Number(
                "BROWSER_HOST_PROTOCOL_VERSION",
                u64::from(host::BROWSER_HOST_PROTOCOL_VERSION),
                "Control protocol version understood by this build.",
                "本构建理解的控制协议版本。",
            ),
            Constant::Number(
                "BROWSER_HOST_MAX_FRAME_BYTES",
                host::BROWSER_HOST_MAX_FRAME_BYTES as u64,
                "Maximum bytes accepted for one control frame.",
                "单个控制帧接受的最大字节数。",
            ),
            Constant::Number(
                "BROWSER_HOST_LENGTH_PREFIX_BYTES",
                host::BROWSER_HOST_LENGTH_PREFIX_BYTES as u64,
                "Big-endian length prefix size of one control frame.",
                "单个控制帧大端长度前缀的字节数。",
            ),
            Constant::Text(
                "BROWSER_HOST_CONTROL_DISCOVERY_FILENAME",
                host::BROWSER_HOST_CONTROL_DISCOVERY_FILENAME,
                "Discovery file name written below the host's own user-data directory.",
                "写入宿主自有用户数据目录下的发现文件名。",
            ),
            Constant::Number(
                "BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION",
                u64::from(host::BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION),
                "Schema version of the control discovery document.",
                "控制发现文档的架构版本。",
            ),
        ],
    )
}

/// Resolves one repository-relative path from this crate's manifest directory.
/// 从本 crate 的清单目录解析一个仓库相对路径。
fn repository_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Generated TypeScript modules must equal what the Rust authorities render.
/// 生成的 TypeScript 模块必须等于 Rust 权威值渲染的结果。
#[test]
fn generated_ts_constants_match_rust_authorities() {
    let update = std::env::var_os(UPDATE_VARIABLE).is_some();
    let mut stale = Vec::new();
    for (relative, expected) in [
        (
            "apps/vulcan-code-web/src/generated/protocol-limits.ts",
            web_module(),
        ),
        (
            "apps/vulcan-code-desktop/src/browser-host/protocol-constants.generated.ts",
            desktop_browser_host_module(),
        ),
    ] {
        let path = repository_path(relative);
        // Line endings are normalized so a checkout's newline conversion is not reported as drift.
        // 统一换行符，避免检出时的换行转换被报告为漂移。
        let actual = std::fs::read_to_string(&path)
            .map(|text| text.replace("\r\n", "\n"))
            .unwrap_or_default();
        if actual == expected {
            continue;
        }
        if update {
            std::fs::write(&path, expected).expect("write generated TypeScript constants");
        } else {
            stale.push(relative);
        }
    }
    assert!(
        stale.is_empty(),
        "generated TypeScript constants are stale: {stale:?}; rerun with {UPDATE_VARIABLE}=1"
    );
}
