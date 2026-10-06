// Synthetic plugin settings mount: one settings layout with its actions and package document.
// 合成插件设置挂载：一个设置版面及其 Action 与包内文档。
//
// Actions follow the real service contract: they are bound to the mount's runtime generation and
// every invocation identity is accepted once, so replayed reads must renew theirs.
// Action 遵循真实服务协议：绑定到挂载的运行时代际，且每个调用身份只接受一次，因此重放的读取必须
// 更新其身份。

use super::{FakeService, required, string};
use serde_json::{Value, json};

/// Synthetic plugin authority, a repository-owned fixture.
/// 合成插件权威数据，仓库持有的夹具。
pub(super) fn fixture() -> Value {
    let mut fixture: Value = serde_json::from_str(include_str!("plugin-fixtures.json"))
        .expect("valid demo plugin fixture");
    fixture["credential"] = json!({ "configured": false });
    fixture
}

impl FakeService {
    /// Whether a method belongs to the plugin mount contract modeled here.
    /// 某方法是否属于此处建模的插件挂载协议。
    pub(super) fn is_plugin_method(method: &str) -> bool {
        matches!(
            method,
            "service_load_plugin_ui_module"
                | "service_read_plugin_text_asset"
                | "service_invoke_plugin_action"
        )
    }

    /// Answers one plugin mount call against the synthetic plugin.
    /// 针对合成插件回答一次插件挂载调用。
    pub(super) fn plugin(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        let plugin_id = self.plugin["summary"]["id"].clone();
        if required(params, "/pluginId")? != &plugin_id {
            return Err("Unknown demo plugin".into());
        }
        match method {
            "service_load_plugin_ui_module" => {
                if string(params, "/mountId")? != "settings" {
                    return Err("Unknown demo plugin mount".into());
                }
                let mut module = self.plugin["module"].clone();
                module["source"] = json!(self.plugin["layout"].to_string());
                Ok(module)
            }
            "service_read_plugin_text_asset" => match string(params, "/path")? {
                "LICENSE" => Ok(json!({ "content": self.plugin["license"] })),
                _ => Err("Undeclared plugin asset".into()),
            },
            _ => self.plugin_action(params),
        }
    }

    /// Runs one declared action of the synthetic plugin.
    /// 运行合成插件的一个已声明 Action。
    fn plugin_action(&mut self, params: &Value) -> Result<Value, String> {
        if required(params, "/runtimeIdentity")? != &self.plugin["module"]["runtimeIdentity"] {
            return Err("Plugin runtime generation changed".into());
        }
        let invocation = string(params, "/identity/invocationId")?.to_string();
        string(params, "/identity/clientId")?;
        if invocation.is_empty() || !self.invocations.insert(invocation) {
            return Err("Plugin invocation identity was already used".into());
        }
        let arguments = required(params, "/arguments")?;
        match string(params, "/actionId")? {
            "settings-get" => Ok(self.plugin["config"].clone()),
            "settings-set" => {
                if !arguments.is_object() {
                    return Err("Configuration must be an object".into());
                }
                self.plugin["config"] = arguments.clone();
                Ok(arguments.clone())
            }
            "credential-status" => Ok(self.plugin["credential"].clone()),
            "credential-set" => {
                if string(arguments, "/apiKey")?.is_empty() {
                    return Err("Empty credential".into());
                }
                self.plugin["credential"] =
                    json!({ "configured": true, "validationStatus": "untested" });
                Ok(self.plugin["credential"].clone())
            }
            "credential-delete" => {
                self.plugin["credential"] = json!({ "configured": false });
                Ok(self.plugin["credential"].clone())
            }
            "credential-test" => {
                if self.plugin["credential"]["configured"] != json!(true) {
                    return Err("No credential to test".into());
                }
                self.plugin["credential"]["validationStatus"] = json!("valid");
                Ok(json!({}))
            }
            "stats" => {
                let days = required(arguments, "/days")?
                    .as_u64()
                    .ok_or("Days must be a count")?;
                let daily: Vec<Value> = (1..=days)
                    .map(|day| json!({ "date": format!("2026-09-{day:02}"), "saved": day * 1500 }))
                    .collect();
                Ok(json!({
                    "totals": { "commands": days * 40, "saved": days * 12_000, "rate": 62.5 },
                    "daily": daily,
                    "weekly": [{ "week": "W38", "saved": 9000 }, { "week": "W39", "saved": 15000 }],
                    "top": [
                        { "command": "git status", "saved": days * 900 },
                        { "command": "cargo test", "saved": days * 300 },
                    ],
                }))
            }
            _ => Err("Undeclared plugin action".into()),
        }
    }
}
