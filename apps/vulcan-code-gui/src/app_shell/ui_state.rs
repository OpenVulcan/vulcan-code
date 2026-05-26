//! Workbench UI state extracted from the main workbench module.
//! 从主工作台模块中抽取的工作台界面状态。

use super::*;

/// Confirm action carried by the reusable blocking notice modal.
/// 可复用阻塞提示弹窗携带的确认动作。
#[derive(Debug, Clone)]
pub(super) enum WorkbenchNoticeConfirmAction {
    /// Closes the modal without mutating domain state.
    /// 关闭弹窗且不修改业务状态。
    Acknowledge,
    /// Abandons the currently visible ask/question modal after the user explicitly accepts interrupting the running conversation.
    /// 在用户显式接受中断当前对话后，放弃当前可见的 ask/question 弹窗。
    AbandonQuestionModal,
    /// Applies one pending model switch and removes staged attachments unsupported by the target model.
    /// 应用一次待确认模型切换，并移除目标模型不支持的已暂存附件。
    ApplyModelSwitchRemovingUnsupportedAttachments {
        /// Pending model choice selected by the user before compatibility confirmation.
        /// 用户在兼容性确认之前选择的待切换模型。
        choice: WorkbenchModelChoice,
    },
    /// Removes one configured model provider after the user confirms the destructive action.
    /// 在用户确认破坏性操作后移除一条模型供应商配置。
    RemoveConfiguredProvider {
        /// Stable configured provider identifier that will be removed permanently.
        /// 完成确认后将被移除的稳定模型供应商标识。
        provider_id: String,
    },
    /// Removes one shared capability vendor after the user confirms the destructive action.
    /// 在用户确认破坏性操作后移除一条共享能力供应商配置。
    RemoveAiCapabilityVendor {
        /// Stable shared capability vendor identifier that will be removed permanently.
        /// 完成确认后将被移除的稳定共享能力供应商标识。
        vendor_id: String,
    },
    /// Removes one configured MCP server after the user confirms the destructive action.
    /// 在用户确认破坏性操作后移除一条 MCP 服务器配置。
    RemoveMcpServerConfiguration {
        /// Stable configured MCP server identifier that will be removed permanently.
        /// 完成确认后将被移除的稳定 MCP 服务器标识。
        server_id: String,
    },
    /// Rebuilds one session whose canonical event-store files are missing while preserving the visible title.
    /// 在保留可见标题的前提下重建一条规范事件文件已丢失的会话。
    RebuildMissingSession {
        /// Stable session identifier whose persisted body files are missing.
        /// 持久化正文文件已丢失的稳定会话标识。
        session_id: SessionId,
    },
    /// Executes one previously previewed project-scoped attachment cleanup after the user confirms the destructive action.
    /// 在用户确认破坏性操作后执行一条此前已预览的项目级附件清理请求。
    ExecuteProjectAttachmentCleanup {
        /// Cleanup execution request captured at confirmation time.
        /// 确认时捕获的清理执行请求。
        request: ProjectAttachmentCleanupExecutionRequest,
    },
}

/// Reusable blocking notice modal state shared by acknowledgement and confirmation prompts.
/// 告知与确认提示共享的可复用阻塞提示弹窗状态。
#[derive(Debug, Clone)]
pub(super) struct WorkbenchNoticeModalState {
    /// Modal title shown in the header row.
    /// 显示在弹窗标题行中的标题。
    pub(super) title: String,
    /// Modal body explaining the condition that needs acknowledgement.
    /// 解释需要确认条件的弹窗正文。
    pub(super) body: String,
    /// Primary confirmation button label.
    /// 主确认按钮标签。
    pub(super) confirm_label: String,
    /// Optional secondary dismiss button label.
    /// 可选次要取消按钮标签。
    pub(super) dismiss_label: Option<String>,
    /// Domain action executed when the primary confirmation button is pressed.
    /// 点击主确认按钮时执行的业务动作。
    pub(super) confirm_action: WorkbenchNoticeConfirmAction,
}

/// Per-question editable answer draft retained while the blocking ask/question modal is visible.
/// 阻塞式 ask/question 弹窗可见期间保留的逐题可编辑答案草稿。
#[derive(Debug, Clone, Default)]
pub(super) struct WorkbenchQuestionAnswerDraft {
    /// Selected option labels currently staged for this question.
    /// 当前为该问题暂存的已选选项标签集合。
    pub(super) selected_labels: Vec<String>,
    /// Whether the built-in custom-answer lane is currently enabled for this question.
    /// 当前该问题是否启用了内置“其他回答”通道。
    pub(super) custom_selected: bool,
    /// Free-text answer staged for the built-in custom-answer lane.
    /// 当前为内置“其他回答”通道暂存的自由文本回答。
    pub(super) custom_text: String,
}

/// Active stage shown by the blocking ask/question modal while one runtime request is waiting for the user.
/// 单条运行时问答请求等待用户期间阻塞式 ask/question 弹窗展示的当前阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkbenchQuestionModalStage {
    /// Question-by-question editing stage used to gather or adjust each answer draft.
    /// 逐题编辑阶段，用于收集或调整每道题的答案草稿。
    Answering,
    /// Final confirmation stage used to review every answer before submission.
    /// 最终确认阶段，用于在提交前复核所有答案。
    Confirming,
}

