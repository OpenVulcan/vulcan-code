/// Default built-in personality identifier.
/// 默认内置人格标识。
pub const DEFAULT_PERSONALITY_ID: &str = "friendly";

/// Built-in friendly personality text copied from Codex.
/// 从 Codex 复制的内置亲和人格文本。
pub const FRIENDLY_PERSONALITY_TEXT: &str =
    include_str!("../../resources/personalities/friendly.md");

/// Built-in pragmatic personality text copied from Codex.
/// 从 Codex 复制的内置务实人格文本。
pub const PRAGMATIC_PERSONALITY_TEXT: &str =
    include_str!("../../resources/personalities/pragmatic.md");
