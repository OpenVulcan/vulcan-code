//! Thin GUI re-export for shared prompt-based context compaction runtime.
//! 面向 GUI 的共享提示词式上下文压缩运行时薄导出。

pub(super) use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
    PromptCompactionExecution, PromptCompactionJobRequest, PromptCompactionStreamEvent,
    stream_prompt_compaction_job,
};
