//! `tool_chain` native tool: ordered, dependent tool calls.
//! `tool_chain` 原生工具：有序、相互依赖的工具调用。
//!
//! Only the descriptor and schema live here. Steps need the tool bridge's definitions and
//! permission gates, so the bridge intercepts this tool before dispatch; reaching this handler
//! means the call came through a path that cannot run chains.
//! 这里只有描述与 Schema。步骤需要工具桥的定义与权限闸门，因此桥在调度前拦截本工具；到达本处理器
//! 意味着调用来自无法运行链条的路径。

use super::*;
use vulcan_tool_chain::{TOOL_CHAIN_TOOL_ID, tool_chain_description, tool_chain_input_schema};

/// Builds the `tool_chain` tool definition.
/// 构建 `tool_chain` 工具定义。
pub(crate) fn build_tool_chain_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(TOOL_CHAIN_TOOL_ID),
        name: String::from(TOOL_CHAIN_TOOL_ID),
        description: tool_chain_description(),
        help_text: None,
        ai_visible: true,
        // Every step passes the plan-mode gate on its own.
        // 每个步骤各自通过计划模式闸门。
        available_in_plan: true,
    };
    build_native_tool_definition(descriptor, tool_chain_input_schema(), |_request| {
        Box::pin(async {
            Err(NativeToolError::new(
                "tool_chain can only run inside an agent turn; call the tools directly.",
            ))
        })
    })
}
