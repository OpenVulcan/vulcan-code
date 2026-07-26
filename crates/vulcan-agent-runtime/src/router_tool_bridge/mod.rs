//! Deterministic Vulcan Router contract discovery, AI tool derivation, and exact-account execution.
//! 确定性的 Vulcan Router 合同发现、AI 工具派生与精确账号执行。

mod runtime;
mod schema;

pub use runtime::{
    RouterContractSelector, append_router_contract_tool_candidates, discover_router_contracts,
};
pub use schema::router_parameter_schema_to_json_schema;

#[cfg(test)]
mod tests;
