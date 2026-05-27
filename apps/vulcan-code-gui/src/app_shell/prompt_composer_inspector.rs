//! Prompt-composer inspector cache and copy helpers for the workbench shell.
//! 工作台 Shell 的提示词组合检查器缓存与复制辅助逻辑。

use super::*;

impl VulcanWorkbench {
    /// Refreshes the prompt-composer inspector cache from the latest runtime report.
    /// 从最近的运行时报告刷新提示词组合检查器缓存。
    pub(super) fn refresh_prompt_composer_inspector_buffer(&mut self) {
        let Some(report) =
            crate::agent_runtime::prompt_composer_runtime::latest_runtime_prompt_composer_report()
        else {
            self.prompt_composer_inspector_markdown.clear();
            self.prompt_composer_inspector_system_text.clear();
            self.prompt_composer_inspector_developer_text.clear();
            self.prompt_composer_inspector_tool_metadata_text.clear();
            self.prompt_composer_inspector_user_text.clear();
            self.prompt_composer_inspector_buffer = text_editor::Content::with_text("");
            self.prompt_composer_inspector_layer_buffers.clear();
            self.prompt_composer_inspector_segment_texts.clear();
            self.prompt_composer_inspector_segment_buffers.clear();
            self.prompt_composer_inspector_expanded_segments.clear();
            return;
        };

        if self.prompt_composer_inspector_markdown == report.markdown
            && self.prompt_composer_inspector_system_text == report.system_text
            && self.prompt_composer_inspector_developer_text == report.developer_text
            && self.prompt_composer_inspector_tool_metadata_text == report.tool_metadata_text
            && self.prompt_composer_inspector_user_text == report.user_text
        {
            return;
        }

        self.prompt_composer_inspector_markdown = report.markdown;
        self.prompt_composer_inspector_system_text = report.system_text;
        self.prompt_composer_inspector_developer_text = report.developer_text;
        self.prompt_composer_inspector_tool_metadata_text = report.tool_metadata_text;
        self.prompt_composer_inspector_user_text = report.user_text;
        self.prompt_composer_inspector_buffer = text_editor::Content::with_text("");
        self.prompt_composer_inspector_layer_buffers.clear();
        self.prompt_composer_inspector_segment_texts.clear();
        self.prompt_composer_inspector_segment_buffers.clear();
        self.prompt_composer_inspector_expanded_segments.clear();
        for (index, segment) in report.segments.iter().enumerate() {
            let segment_key = prompt_composer_inspector_segment_key(
                index,
                &segment.target_label,
                &segment.node_id,
            );
            self.prompt_composer_inspector_segment_buffers.insert(
                segment_key.clone(),
                text_editor::Content::with_text(&prompt_composer_inspector_display_text(
                    &segment.body,
                )),
            );
            self.prompt_composer_inspector_segment_texts
                .insert(segment_key, segment.body.clone());
        }
        let expanded_layers = self
            .prompt_composer_inspector_expanded_layers
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for layer in expanded_layers {
            self.ensure_prompt_composer_inspector_layer_buffer(layer);
        }
    }

    /// Lazily materializes one prompt-composer layer editor buffer only when the layer is actually opened.
    /// 仅在提示词组合检查器分层真正展开时，才惰性物化对应编辑器缓冲。
    pub(super) fn ensure_prompt_composer_inspector_layer_buffer(
        &mut self,
        layer: PromptComposerInspectorLayer,
    ) {
        if self
            .prompt_composer_inspector_layer_buffers
            .contains_key(&layer)
        {
            return;
        }

        let layer_text = prompt_composer_inspector_display_text(
            self.prompt_composer_inspector_layer_text(layer),
        );
        self.prompt_composer_inspector_layer_buffers
            .insert(layer, text_editor::Content::with_text(&layer_text));
    }

    /// Lazily materializes one prompt-composer segment editor buffer only when the node is actually opened.
    /// 仅在提示词组合检查器节点真正展开时，才惰性物化对应编辑器缓冲。
    pub(super) fn ensure_prompt_composer_inspector_segment_buffer(&mut self, segment_key: &str) {
        if self
            .prompt_composer_inspector_segment_buffers
            .contains_key(segment_key)
        {
            return;
        }

        let Some(segment_text) = self
            .prompt_composer_inspector_segment_texts
            .get(segment_key)
            .cloned()
        else {
            return;
        };
        let display_text = prompt_composer_inspector_display_text(&segment_text);
        self.prompt_composer_inspector_segment_buffers.insert(
            segment_key.to_string(),
            text_editor::Content::with_text(&display_text),
        );
    }

    /// Returns the cached text for one prompt-composer inspector layer.
    /// 返回单个提示词组合检查器分层的缓存文本。
    pub(super) fn prompt_composer_inspector_layer_text(
        &self,
        layer: PromptComposerInspectorLayer,
    ) -> &str {
        match layer {
            PromptComposerInspectorLayer::System => &self.prompt_composer_inspector_system_text,
            PromptComposerInspectorLayer::Developer => {
                &self.prompt_composer_inspector_developer_text
            }
            PromptComposerInspectorLayer::ToolMetadata => {
                &self.prompt_composer_inspector_tool_metadata_text
            }
            PromptComposerInspectorLayer::UserContext => &self.prompt_composer_inspector_user_text,
        }
    }

    /// Copies one prompt-composer inspector layer to the clipboard when available.
    /// 在可用时将单个提示词组合检查器分层复制到剪贴板。
    pub(super) fn copy_prompt_composer_inspector_layer(
        &mut self,
        layer: PromptComposerInspectorLayer,
    ) -> Task<Message> {
        if self
            .prompt_composer_inspector_layer_text(layer)
            .trim()
            .is_empty()
        {
            self.refresh_prompt_composer_inspector_buffer();
        }
        let layer_text = self.prompt_composer_inspector_layer_text(layer).to_string();
        if layer_text.trim().is_empty() {
            self.status_message = format!("{}暂无可复制内容。", layer.label());
            Task::none()
        } else {
            self.status_message = format!("已复制{}提示词。", layer.label());
            clipboard::write(layer_text).discard()
        }
    }

    /// Copies one prompt-composer segment to the clipboard when available.
    /// 在可用时将单个提示词组合片段复制到剪贴板。
    pub(super) fn copy_prompt_composer_inspector_segment(
        &mut self,
        segment_key: String,
    ) -> Task<Message> {
        if let Some(segment_text) = self
            .prompt_composer_inspector_segment_texts
            .get(&segment_key)
            .cloned()
        {
            if segment_text.trim().is_empty() {
                self.status_message = String::from("该节点暂无可复制内容。");
                Task::none()
            } else {
                self.status_message = String::from("已复制提示词节点正文。");
                clipboard::write(segment_text).discard()
            }
        } else {
            self.status_message = String::from("该节点正文缓存已失效，请重新执行一次会话。");
            Task::none()
        }
    }
}