/// Origin describing how the current ask/question modal was opened.
/// 描述当前 ask/question 弹窗是通过哪条链路打开的来源标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkbenchQuestionModalOrigin {
    /// Runtime-backed ask request created by a real model tool invocation.
    /// 由真实模型工具调用创建的运行时 ask 请求。
    Runtime,
    /// Local example-panel debugging flow used only for ask-style iteration.
    /// 仅用于 ask 样式迭代的本地示例面板调试链路。
    AskDebug,
}

/// Blocking ask/question modal state mirrored from the shared question runtime while one tool call is waiting for user input.
/// 当某条工具调用正在等待用户输入时，从共享问题运行时镜像而来的阻塞式 ask/question 弹窗状态。
#[derive(Debug, Clone)]
pub(super) struct WorkbenchQuestionModalState {
    /// Canonical pending request currently being answered by the user.
    /// 当前正由用户作答的规范待回答请求。
    pub(super) request: QuestionRequest,
    /// Source that opened the current modal so submit/cancel can route correctly.
    /// 打开当前弹窗的来源，用于让提交与取消动作走到正确分支。
    pub(super) origin: WorkbenchQuestionModalOrigin,
    /// Ordered answer drafts aligned with the request question array.
    /// 与请求问题数组一一对齐的有序答案草稿集合。
    pub(super) drafts: Vec<WorkbenchQuestionAnswerDraft>,
    /// Zero-based question index currently shown in the editor stage.
    /// 当前编辑阶段正在展示的零基问题下标。
    pub(super) active_question_index: usize,
    /// Zero-based highlighted option index inside the active question, including the synthetic custom-answer row.
    /// 当前激活问题内部高亮的零基选项下标，包含合成的“其他回答”行。
    pub(super) active_option_index: usize,
    /// Current modal stage used to switch between answer editing and final confirmation.
    /// 当前弹窗阶段，用于在答案编辑与最终确认之间切换。
    pub(super) stage: WorkbenchQuestionModalStage,
    /// Optional inline validation error shown when the user tries to submit incomplete answers.
    /// 当用户尝试提交不完整答案时展示的可选内联校验错误文本。
    pub(super) validation_error: Option<String>,
}

/// Blocking image preview modal state opened from one staged image attachment tile.
/// 从已暂存图片附件方格打开的阻塞式图片预览弹窗状态。
#[derive(Debug, Clone)]
pub(super) struct WorkbenchImagePreviewModalState {
    /// File name shown in the preview modal header.
    /// 显示在预览弹窗标题栏中的文件名。
    pub(super) file_name: String,
    /// Local image path rendered by the preview surface.
    /// 由预览区域渲染的本地图片路径。
    pub(super) local_path: String,
    /// Current zoom factor applied to the preview image surface.
    /// 当前应用到图片预览区域的缩放倍率。
    pub(super) zoom: f32,
    /// Last window cursor position captured while left-button panning is active.
    /// 左键平移激活时捕获的上一帧窗口光标位置。
    pub(super) pan_pointer: Option<iced::Point>,
    /// Latest local cursor position tracked inside the preview image surface.
    /// 最近一次在图片预览区域内部追踪到的局部光标位置。
    pub(super) preview_pointer: Option<iced::Point>,
    /// Latest absolute scroll offset reported by the preview image scrollable.
    /// 图片预览滚动容器最近一次回传的绝对滚动偏移。
    pub(super) scroll_offset: scrollable::AbsoluteOffset,
}

/// Host-side generated-audio playback state retained while one inline audio strip is actively playing.
/// 当某条内联音频条正在播放时保留的宿主侧生成音频播放状态。
pub(super) struct GeneratedAudioPlaybackState {
    /// Local absolute path of the currently playing audio file.
    /// 当前正在播放的音频文件本地绝对路径。
    pub(super) local_path: String,
    /// Output stream kept alive for the lifetime of the active sink.
    /// 为当前激活播放 sink 保活的输出流。
    pub(super) _stream: OutputStream,
    /// Active sink responsible for decoding and playing the generated audio file.
    /// 负责解码并播放生成音频文件的激活 sink。
    pub(super) sink: Sink,
}

/// Live scrollbar geometry derived from the current composer editor content and viewport.
/// 根据当前提示词编辑器内容与视口推导出的实时滚动条几何信息。
#[derive(Debug, Clone, Copy)]
pub(super) struct ComposerScrollbarLayout {
    /// Height of the scrollbar track after applying vertical paddings.
    /// 应用上下留白后的滚动条轨道高度。
    pub(super) track_height: f32,
    /// Height of the draggable scrollbar thumb.
    /// 可拖拽滚动条滑块高度。
    pub(super) thumb_height: f32,
    /// Maximal distance the scrollbar thumb can travel inside the track.
    /// 滚动条滑块在轨道内部可移动的最大距离。
    pub(super) thumb_travel: f32,
    /// Current thumb top offset inside the track.
    /// 当前滑块在轨道内部的顶部偏移量。
    pub(super) thumb_top: f32,
    /// Maximal vertical scroll offset allowed by the current content.
    /// 当前内容允许的最大纵向滚动偏移量。
    pub(super) max_scroll_offset: f32,
}
