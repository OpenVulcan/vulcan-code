// VULCAN MODIFICATION: this module gathers the few `codex_protocol` items the code-mode crates
// use, so they compile without the Codex protocol crate. Each item is copied from openai/codex at
// c419fd01a70dd1d05356da443ac44328bda2e6f4; only the source files and derives differ (the `ts_rs`
// and `schemars` derives are dropped).

use serde::Deserialize;
use serde::Serialize;
use std::cmp::Ordering;
use std::fmt;

// Copied from codex-rs/protocol/src/local_media.rs.
pub const MAX_PROMPT_AUDIO_INPUT_BYTES: usize = 50 * 1024 * 1024;

// Copied from codex-rs/protocol/src/tool_name.rs.

/// Namespace used for top-level function and custom tools.
pub const DEFAULT_FUNCTION_NAMESPACE: &str = "functions";

/// Identifies a callable tool, preserving the namespace split when the model
/// provides one.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ToolName {
    pub name: String,
    pub namespace: Option<String>,
}

impl ToolName {
    pub fn new(namespace: Option<String>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace,
        }
    }

    pub fn plain(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: None,
        }
    }

    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: Some(namespace.into()),
        }
    }

    pub fn with_default_namespace(mut self) -> Self {
        if self.namespace.as_deref().is_none_or(str::is_empty) {
            self.namespace = Some(DEFAULT_FUNCTION_NAMESPACE.to_string());
        }
        self
    }

    pub fn is_default_namespace(&self) -> bool {
        matches!(
            self.namespace.as_deref(),
            None | Some("") | Some(DEFAULT_FUNCTION_NAMESPACE)
        )
    }
}

impl fmt::Display for ToolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(_) if self.is_default_namespace() => f.write_str(&self.name),
            Some(namespace) => write!(f, "{namespace}{}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

impl Ord for ToolName {
    fn cmp(&self, other: &Self) -> Ordering {
        let lhs = match &self.namespace {
            Some(namespace) => (namespace.as_str(), Some(self.name.as_str())),
            None => (self.name.as_str(), None),
        };
        let rhs = match &other.namespace {
            Some(namespace) => (namespace.as_str(), Some(other.name.as_str())),
            None => (other.name.as_str(), None),
        };
        lhs.cmp(&rhs)
    }
}

impl PartialOrd for ToolName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<String> for ToolName {
    fn from(name: String) -> Self {
        Self::plain(name)
    }
}

impl From<&str> for ToolName {
    fn from(name: &str) -> Self {
        Self::plain(name)
    }
}

// Copied from codex-rs/protocol/src/openai_models.rs.

/// Model-owned instructions for Code Mode's exec and wait tools.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CodeModeToolMessages {
    /// Instructional template supporting `{{ default_exec_yield_time_ms }}` and `{{ image_helper }}`.
    /// Runtime tool declarations are appended. Unknown placeholders remain literal.
    /// Exec accepts raw JavaScript; `parameters` is not consumed and its grammar is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec: Option<ToolMessage>,
    /// Complete description and JSON parameter schema, selected independently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<ToolMessage>,
    /// Literal guidance appended when deferred nested tools exist.
    /// Missing or null uses bundled text; an empty string omits the section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_nested_tools_guidance: Option<String>,
    /// Literal shared TypeScript definitions, emitted when Code Mode Only exposes MCP results.
    /// Missing or null uses bundled definitions; an empty string omits the section.
    /// Custom definitions must remain compatible with the generated tool declarations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_typescript_preamble: Option<String>,
}

/// Model-owned messages for a built-in tool.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ToolMessage {
    /// Missing or null uses the built-in description; an empty string suppresses its static
    /// text without disabling the tool. Tool-owned runtime guidance is retained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Complete JSON Schema encoded as a string. Consumed by Multi-Agent V2 tools, Code Mode wait,
    /// request_user_input_async (the send_user_message_async catalog key), and MCP resource helpers.
    /// Uses the harness's supported schema subset; unrecognized keywords are ignored.
    /// Missing, null, invalid or unsupported structures, or a root without `type: "object"`
    /// retains the harness parameters. Schema semantics must remain API-compatible.
    /// Overrides must declare harness-encrypted properties so their annotations can be retained.
    /// Argument handling is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<String>,
}
