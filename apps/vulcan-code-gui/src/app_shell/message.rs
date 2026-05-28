//! Workbench message definitions extracted from the main workbench module.
//! 从主工作台模块提取出的工作台消息定义。

use super::*;

/// Messages emitted by iced widgets in the first prototype.
/// 首版原型中由 iced 控件发出的消息集合。
#[derive(Debug, Clone)]
pub(super) enum Message {
    /// Ignores one callback branch without mutating any state.
    /// 忽略某个回调分支且不修改任何状态。
    Noop,
    /// Starts the controller-backed startup bootstrap sequence.
    /// 启动基于控制器的启动引导流程。
    BeginStartupBootstrap,
    /// Applies the completed controller bootstrap report.
    /// 应用已完成的控制器引导报告。
    StartupBootstrapFinished(Result<ControllerBootstrapReport, String>),
    /// Applies the completed LuaSkills bootstrap report.
    /// 应用已完成的 LuaSkills 引导报告。
    LuaSkillsBootstrapFinished(Result<LuaSkillsBootstrapReport, String>),
    /// Applies the completed token-budget runtime bootstrap result.
    /// 应用已完成的 Token 预算运行时引导结果。
    TokenBudgetRuntimeInitialized(std::sync::Arc<TokenBudgetRuntime>),
    /// Applies the completed MCP runtime bootstrap result.
    /// 应用已完成的 MCP 运行时引导结果。
    McpRuntimeBootstrapped(Result<McpRuntimeAccess, String>),
    /// Applies the completed workspace-state load result after the shared runtimes become ready.
    /// 应用共享运行时就绪后已完成的工作台状态加载结果。
    WorkspaceStateLoaded(Result<WorkspaceSnapshot, String>),
    /// Applies the completed lazy hydration result for one opened session.
    /// 应用单个已打开会话的懒加载水合结果。
    SessionHydrationLoaded(SessionId, Result<SessionHydrationSnapshot, String>),
    /// Applies one background-built unified transcript projection snapshot for the given session.
    /// 应用指定会话在后台构建完成的一份统一转录投影视图快照。
    SessionAgentViewProjectionLoaded(
        SessionId,
        Result<SessionAgentViewProjectionSnapshot, String>,
    ),
    /// Applies one older transcript page loaded before the current session window.
    /// 应用在当前会话窗口之前加载的一页更早转录历史。
    OlderSessionHistoryLoaded(SessionId, Result<SessionHistoryPage, String>),
    /// Applies the completed background legacy-storage maintenance result for one session.
    /// 应用单个会话的后台旧存储维护结果。
    BackgroundLegacySessionMaintenanceFinished(SessionId, Result<SessionMaintenanceReport, String>),
    /// Applies the completed project-scoped temp cleanup result after startup or one project-load action.
    /// 应用启动后或单次项目载入动作后已完成的项目级临时目录清理结果。
    ProjectTempMaintenanceFinished(Result<(), String>),
    /// Applies the completed workspace-state persistence result after one user-visible mutation.
    /// 应用某次用户可见变更后已完成的工作台状态持久化结果。
    WorkspaceStatePersisted(Result<WorkspaceSnapshotPersistOutcome, String>),
    /// Main workbench window opened successfully.
    /// 主工作台窗口已成功打开。
    MainWindowOpened(window::Id),
    /// Dedicated diff preview window opened successfully.
    /// 独立 Diff 预览窗口已成功打开。
    DiffWindowOpened(window::Id),
    /// Requests opening or focusing the dedicated diff preview window.
    /// 请求打开或聚焦独立 Diff 预览窗口。
    OpenDiffWindow,
    /// Opens the local ask debug modal from the example panel for style iteration.
    /// 从示例面板打开本地 ask 调试弹窗，供样式迭代使用。
    OpenAskDebugModal,
    /// Records that one custom title bar drag region has been pressed.
    /// 记录某个自定义标题栏拖拽区域已被按下。
    WindowTitleBarPressed(window::Id),
    /// Updates the latest pointer position inside one custom title bar drag region.
    /// 更新某个自定义标题栏拖拽区域内的最新鼠标位置。
    WindowTitleBarPointerMoved(window::Id, iced::Point),
    /// Clears the pending drag candidate after one custom title bar release.
    /// 在某次自定义标题栏释放后清理待确认拖动候选状态。
    WindowTitleBarReleased(window::Id),
    /// Clears the tracked pointer state after the cursor exits one custom title bar drag region.
    /// 在鼠标离开某个自定义标题栏拖拽区域后清理追踪状态。
    WindowTitleBarExited(window::Id),
    /// Requests minimizing the target window from the custom title bar.
    /// 请求从自定义标题栏最小化目标窗口。
    MinimizeWindow(window::Id),
    /// Requests starting native window resize from one undecorated edge or corner.
    /// 请求从无系统装饰窗口的某个边缘或角开始原生缩放。
    StartWindowResize(window::Id, window::Direction),
    /// Requests toggling maximized state for the target window from the custom title bar.
    /// 请求从自定义标题栏切换目标窗口的最大化状态。
    ToggleMaximizeWindow(window::Id),
    /// Requests closing the target window from the custom title bar.
    /// 请求从自定义标题栏关闭目标窗口。
    CloseWindow(window::Id),
    /// Toggles the left task drawer shown inside the main workbench panel.
    /// 切换显示在主工作台面板中的左侧任务抽屉。
    ToggleMainTaskPanel,
    /// Starts resizing the left task drawer shown inside the main workbench panel.
    /// 开始缩放显示在主工作台面板中的左侧任务抽屉。
    StartMainTaskPanelResize,
    /// Updates whether the task-drawer resize handle is currently hovered.
    /// 更新任务抽屉缩放柄当前是否处于悬停状态。
    MainTaskPanelResizeHandleHovered(bool),
    /// Reports the latest maximized state of one tracked window.
    /// 回传某个受跟踪窗口当前最新的最大化状态。
    WindowMaximizedStateChanged(window::Id, bool),
    /// Switches the active top-level panel shown inside the main display region.
    /// 切换显示在主显示区域中的顶层面板。
    MainDisplayPanelSelected(MainDisplayPanel),
    /// A window was closed by the user or the runtime.
    /// 某个窗口已被用户或运行时关闭。
    WindowClosed(window::Id),
    /// A window was resized by the runtime.
    /// 某个窗口的尺寸已被运行时更新。
    WindowResized(window::Id, iced::Size),
    /// Reports a pointer move tracked at the window level.
    /// 回传在窗口级别追踪到的鼠标移动事件。
    WindowPointerMoved(window::Id, iced::Point),
    /// Tracks the latest keyboard modifier state reported by the window event stream.
    /// 跟踪窗口事件流回传的最新键盘修饰键状态。
    WorkbenchKeyboardModifiersChanged(keyboard::Modifiers),
    /// Reports that the left mouse button was released at the window level.
    /// 回传在窗口级别追踪到的鼠标左键释放事件。
    WindowLeftButtonReleased(window::Id),
    /// Workspace path changed from the text input.
    /// 工作区路径在文本输入框中发生变化。
    WorkspacePathChanged(String),
    /// Reports that workspace-level project rules may have changed on disk.
    /// 回传工作区级项目规则可能已在磁盘上发生变化。
    ProjectRulesWatchChanged {
        /// Workspace path owning the changed project-rules file.
        /// 拥有变化项目规则文件的工作区路径。
        workspace_path: String,
    },
    /// Reports that prompt resources may have changed on disk.
    /// 回传提示词资源可能已在磁盘上发生变化。
    PromptResourcesWatchChanged {
        /// Workspace path associated with the prompt-resource watch.
        /// 与提示词资源监听关联的工作区路径。
        workspace_path: String,
        /// Prompt-resource category that changed.
        /// 发生变化的提示词资源类别。
        kind: PromptResourceWatchKind,
    },
    /// Prompt composer changed from the text input.
    /// 提示词输入框内容发生变化。
    ComposerChanged(String),
    /// Applies one text-editor action inside the main prompt composer.
    /// 应用主提示词编辑器中的一条文本编辑动作。
    ComposerEditorAction(text_editor::Action),
    /// Moves the highlighted slash-command suggestion one row upward inside the composer inline panel.
    /// 将提示词输入框内联面板中的高亮 slash 指令建议向上移动一行。
    ComposerSlashCommandMoveSelectionUp,
    /// Moves the highlighted slash-command suggestion one row downward inside the composer inline panel.
    /// 将提示词输入框内联面板中的高亮 slash 指令建议向下移动一行。
    ComposerSlashCommandMoveSelectionDown,
    /// Activates one slash-command suggestion from the composer inline panel.
    /// 从提示词输入框内联面板激活某条 slash 指令建议。
    ActivateComposerSlashCommand(ComposerSlashCommand),
    /// Dismisses the inline slash-command suggestion panel while preserving the current composer text.
    /// 在保留当前提示词文本的前提下关闭内联 slash 指令建议面板。
    DismissComposerSlashCommandPanel,
    /// Restores the previous composer snapshot requested by the undo shortcut.
    /// 恢复由撤销快捷键请求的上一条提示词快照。
    ComposerUndo,
    /// Opens the native file picker and appends one or more supported attachments into the composer.
    /// 打开原生文件选择器并向提示词输入框追加一个或多个受支持附件。
    AddComposerAttachments,
    /// Smart-pastes clipboard contents into the composer by preferring file attachments, then in-memory images, and finally plain text.
    /// 按“文件附件、内存图片、普通文本”的优先顺序，将剪贴板内容智能粘贴到提示词输入框。
    PasteComposerClipboardContents,
    /// Pastes file or image attachments from the current clipboard into the composer.
    /// 将当前剪贴板中的文件或图片附件粘贴到提示词输入框。
    PasteComposerClipboardAttachments,
    /// Removes one staged composer attachment identified by its canonical local path.
    /// 根据规范本地路径移除一条已暂存的提示词附件。
    RemoveComposerAttachment(String),
    /// Opens the blocking image preview modal for one staged image attachment.
    /// 为某个已暂存图片附件打开阻塞式图片预览弹窗。
    OpenComposerImageAttachmentPreview {
        /// File name shown in the image preview modal header.
        /// 显示在图片预览弹窗标题栏中的文件名。
        file_name: String,
        /// Local image path rendered by the image preview modal.
        /// 由图片预览弹窗渲染的本地图片路径。
        local_path: String,
    },
    /// Opens one generated media file with the host operating system default application.
    /// 使用宿主操作系统默认应用打开某个生成媒体文件。
    OpenGeneratedAttachmentWithDefaultApp(String),
    /// Reveals one generated media file in the host operating system file manager.
    /// 在宿主操作系统文件管理器中定位某个生成媒体文件。
    RevealGeneratedAttachmentInFolder(String),
    /// Copies one generated media file path into the system clipboard.
    /// 将某个生成媒体文件路径复制到系统剪贴板。
    CopyGeneratedAttachmentPath(String),
    /// Toggles host-side playback for one generated audio file.
    /// 为某个生成音频文件切换宿主侧播放状态。
    ToggleGeneratedAudioPlayback {
        /// Human-readable file name used by status text and notices.
        /// 用于状态文本与提示弹窗的人类可读文件名。
        file_name: String,
        /// Local absolute path of the generated audio file.
        /// 生成音频文件的本地绝对路径。
        local_path: String,
    },
    /// Copies one generated image file into the system clipboard as bitmap content.
    /// 将某个生成图片文件以位图内容形式复制到系统剪贴板。
    CopyGeneratedImageToClipboard {
        /// Human-readable file name used by status text and notices.
        /// 用于状态文本与提示弹窗的人类可读文件名。
        file_name: String,
        /// Local absolute path of the generated image file.
        /// 生成图片文件的本地绝对路径。
        local_path: String,
    },
    /// Closes the currently visible image preview modal.
    /// 关闭当前可见的图片预览弹窗。
    CloseWorkbenchImagePreviewModal,
    /// Adjusts the current image preview zoom factor by one signed delta.
    /// 按一个有符号增量调整当前图片预览缩放倍率。
    AdjustWorkbenchImagePreviewZoom(f32),
    /// Handles one captured mouse-wheel movement while the image preview modal is visible.
    /// 处理图片预览弹窗可见时捕获到的一次鼠标滚轮移动。
    WorkbenchImagePreviewWheelScrolled(iced::mouse::ScrollDelta),
    /// Starts one left-button pan interaction inside the image preview surface.
    /// 在图片预览区域内部启动一次鼠标左键平移交互。
    WorkbenchImagePreviewLeftPressed,
    /// Ends the active left-button pan interaction inside the image preview surface.
    /// 结束图片预览区域内部当前激活的鼠标左键平移交互。
    WorkbenchImagePreviewLeftReleased,
    /// Tracks the latest local pointer position inside the image preview surface.
    /// 跟踪图片预览区域内部最近一次局部鼠标位置。
    WorkbenchImagePreviewPointerMoved(iced::Point),
    /// Tracks the latest scrollable viewport emitted by the image preview surface.
    /// 跟踪图片预览区域滚动容器最近一次回传的视口状态。
    WorkbenchImagePreviewScrolled(scrollable::Viewport),
    /// Resets the current image preview zoom factor to its default fit value.
    /// 将当前图片预览缩放倍率还原到默认适配值。
    ResetWorkbenchImagePreviewZoom,
    /// Confirms and closes the currently visible reusable workbench notice modal.
    /// 确认并关闭当前可见的可复用工作台提示弹窗。
    ConfirmWorkbenchNoticeModal,
    /// Closes the currently visible reusable workbench notice modal without changing domain state.
    /// 关闭当前可见的可复用工作台提示弹窗且不修改业务状态。
    CloseWorkbenchNoticeModal,
    /// Polls the shared question runtime and mirrors the oldest pending ask/question request into the main workbench modal layer.
    /// 轮询共享问题运行时，并把最早的待回答 ask/question 请求镜像到主工作台弹层。
    QuestionRuntimePollTick,
    /// Toggles one structured or custom option row inside the currently visible ask/question modal.
    /// 在当前可见的 ask/question 弹窗中切换一条结构化或自定义选项行。
    QuestionModalOptionPressed {
        /// Zero-based question index inside the pending request.
        /// 待回答请求中的零基问题下标。
        question_index: usize,
        /// Zero-based option index inside the rendered list, where the last row is the synthetic custom-answer entry.
        /// 当前渲染列表中的零基选项下标，其中最后一行代表合成的“其他回答”条目。
        option_index: usize,
    },
    /// Updates the custom free-text answer draft for one prompt inside the currently visible ask/question modal.
    /// 更新当前可见 ask/question 弹窗内某个提示的自定义自由文本答案草稿。
    QuestionModalCustomTextChanged {
        /// Zero-based question index inside the pending request.
        /// 待回答请求中的零基问题下标。
        question_index: usize,
        /// Latest custom free-text answer draft entered by the user.
        /// 用户输入的最新自定义自由文本答案草稿。
        value: String,
    },
    /// Selects one question tab inside the currently visible ask/question modal and returns the modal to the editing stage.
    /// 选择当前可见 ask/question 弹窗中的某个问题标签，并让弹窗回到编辑阶段。
    QuestionModalSelectQuestionTab(usize),
    /// Selects the dedicated confirmation tab inside the currently visible ask/question modal.
    /// 选择当前可见 ask/question 弹窗中的专用确认标签页。
    QuestionModalSelectConfirmTab,
    /// Moves the active ask/question modal editor to the previous question.
    /// 将当前 ask/question 弹窗编辑焦点移动到上一道问题。
    QuestionModalMoveToPreviousQuestion,
    /// Moves the active ask/question modal editor to the next question.
    /// 将当前 ask/question 弹窗编辑焦点移动到下一道问题。
    QuestionModalMoveToNextQuestion,
    /// Toggles the currently highlighted option row inside the active ask/question modal question.
    /// 在当前 ask/question 弹窗的激活问题中切换当前高亮选项行。
    QuestionModalToggleHighlightedOption,
    /// Submits the currently visible ask/question modal back into the shared question runtime.
    /// 将当前可见 ask/question 弹窗提交回共享问题运行时。
    SubmitQuestionModal,
    /// Dismisses the top-most keyboard-driven overlay, preferring the reusable notice modal before legacy overlays.
    /// 关闭键盘驱动的最顶层遮罩，优先处理可复用提示弹窗再回退到旧弹层。
    DismissTopLevelOverlay,
    /// Confirms the top-most keyboard-driven overlay, preferring the reusable notice modal before model selection.
    /// 确认键盘驱动的最顶层遮罩，优先处理可复用提示弹窗再回退到模型选择。
    ConfirmTopLevelOverlay,
    /// Tracks one supported or unsupported file currently hovering over the main window as a potential composer drop target.
    /// 跟踪当前悬停在主窗口上、可作为提示词拖放目标的一条受支持或不受支持文件。
    WindowFileHovered(window::Id, PathBuf),
    /// Reports that hovered files have left the target window and the composer drop preview should clear.
    /// 回传悬停文件已离开目标窗口，提示词拖放预览应被清空。
    WindowFilesHoverLeft(window::Id),
    /// Reports that one file has been dropped onto the target window for composer attachment import.
    /// 回传某个文件已被拖放到目标窗口，供提示词附件导入使用。
    WindowFileDropped(window::Id, PathBuf),
    /// Opens the composer context menu at the latest tracked cursor position.
    /// 在最近一次追踪到的鼠标位置打开提示词上下文菜单。
    OpenComposerContextMenu,
    /// Opens the shared context menu for one visible task project row.
    /// 为一条可见任务项目行打开共享右键菜单。
    OpenTaskProjectContextMenu(String),
    /// Opens the shared context menu for one visible task session row.
    /// 为一条可见任务会话行打开共享右键菜单。
    OpenTaskSessionContextMenu(SessionId),
    /// Opens the shared context menu for one archived task project row.
    /// 为一条已归档任务项目行打开共享右键菜单。
    OpenArchivedTaskProjectContextMenu(String),
    /// Opens the shared context menu for one archived task session row.
    /// 为一条已归档任务会话行打开共享右键菜单。
    OpenArchivedTaskSessionContextMenu(SessionId),
    /// Opens the shared context menu requested by one selectable transcript surface.
    /// 打开某个可选择转录表面请求的共享右键菜单。
    OpenTranscriptContextMenu(SelectableTranscriptContextMenuRequest),
    /// Closes the current shared workbench context menu without invoking an item.
    /// 在不触发菜单项的情况下关闭当前共享工作台右键菜单。
    CloseWorkbenchContextMenu,
    /// Invokes one item from the currently open shared workbench context menu.
    /// 触发当前已打开共享工作台右键菜单中的某一项。
    InvokeWorkbenchContextMenuItem(usize),
    /// Copies the current composer selection through the context menu.
    /// 通过上下文菜单复制当前提示词选区。
    ComposerContextMenuCopy,
    /// Cuts the current composer selection through the context menu.
    /// 通过上下文菜单剪切当前提示词选区。
    ComposerContextMenuCut,
    /// Pastes the current clipboard text through the context menu.
    /// 通过上下文菜单粘贴当前剪贴板文本。
    ComposerContextMenuPaste,
    /// Selects all composer contents through the context menu.
    /// 通过上下文菜单全选提示词内容。
    ComposerContextMenuSelectAll,
    /// Copies the selected transcript text captured when its context menu opened.
    /// 复制打开转录右键菜单时捕获的转录选中文本。
    TranscriptContextMenuCopySelection(String),
    /// Selects all text in the transcript surface that opened the shared context menu.
    /// 全选打开共享右键菜单的转录表面中的文本。
    TranscriptContextMenuSelectAll(SelectableTranscriptSurfaceId),
    /// Applies the clipboard text snapshot loaded for the composer context menu.
    /// 应用为提示词上下文菜单载入的剪贴板文本快照。
    ComposerContextClipboardLoaded(Result<std::sync::Arc<String>, clipboard::Error>),
    /// Updates the latest local pointer y position tracked inside the composer editor surface.
    /// 更新在提示词编辑器表面内部追踪到的最新局部纵向指针位置。
    ComposerScrollbarPointerMoved(f32),
    /// Starts one scrollbar drag interaction using the latest tracked pointer position.
    /// 使用最近一次追踪到的指针位置启动一次滚动条拖拽交互。
    StartComposerScrollbarDrag,
    /// Embedded editor emitted an editing action.
    /// 嵌入式编辑器发出了编辑动作。
    EditorActionPerformed(text_editor::Action),
    /// Example ask-debug Markdown preview emitted one selectable read-only action.
    /// 示例 ask 调试 Markdown 预览发出了一次可选择只读动作。
    ExamplePreviewMarkdownAction(text_editor::Action),
    /// Supported provider selected from the provider catalog picker.
    /// 从供应商目录选择框中选中了某个受支持供应商。
    ProviderCatalogSelected(ProviderChoice),
    /// Selects one dedicated section inside the settings workspace.
    /// 选中设置工作区中的某个独立分区。
    SettingsSectionSelected(SettingsSection),
    /// Toggles whether the AI capability subtree stays expanded in the left settings tree.
    /// 切换左侧设置树中的 AI 能力子树是否保持展开。
    ToggleAiCapabilitySettingsTree,
    /// Toggles whether the model-settings subtree stays expanded in the left settings tree.
    /// 切换左侧设置树中的模型设置子树是否保持展开。
    ToggleModelSettingsTree,
    /// One provider form field changed in the configuration page.
    /// 配置页面中的某个供应商表单字段发生变化。
    ProviderFieldChanged(ProviderFieldKey, String),
    /// Opens the blocking editor used to create one brand-new provider configuration entry.
    /// 打开用于创建全新供应商配置条目的阻塞式编辑弹窗。
    OpenProviderEditorForCreate,
    /// Opens the blocking editor used to update one existing configured provider entry.
    /// 打开用于更新某条已有供应商配置的阻塞式编辑弹窗。
    OpenProviderEditorForEdit(String),
    /// Closes the currently visible provider editor without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的供应商编辑弹窗。
    CloseProviderEditor,
    /// Persists the currently selected provider configuration.
    /// 持久化当前选中供应商的配置。
    SaveProviderConfiguration,
    /// Opens the blocking model-picker modal used to edit one configured provider's enabled-model draft.
    /// 打开用于编辑某个已配置供应商启用模型草稿的阻塞式模型配置弹窗。
    OpenProviderModelPicker(String),
    /// Closes the currently visible provider model-picker modal without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的供应商模型配置弹窗。
    CloseProviderModelPicker,
    /// Selects one provider row inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部选中某条供应商行。
    ProviderModelPickerProviderSelected(String),
    /// Updates the live filter text used by the blocking provider model-picker modal.
    /// 更新阻塞式供应商模型配置弹窗使用的实时过滤文本。
    ProviderModelPickerFilterChanged(String),
    /// Toggles one staged enabled-model checkbox inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部切换某个暂存启用模型勾选框。
    ProviderModelPickerModelToggled {
        /// Provider identifier owning the staged catalog row.
        /// 拥有该暂存目录行的供应商标识。
        provider_id: String,
        /// Model identifier being toggled in the staged enabled-model set.
        /// 当前在暂存启用模型集合中被切换的模型标识。
        model_id: String,
        /// Whether the staged model should stay enabled after the toggle.
        /// 当前暂存模型在切换后是否应保持启用。
        enabled: bool,
    },
    /// Selects every currently visible model row inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部选中当前全部可见模型行。
    ProviderModelPickerSelectAllVisible,
    /// Clears every currently visible model row inside the blocking provider model-picker modal.
    /// 在阻塞式供应商模型配置弹窗内部取消当前全部可见模型行。
    ProviderModelPickerClearAllVisible,
    /// Persists the staged enabled-model draft from the blocking provider model-picker modal.
    /// 将阻塞式供应商模型配置弹窗中的启用模型草稿持久化保存。
    SaveProviderModelPicker,
    /// Removes one configured provider entry chosen from the provider result list.
    /// 从供应商结果列表中移除一条已配置供应商条目。
    RemoveConfiguredProvider(String),
    /// Opens the blocking per-model metadata editor used to correct one enabled model entry.
    /// 打开用于修正某条已启用模型元数据的阻塞式逐模型编辑弹窗。
    OpenProviderModelOverrideEditor {
        /// Provider identifier owning the selected model.
        /// 拥有当前所选模型的供应商标识。
        provider_id: String,
        /// Model identifier whose override metadata should be edited.
        /// 需要编辑其覆盖元数据的模型标识。
        model_id: String,
    },
    /// Closes the currently visible per-model metadata editor without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的逐模型元数据编辑弹窗。
    CloseProviderModelOverrideEditor,
    /// Updates the staged reasoning support flag inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存推理能力标记。
    ProviderModelOverrideReasoningChanged(bool),
    /// Updates the staged attachment declaration flag inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存附件声明标记。
    ProviderModelOverrideAttachmentChanged(bool),
    /// Toggles one staged input modality flag inside the per-model metadata editor.
    /// 在逐模型元数据编辑弹窗中切换某个暂存输入模态标记。
    ProviderModelOverrideInputModalityToggled {
        /// Normalized modality name being toggled.
        /// 当前被切换的规范化模态名称。
        modality: String,
        /// Whether the staged modality should stay enabled.
        /// 当前暂存模态在切换后是否应保持启用。
        enabled: bool,
    },
    /// Updates the staged context-window limit text inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存上下文窗口上限文本。
    ProviderModelOverrideContextLimitChanged(String),
    /// Updates the staged input-token limit text inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存输入 Token 上限文本。
    ProviderModelOverrideInputLimitChanged(String),
    /// Updates the staged output-token limit text inside the per-model metadata editor.
    /// 更新逐模型元数据编辑弹窗中的暂存输出 Token 上限文本。
    ProviderModelOverrideOutputLimitChanged(String),
    /// Applies one text-editor action inside the staged reasoning-profile JSON editor.
    /// 在暂存推理档位 JSON 编辑器中应用一条文本编辑器动作。
    ProviderModelOverrideReasoningProfileAction(text_editor::Action),
    /// Persists the staged per-model override metadata back into the provider configuration file.
    /// 将暂存的逐模型覆盖元数据持久化回供应商配置文件。
    SaveProviderModelOverrideEditor,
    /// Selects the global web-search routing mode.
    /// 选择全局网页搜索路由模式。
    WebSearchRoutingModeSelected(WebSearchRoutingMode),
    /// Selects the global native-versus-host routing mode for image understanding.
    /// 选择图像理解的全局原生与宿主桥接路由模式。
    UnderstandImageRoutingModeSelected(NativeBridgeRoutingMode),
    /// Opens the compact routing-strategy picker used by one concrete capability settings page.
    /// 打开某个具体能力设置页使用的紧凑路由策略选择弹窗。
    OpenAiCapabilityRoutingPicker(AiToolKind),
    /// Closes the currently visible compact routing-strategy picker without changing the active mode.
    /// 在不修改当前模式的前提下关闭当前可见的紧凑路由策略选择弹窗。
    CloseAiCapabilityRoutingPicker,
    /// Toggles one configured semantic capability target directly from the settings page or quick capability panel.
    /// 直接从设置页或快捷能力面板切换某条已配置语义能力目标的启用状态。
    AiCapabilityTargetEnabledToggled {
        /// Semantic capability owning the target row.
        /// 拥有该目标行的语义能力。
        capability_kind: AiToolKind,
        /// Stable vendor identifier owning the target.
        /// 拥有该目标的稳定供应商标识。
        vendor_id: String,
        /// Stable target alias being toggled.
        /// 当前切换的稳定目标别名。
        target_alias: String,
        /// Whether the configured target should stay enabled.
        /// 当前已配置目标是否应保持启用。
        enabled: bool,
    },
    /// Removes one shared AI capability vendor instance.
    /// 移除一条共享 AI 能力供应商实例。
    RemoveAiCapabilityVendor(String),
    /// Opens the vendor picker used by the supplier page before one new shared AI capability vendor is added.
    /// 打开供应商页在新增共享 AI 能力供应商前使用的供应商选择器。
    OpenAiCapabilityVendorPicker,
    /// Closes the currently visible AI capability vendor picker without creating any vendor.
    /// 关闭当前可见的 AI 能力供应商选择器，且不创建任何供应商。
    CloseAiCapabilityVendorPicker,
    /// Updates the live keyword filter shown inside the AI capability vendor picker.
    /// 更新 AI 能力供应商选择器中展示的实时关键字过滤值。
    AiCapabilityVendorPickerFilterChanged(String),
    /// Chooses one vendor from the AI capability vendor picker and opens the dedicated vendor editor.
    /// 从 AI 能力供应商选择器中选中一个供应商并打开独立供应商编辑器。
    AiCapabilityVendorPickerVendorSelected(String),
    /// Opens the dedicated editor for one already configured shared AI capability vendor.
    /// 为一个已配置的共享 AI 能力供应商打开独立编辑器。
    OpenAiCapabilityVendorEditor(String),
    /// Closes the currently visible shared AI capability vendor editor without saving staged changes.
    /// 关闭当前可见的共享 AI 能力供应商编辑器且不保存暂存改动。
    CloseAiCapabilityVendorEditor,
    /// Updates the staged API key value inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存 API Key 值。
    AiCapabilityVendorEditorApiKeyChanged(String),
    /// Updates the staged local/cloud connection mode inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存本地/云端连接模式。
    AiCapabilityVendorEditorConnectionModeChanged(
        crate::provider_config::AiCapabilityVendorConnectionMode,
    ),
    /// Updates the staged Base URL value inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存 Base URL 值。
    AiCapabilityVendorEditorBaseUrlChanged(String),
    /// Updates the staged region value inside the currently visible shared AI capability vendor editor.
    /// 更新当前可见共享 AI 能力供应商编辑器中的暂存区域值。
    AiCapabilityVendorEditorRegionChanged(String),
    /// Persists the staged shared AI capability vendor editor values back into the dedicated vendor list.
    /// 将暂存的共享 AI 能力供应商编辑器内容持久化回独立供应商列表。
    SaveAiCapabilityVendorEditor,
    /// Opens the batch target-picker modal for one semantic AI capability page.
    /// 为某个语义 AI 能力页打开批量目标选择弹窗。
    OpenAiCapabilityTargetPicker(AiToolKind),
    /// Closes the currently visible AI capability target-picker modal without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的 AI 能力目标选择弹窗。
    CloseAiCapabilityTargetPicker,
    /// Selects one vendor row inside the AI capability target-picker modal.
    /// 在 AI 能力目标选择弹窗中选中某一条供应商行。
    AiCapabilityTargetPickerVendorSelected(String),
    /// Toggles one staged target checkbox inside the AI capability target-picker modal.
    /// 在 AI 能力目标选择弹窗中切换某个暂存目标勾选框。
    AiCapabilityTargetPickerTargetToggled {
        /// Stable vendor identifier owning the staged target.
        /// 拥有该暂存目标的稳定供应商标识。
        vendor_id: String,
        /// Stable target alias being toggled.
        /// 当前切换的稳定目标别名。
        target_alias: String,
        /// Whether the staged target should stay selected.
        /// 当前暂存目标是否应保持选中。
        enabled: bool,
    },
    /// Updates the staged request model attached to one selected target inside the AI capability target-picker modal.
    /// 更新 AI 能力目标选择弹窗中某个已选目标绑定的暂存请求模型。
    AiCapabilityTargetPickerTargetModelChanged {
        /// Stable vendor identifier owning the staged target.
        /// 拥有该暂存目标的稳定供应商标识。
        vendor_id: String,
        /// Stable target alias whose model is being edited.
        /// 当前正在编辑模型的稳定目标别名。
        target_alias: String,
        /// Updated staged provider request model identifier.
        /// 更新后的暂存 Provider 请求模型标识。
        model_id: String,
    },
    /// Persists the current staged capability target selections back into the shared vendor-centric AI capability configuration.
    /// 将当前暂存的能力目标选择持久化回共享供应商中心 AI 能力配置。
    SaveAiCapabilityTargetPicker,
    /// Starts one drag interaction from an enabled AI capability target row handle.
    /// 从某条已启用 AI 能力目标行的拖拽柄启动一次拖拽交互。
    StartAiCapabilityRouteDrag {
        /// Semantic capability owning the route row.
        /// 拥有该路由行的语义能力。
        capability_kind: AiToolKind,
        /// Fully qualified `vendor/target` selector that may become the active drag source.
        /// 可能升级为激活拖拽源的完整 `供应商/目标` 选择器。
        target_selector: String,
    },
    /// Updates the drop-preview target while one AI capability route row is being dragged.
    /// 在拖拽某条 AI 能力路由行期间更新插入目标预览。
    AiCapabilityRouteDropPreviewMoved {
        /// Semantic capability owning the route row under the cursor.
        /// 当前光标所在路由行所属的语义能力。
        capability_kind: AiToolKind,
        /// Fully qualified `vendor/target` selector represented by the hovered row.
        /// 当前悬停行代表的完整 `供应商/目标` 选择器。
        target_selector: String,
        /// Latest local pointer position inside the hovered row.
        /// 当前悬停行内部最近一次局部鼠标位置。
        position: iced::Point,
    },
    /// Updates the tail drop-preview state while dragging inside one AI capability route list.
    /// 在单个 AI 能力路由列表内部拖拽时更新尾部落点预览状态。
    AiCapabilityRouteTailHovered {
        /// Semantic capability owning the tail drop zone.
        /// 拥有该尾部落点的语义能力。
        capability_kind: AiToolKind,
        /// Whether the cursor is currently hovering the tail drop zone.
        /// 当前鼠标是否正悬停在尾部落点之上。
        is_hovered: bool,
    },
    /// Removes one enabled capability target from its semantic capability page.
    /// 从其语义能力页中移除单条已启用能力目标。
    RemoveAiCapabilityRouteTarget {
        /// Semantic capability owning the route order.
        /// 拥有该路由顺序的语义能力。
        capability_kind: AiToolKind,
        /// Fully qualified `vendor/target` selector to remove.
        /// 需要移除的完整 `供应商/目标` 选择器。
        target_selector: String,
    },
    /// Opens the workbench quick AI capability panel from the composer controls.
    /// 从提示词控制区打开工作台快捷 AI 能力面板。
    OpenWorkbenchAiCapabilityPanel,
    /// Closes the workbench quick AI capability panel without changing the selected section.
    /// 在不改变当前分区选择的前提下关闭工作台快捷 AI 能力面板。
    CloseWorkbenchAiCapabilityPanel,
    /// Selects one capability section inside the workbench quick AI capability panel.
    /// 在工作台快捷 AI 能力面板内部选中某个能力分区。
    WorkbenchAiCapabilitySectionSelected(WorkbenchAiCapabilitySection),
    /// Quickly enables or disables one capability from the top checkbox grid of the quick capability panel.
    /// 从快捷能力面板顶部勾选网格中快速启用或关闭某个能力。
    WorkbenchAiCapabilitySectionEnabledToggled {
        /// Quick-panel section whose effective enabled state is being toggled.
        /// 当前被切换生效启用状态的快捷面板分区。
        section: WorkbenchAiCapabilitySection,
        /// Desired enabled state after the toggle interaction.
        /// 当前切换交互结束后期望达到的启用状态。
        enabled: bool,
    },
    /// Selects one globally configured model from the in-app workbench model picker.
    /// 从工作台内嵌模型选择器中选中一个全局已配置模型。
    WorkbenchModelSelected(WorkbenchModelChoice),
    /// Opens the OpenRouter secondary-provider picker for the currently active OpenRouter model.
    /// 为当前激活的 OpenRouter 模型打开二级供应商选择弹窗。
    OpenWorkbenchOpenRouterEndpointPicker,
    /// Closes the OpenRouter secondary-provider picker without changing the remembered route.
    /// 在不修改已记忆路由的前提下关闭 OpenRouter 二级供应商选择弹窗。
    CloseWorkbenchOpenRouterEndpointPicker,
    /// Updates the live filter text shown at the top of the OpenRouter secondary-provider picker.
    /// 更新显示在 OpenRouter 二级供应商选择弹窗顶部的实时过滤文本。
    WorkbenchOpenRouterEndpointPickerFilterChanged(String),
    /// Persists the selected OpenRouter secondary-provider route for the current model, or clears it back to Auto.
    /// 为当前模型持久化保存选中的 OpenRouter 二级供应商路由，或清除回 Auto。
    WorkbenchOpenRouterEndpointSelected(
        Option<crate::provider_config::OpenRouterEndpointRouteSelection>,
    ),
    /// Receives the asynchronously loaded OpenRouter endpoint catalog for one concrete model.
    /// 接收为某个具体模型异步加载得到的 OpenRouter 端点目录结果。
    WorkbenchOpenRouterEndpointsLoaded {
        /// Stable model identifier that owned the request.
        /// 拥有该请求的稳定模型标识。
        model_id: String,
        /// Loaded endpoint catalog or a human-readable error.
        /// 已加载的端点目录或人类可读错误信息。
        result: Result<Vec<WorkbenchOpenRouterEndpointChoice>, String>,
    },
    /// Opens the in-app workbench model-picker modal.
    /// 打开工作台内嵌模型选择弹窗。
    OpenWorkbenchModelPicker,
    /// Closes the in-app workbench model-picker modal without changing the active model.
    /// 在不修改当前激活模型的前提下关闭工作台内嵌模型选择弹窗。
    CloseWorkbenchModelPicker,
    /// Updates the live filter text shown at the top of the workbench model-picker modal.
    /// 更新显示在工作台模型选择弹窗顶部的实时过滤文本。
    WorkbenchModelPickerFilterChanged(String),
    /// Moves the highlighted option upward inside the filtered workbench model-picker result list.
    /// 在工作台模型选择弹窗的过滤结果列表中向上移动高亮选项。
    WorkbenchModelPickerMoveSelectionUp,
    /// Moves the highlighted option downward inside the filtered workbench model-picker result list.
    /// 在工作台模型选择弹窗的过滤结果列表中向下移动高亮选项。
    WorkbenchModelPickerMoveSelectionDown,
    /// Selects the active runtime agent used by compose-based execution.
    /// 选中 compose 执行链路使用的激活运行时代理。
    ActiveAgentSelected(AgentChoice),
    /// Selects one user-authored agent profile for editing in the dedicated Agent panel.
    /// 在独立 Agent 面板中选中一条用户自定义 Agent 配置进行编辑。
    UserAgentProfileSelected(String),
    /// Creates a new editable user-agent draft in the dedicated Agent panel.
    /// 在独立 Agent 面板中创建一份新的用户 Agent 草稿。
    CreateUserAgentProfileDraft,
    /// Updates the editable user-agent identifier field.
    /// 更新可编辑用户 Agent 标识字段。
    UserAgentProfileIdChanged(String),
    /// Updates the editable user-agent display name field.
    /// 更新可编辑用户 Agent 显示名称字段。
    UserAgentProfileNameChanged(String),
    /// Updates the editable user-agent description field.
    /// 更新可编辑用户 Agent 描述字段。
    UserAgentProfileDescriptionChanged(String),
    /// Updates the editable user-agent model override field.
    /// 更新可编辑用户 Agent 模型覆盖字段。
    UserAgentProfileModelChanged(String),
    /// Updates the editable user-agent temperature override field.
    /// 更新可编辑用户 Agent 温度覆盖字段。
    UserAgentProfileTemperatureChanged(String),
    /// Updates the editable user-agent max-turn override field.
    /// 更新可编辑用户 Agent 最大轮次数覆盖字段。
    UserAgentProfileMaxTurnsChanged(String),
    /// Applies one editing action to the editable user-agent prompt buffer.
    /// 向可编辑用户 Agent 提示词缓冲区应用一次编辑动作。
    UserAgentProfilePromptAction(text_editor::Action),
    /// Persists the currently edited user-agent profile to the dedicated user directory.
    /// 将当前编辑中的用户 Agent 配置持久化到独立用户目录。
    SaveUserAgentProfile,
    /// Deletes the currently selected user-agent profile from the dedicated user directory.
    /// 从独立用户目录中删除当前选中的用户 Agent 配置。
    DeleteUserAgentProfile,
    /// Selects one configured MCP server entry for editing.
    /// 选中一个已配置 MCP 服务器条目进行编辑。
    McpServerSelected(McpServerChoice),
    /// Closes the currently visible MCP server editor without saving staged changes.
    /// 在不保存暂存改动的前提下关闭当前可见的 MCP 服务器编辑器。
    CloseMcpServerEditor,
    /// Updates the editable MCP server name field.
    /// 更新可编辑 MCP 服务器名称字段。
    McpServerNameChanged(String),
    /// Selects the editable MCP transport family.
    /// 选择可编辑 MCP 传输类型。
    McpServerTransportSelected(PersistedMcpTransportKind),
    /// Updates the editable MCP server command field.
    /// 更新可编辑 MCP 服务器命令字段。
    McpServerCommandChanged(String),
    /// Updates one editable MCP argument row.
    /// 更新一条可编辑 MCP 参数行。
    McpServerArgRowChanged(usize, String),
    /// Appends one blank MCP argument row.
    /// 追加一条空白 MCP 参数行。
    AddMcpServerArgRow,
    /// Removes one editable MCP argument row while preserving at least one row.
    /// 删除一条可编辑 MCP 参数行并保证至少保留一行。
    RemoveMcpServerArgRow(usize),
    /// Updates the editable MCP server URL field.
    /// 更新可编辑 MCP 服务器 URL 字段。
    McpServerUrlChanged(String),
    /// Updates the editable MCP server bearer token field.
    /// 更新可编辑 MCP 服务器 Bearer Token 字段。
    McpServerBearerTokenChanged(String),
    /// Updates one editable explicit MCP HTTP-header key row.
    /// 更新一条可编辑显式 MCP HTTP 请求头键行。
    McpServerHeaderKeyChanged(usize, String),
    /// Updates one editable explicit MCP HTTP-header value row.
    /// 更新一条可编辑显式 MCP HTTP 请求头值行。
    McpServerHeaderValueChanged(usize, String),
    /// Appends one blank explicit MCP HTTP-header row.
    /// 追加一条空白显式 MCP HTTP 请求头行。
    AddMcpServerHeaderRow,
    /// Removes one editable explicit MCP HTTP-header row while preserving at least one row.
    /// 删除一条可编辑显式 MCP HTTP 请求头行并保证至少保留一行。
    RemoveMcpServerHeaderRow(usize),
    /// Updates one editable environment-backed MCP HTTP-header key row.
    /// 更新一条可编辑环境变量来源 MCP HTTP 请求头键行。
    McpServerHeaderFromEnvKeyChanged(usize, String),
    /// Updates one editable environment-backed MCP HTTP-header environment-name row.
    /// 更新一条可编辑环境变量来源 MCP HTTP 请求头环境变量名行。
    McpServerHeaderFromEnvValueChanged(usize, String),
    /// Appends one blank environment-backed MCP HTTP-header row.
    /// 追加一条空白环境变量来源 MCP HTTP 请求头行。
    AddMcpServerHeaderFromEnvRow,
    /// Removes one editable environment-backed MCP HTTP-header row while preserving at least one row.
    /// 删除一条可编辑环境变量来源 MCP HTTP 请求头行并保证至少保留一行。
    RemoveMcpServerHeaderFromEnvRow(usize),
    /// Updates the editable MCP server OAuth client identifier field.
    /// 更新可编辑 MCP 服务器 OAuth 客户端标识字段。
    McpServerOAuthClientIdChanged(String),
    /// Updates the editable MCP server OAuth client secret field.
    /// 更新可编辑 MCP 服务器 OAuth 客户端密钥字段。
    McpServerOAuthClientSecretChanged(String),
    /// Updates the editable MCP server OAuth scopes field.
    /// 更新可编辑 MCP 服务器 OAuth scopes 字段。
    McpServerOAuthScopesChanged(String),
    /// Updates the editable MCP server OAuth redirect URI field.
    /// 更新可编辑 MCP 服务器 OAuth 回调 URI 字段。
    McpServerOAuthRedirectUriChanged(String),
    /// Updates the editable MCP server working-directory field.
    /// 更新可编辑 MCP 服务器工作目录字段。
    McpServerCwdChanged(String),
    /// Updates one editable explicit MCP environment-variable key row.
    /// 更新一条可编辑显式 MCP 环境变量键行。
    McpServerEnvKeyChanged(usize, String),
    /// Updates one editable explicit MCP environment-variable value row.
    /// 更新一条可编辑显式 MCP 环境变量值行。
    McpServerEnvValueChanged(usize, String),
    /// Appends one blank explicit MCP environment-variable row.
    /// 追加一条空白显式 MCP 环境变量行。
    AddMcpServerEnvRow,
    /// Removes one editable explicit MCP environment-variable row while preserving at least one row.
    /// 删除一条可编辑显式 MCP 环境变量行并保证至少保留一行。
    RemoveMcpServerEnvRow(usize),
    /// Updates one editable passthrough-environment row.
    /// 更新一条可编辑透传环境变量行。
    McpServerEnvPassthroughChanged(usize, String),
    /// Appends one blank passthrough-environment row.
    /// 追加一条空白透传环境变量行。
    AddMcpServerEnvPassthroughRow,
    /// Removes one editable passthrough-environment row while preserving at least one row.
    /// 删除一条可编辑透传环境变量行并保证至少保留一行。
    RemoveMcpServerEnvPassthroughRow(usize),
    /// Updates the editable MCP OAuth callback input field used for manual authorization completion.
    /// 更新用于手动完成授权的 MCP OAuth 回调输入字段。
    McpServerOAuthCallbackInputChanged(String),
    /// Toggles the editable MCP server enabled flag.
    /// 切换可编辑 MCP 服务器启用标记。
    McpServerEnabledToggled(bool),
    /// Creates a new editable MCP server draft.
    /// 创建一份新的可编辑 MCP 服务器草稿。
    CreateMcpServer,
    /// Persists the currently edited MCP server entry.
    /// 持久化当前编辑中的 MCP 服务器条目。
    SaveMcpServerConfiguration,
    /// Requests removal for one configured MCP server entry from the list page.
    /// 从列表页请求移除一条已配置 MCP 服务器条目。
    RequestRemoveMcpServerConfiguration(String),
    /// Starts the host-driven OAuth authorization flow for the currently selected MCP server.
    /// 为当前选中的 MCP 服务器启动宿主驱动的 OAuth 授权流程。
    StartSelectedMcpServerOAuth,
    /// Applies the OAuth challenge payload generated for one selected MCP server.
    /// 应用为某个选中 MCP 服务器生成的 OAuth 挑战载荷。
    SelectedMcpServerOAuthChallengeReady(String, Result<(String, String), String>),
    /// Applies the browser-open attempt result produced for one MCP OAuth flow.
    /// 应用为一条 MCP OAuth 流程执行的浏览器打开结果。
    McpServerOAuthBrowserOpened(Result<(), String>),
    /// Applies one locally captured OAuth callback payload for the selected MCP server.
    /// 应用为当前选中 MCP 服务器捕获到的一份本地 OAuth 回调载荷。
    SelectedMcpServerOAuthCallbackCaptured(String, Result<String, String>),
    /// Finishes the selected MCP server OAuth flow from the current manual callback input.
    /// 使用当前手动回调输入完成所选 MCP 服务器的 OAuth 流程。
    CompleteSelectedMcpServerOAuthFromInput,
    /// Applies the completion result of one selected MCP server OAuth flow.
    /// 应用某个已选 MCP 服务器 OAuth 流程的完成结果。
    SelectedMcpServerOAuthFinished(String, Result<String, String>),
    /// Removes stored OAuth credentials for the currently selected MCP server.
    /// 移除当前选中 MCP 服务器的已存储 OAuth 凭据。
    RemoveSelectedMcpServerOAuthAuthorization,
    /// Applies the removal result of one selected MCP server OAuth credential set.
    /// 应用某个已选 MCP 服务器 OAuth 凭据集合的移除结果。
    SelectedMcpServerOAuthAuthorizationRemoved(String, Result<(), String>),
    /// Applies the latest OAuth status summary loaded for one selected MCP server.
    /// 应用为某个选中 MCP 服务器载入的最新 OAuth 状态摘要。
    SelectedMcpServerOAuthStatusLoaded(String, Result<String, String>),
    /// Applies one freshly loaded LuaSkills catalog snapshot from the embedded host runtime.
    /// 应用一份来自嵌入式宿主运行时的最新 LuaSkills 技能目录快照。
    LuaSkillsCatalogLoaded(Result<LuaSkillsCatalog, String>),
    /// Polls queued LuaSkills lifecycle progress events from the embedded host runtime.
    /// 从嵌入式宿主运行时轮询已排队的 LuaSkills 生命周期进度事件。
    PollLuaSkillsOperationProgressTick,
    /// Requests reloading the visible LuaSkills catalog from the embedded host runtime.
    /// 请求从嵌入式宿主运行时重新载入可见 LuaSkills 技能目录。
    RefreshLuaSkillsCatalog,
    /// Updates the install source choice selected in the LuaSkills panel.
    /// 更新 LuaSkills 面板中当前选中的安装来源类型。
    LuaSkillsInstallSourceChoiceSelected(LuaSkillsInstallSourceChoice),
    /// Updates the optional explicit skill id field shown in the LuaSkills install form.
    /// 更新 LuaSkills 安装表单中显示的可选显式技能标识符字段。
    LuaSkillsInstallSkillIdChanged(String),
    /// Updates the raw source locator field shown in the LuaSkills install form.
    /// 更新 LuaSkills 安装表单中显示的原始来源定位字段。
    LuaSkillsInstallSourceChanged(String),
    /// Requests installing one LuaSkills package into the USER root through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时向 USER 根安装一个 LuaSkills 包。
    InstallLuaSkill,
    /// Requests updating one managed LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时更新一个受管 LuaSkills 包。
    UpdateLuaSkill(String),
    /// Requests enabling one disabled LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时启用一个已停用的 LuaSkills 包。
    EnableLuaSkill(String),
    /// Requests disabling one enabled LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时停用一个已启用的 LuaSkills 包。
    DisableLuaSkill(String),
    /// Requests uninstalling one ordinary LuaSkills package through the embedded host runtime.
    /// 请求通过嵌入式宿主运行时卸载一个普通 LuaSkills 包。
    UninstallLuaSkill(String),
    /// Updates one host-managed LuaSkills runtime tool visibility override without mutating the package lifecycle state itself.
    /// 更新单个宿主管理的 LuaSkills 运行时工具可见性覆盖，而不会改动技能包自身生命周期状态。
    SetLuaSkillToolVisibility {
        skill_id: String,
        local_name: String,
        hidden: bool,
    },
    /// Applies the finished result of one host-side LuaSkills management action.
    /// 应用单次宿主侧 LuaSkills 管理动作的完成结果。
    LuaSkillsManagementFinished(Result<LuaSkillsManagementReport, String>),
    /// Opens the project settings modal for one task-tree project.
    /// 为任务树中的某个项目打开项目设置弹窗。
    OpenTaskProjectSettings(String),
    /// Closes the project settings modal and its transient overlays.
    /// 关闭项目设置弹窗及其临时浮层。
    CloseTaskProjectSettings,
    /// Opens the project attachment cleanup cutoff date picker.
    /// 打开项目附件清理截止日期选择器。
    OpenProjectAttachmentCleanupDatePicker,
    /// Cancels the project attachment cleanup cutoff date picker without changing the date.
    /// 取消项目附件清理截止日期选择器且不修改日期。
    CancelProjectAttachmentCleanupDatePicker,
    /// Applies one date selected by the project attachment cleanup date picker.
    /// 应用项目附件清理日期选择器选中的日期。
    ProjectAttachmentCleanupDateSelected(Date),
    /// Toggles whether one attachment kind should participate in project-scoped cleanup preview and execution.
    /// 切换某种附件类型是否参与项目级清理预览与执行。
    ProjectAttachmentCleanupKindToggled(ConversationAttachmentKind, bool),
    /// Builds one preview for the current project-scoped attachment cleanup selection.
    /// 为当前项目级附件清理选择构建一份预览。
    PreviewProjectAttachmentCleanup,
    /// Applies the finished result of one background project-scoped attachment cleanup preview task.
    /// 应用一条后台项目级附件清理预览任务的完成结果。
    ProjectAttachmentCleanupPreviewFinished(
        ProjectAttachmentCleanupTaskContext,
        Result<AttachmentCleanupPreview, String>,
    ),
    /// Executes one destructive project-scoped attachment cleanup using the current selection.
    /// 使用当前选择执行一次破坏性的项目级附件清理。
    ExecuteProjectAttachmentCleanup,
    /// Applies the finished result of one background project-scoped attachment cleanup execution task.
    /// 应用一条后台项目级附件清理执行任务的完成结果。
    ProjectAttachmentCleanupExecutionFinished(
        ProjectAttachmentCleanupTaskContext,
        Result<ProjectAttachmentCleanupExecutionOutcome, String>,
    ),
    /// Application theme selected from the pick list.
    /// 从下拉选择框中选择了应用主题。
    ThemeSelected(Theme),
    /// Application locale selected from the pick list.
    /// 从下拉选择框中选择了应用语言。
    LocaleSelected(AppLocale),
    /// Startup transcript turn-page size selected from the settings pick list.
    /// 从设置下拉选择框中选中了启动转录轮次分页大小。
    SessionTranscriptTurnPageSizeSelected(usize),
    /// Default command shell policy selected from the settings pick list.
    /// 从设置下拉选择框中选中了默认命令 shell 策略。
    CommandShellPreferenceSelected(CommandShellPickerChoice),
    /// Provider retry enablement toggled in runtime settings.
    /// 在运行时设置中切换了 Provider 重试启用状态。
    ProviderRetryEnabledToggled(bool),
    /// Provider retry maximum retry count changed in runtime settings.
    /// 在运行时设置中调整了 Provider 最大重试次数。
    ProviderRetryMaxRetriesChanged(f32),
    /// Provider retry initial delay changed in runtime settings.
    /// 在运行时设置中调整了 Provider 初始重试等待时间。
    ProviderRetryInitialDelaySecondsChanged(f32),
    /// Provider retry maximum delay changed in runtime settings.
    /// 在运行时设置中调整了 Provider 最大重试等待时间。
    ProviderRetryMaxDelaySecondsChanged(f32),
    /// Provider retry exponential backoff factor changed in runtime settings.
    /// 在运行时设置中调整了 Provider 指数退避倍数。
    ProviderRetryBackoffFactorChanged(f32),
    /// Provider retry jitter percentage changed in runtime settings.
    /// 在运行时设置中调整了 Provider 重试抖动比例。
    ProviderRetryJitterPercentChanged(f32),
    /// Provider retry-after handling toggled in runtime settings.
    /// 在运行时设置中切换了 Provider Retry-After 处理。
    ProviderRetryRespectRetryAfterToggled(bool),
    /// Provider HTTP 429 retry handling toggled in runtime settings.
    /// 在运行时设置中切换了 Provider HTTP 429 重试处理。
    ProviderRetry429Toggled(bool),
    /// Prompt-composer personality selected from the settings pick list.
    /// 从设置下拉选择框中选择了提示词组合人格。
    PromptPersonalitySelected(PromptPersonalityChoice),
    /// Project-level prompt injection toggle changed.
    /// 项目级提示词注入开关发生变化。
    ProjectPromptInjectionToggled(bool),
    /// Project-local prompt override toggle changed in the prompt settings page.
    /// 提示词设置页中的项目本地提示词覆盖开关发生变化。
    ProjectPromptOverrideToggled(bool),
    /// Project-local prompt file path changed in the prompt settings page.
    /// 提示词设置页中的项目本地提示词文件路径发生变化。
    ProjectPromptPathChanged(String),
    /// Reloads the project-local prompt configuration draft from disk.
    /// 从磁盘重新载入项目本地提示词配置草稿。
    ReloadProjectPromptConfiguration,
    /// Saves the project-local prompt configuration draft to disk.
    /// 将项目本地提示词配置草稿保存到磁盘。
    SaveProjectPromptConfiguration,
    /// Applies one read-only text editor action to the inspector guide Markdown buffer.
    /// 向检查面板指引 Markdown 缓冲区应用一次只读文本编辑器动作。
    QuickGuideMarkdownAction(text_editor::Action),
    /// Toggles one prompt-composer inspector layer body.
    /// 切换单个提示词组合检查器分层正文。
    TogglePromptComposerInspectorLayer(PromptComposerInspectorLayer),
    /// Applies one read-only text editor action to one prompt-composer inspector layer buffer.
    /// 向单个提示词组合检查器分层缓冲区应用一次只读文本编辑器动作。
    PromptComposerInspectorLayerAction(PromptComposerInspectorLayer, text_editor::Action),
    /// Toggles one prompt-composer inspector segment body.
    /// 切换单个提示词组合检查器节点正文。
    TogglePromptComposerInspectorSegment(String),
    /// Applies one read-only text editor action to one prompt-composer inspector segment buffer.
    /// 向单个提示词组合检查器节点缓冲区应用一次只读文本编辑器动作。
    PromptComposerInspectorSegmentAction(String, text_editor::Action),
    /// Copies the complete prompt-composer inspector report to the clipboard.
    /// 将完整提示词组合检查器报告复制到剪贴板。
    CopyPromptComposerInspectorReport,
    /// Copies one prompt-composer inspector layer to the clipboard.
    /// 将单个提示词组合检查器分层复制到剪贴板。
    CopyPromptComposerInspectorLayer(PromptComposerInspectorLayer),
    /// Copies one prompt-composer inspector segment to the clipboard.
    /// 将单个提示词组合检查器节点复制到剪贴板。
    CopyPromptComposerInspectorSegment(String),
    /// Applies the finished result of one background About-page license catalog loading task.
    /// 应用一条后台关于页许可证目录加载任务的完成结果。
    AboutLicenseCatalogLoaded(u64, Result<AboutLicenseCatalog, String>),
    /// Opens the preferred application-side license resource directory in the host file manager.
    /// 在宿主文件管理器中打开首选的应用侧许可证资源目录。
    OpenAboutLicensesDirectory,
    /// Opens one concrete About-page license document with the host operating system default application.
    /// 使用宿主操作系统默认应用打开关于页中的某份具体许可证文档。
    OpenAboutLicenseDocumentPath(String),
    /// Toggles one concrete About-page license document body and starts lazy buffer preparation when needed.
    /// 切换关于页中某份具体许可证文档的正文展开状态，并在需要时启动延迟缓冲构建。
    ToggleAboutLicenseDocumentBody(String),
    /// Applies the finished result of one background About-page document-buffer loading task.
    /// 应用一条后台关于页文档缓冲区加载任务的完成结果。
    AboutLicenseDocumentBufferLoaded(u64, String, Result<text_editor::Content, String>),
    /// Applies one read-only text editor action to one About-page license document buffer.
    /// 向关于页中的某个许可证文档缓冲区应用一次只读文本编辑器动作。
    AboutLicenseDocumentRawAction(String, text_editor::Action),
    /// Execution mode selected from the radio group.
    /// 从单选组中选择了执行模式。
    ExecutionModeSelected(ExecutionMode),
    /// Compact reasoning-effort selection changed inside the workbench composer footer row.
    /// 工作台提示词底部控制行中的紧凑推理级别选择发生变化。
    WorkbenchReasoningEffortSelected(ReasoningSelectionOption),
    /// Opens the compact reasoning-effort picker modal from the composer footer trigger.
    /// 从提示词底部触发器打开紧凑思考级别选择弹窗。
    OpenWorkbenchReasoningPicker,
    /// Closes the compact reasoning-effort picker modal.
    /// 关闭紧凑思考级别选择弹窗。
    CloseWorkbenchReasoningPicker,
    /// Records the current viewport of the virtualized workbench model-picker list.
    /// 记录虚拟化工作台模型选择列表的当前视口状态。
    WorkbenchModelPickerScrolled(scrollable::Viewport),
    /// Cycles the current execution mode from the main-input shortcut.
    /// 通过主输入区快捷键循环切换当前执行模式。
    CycleExecutionMode,
    /// Temperature slider changed.
    /// 温度滑块发生变化。
    TemperatureChanged(f32),
    /// Temperature override toggle changed.
    /// 温度覆盖开关被切换。
    TemperatureEnabledToggled(bool),
    /// Plan-first workflow checkbox toggled.
    /// 先计划后执行复选框被切换。
    GeneratePlanToggled(bool),
    /// cargo check checkbox toggled.
    /// cargo check 复选框被切换。
    RunCargoCheckToggled(bool),
    /// Persist memory checkbox toggled.
    /// 持久化记忆复选框被切换。
    PersistMemoryToggled(bool),
    /// Auto-approve toggler changed.
    /// 自动批准开关被切换。
    AutoApproveToggled(bool),
    /// Session selected from the navigator.
    /// 从导航区选择了某个会话。
    SessionSelected(SessionId),
    /// Switches the unified transcript panel into the raw-source selectable-document mode.
    /// 将统一转录面板切换到原始源码可选文档模式。
    ShowSelectedSessionDocumentRawSource,
    /// Switches the unified transcript panel into the last-API-request preview mode.
    /// 将统一转录面板切换到最后一次 API 请求预览模式。
    ShowSelectedSessionDocumentApiRequest,
    /// Reports the captured simulated API request for the selected session request tab.
    /// 回传选中会话请求页签捕获到的模拟 API 请求。
    SessionApiRequestSimulationFinished {
        /// Session identifier that requested the simulated API preview.
        /// 请求模拟 API 预览的会话标识。
        session_id: SessionId,
        /// Captured pretty JSON text or a diagnostic error message.
        /// 捕获到的格式化 JSON 文本，或诊断错误信息。
        result: Result<String, String>,
    },
    /// Switches the transcript panel into the segmented rendered-preview mode.
    /// 将转录面板切换到分段渲染预览模式。
    ShowSelectedSessionDocumentDetailCards,
    /// Opens one inline thinking block in the centered modal surface.
    /// 在居中弹层表面中打开某个内联思考区块。
    OpenSelectedSessionThinkingModal(String),
    /// Toggles one inline rendered agent-view block between collapsed and expanded states.
    /// 在折叠与展开状态之间切换一个内联渲染 agent-view 区块。
    ToggleSelectedSessionAgentViewBlock(String),
    /// Opens one inline tool block in the centered modal surface.
    /// 在居中弹层表面中打开某个内联工具区块。
    OpenSelectedSessionToolModal(String),
    /// Opens one cached subagent task panel inside the main transcript region.
    /// 在主转录区域内部打开一个已缓存的子代理任务面板。
    OpenSelectedSessionSubagentPanel(SessionId),
    /// Updates whether the left-edge subagent navigation panel is expanded by pointer hover.
    /// 更新左侧贴边子代理导航面板是否因鼠标悬停而展开。
    SubagentNavigationHoverChanged(bool),
    /// Stops the currently opened running subagent task without aborting the parent session.
    /// 停止当前已打开的运行中子代理任务，而不影响父会话。
    StopSelectedSubagentTask,
    /// Stops the currently opened running command session from the dedicated tool modal.
    /// 在专属工具弹层中停止当前已打开且仍在运行的命令会话。
    StopSelectedCommandSession,
    /// Reports the completion of one GUI-initiated command-session stop request.
    /// 回传一条由 GUI 发起的命令会话停止请求的完成结果。
    SelectedCommandSessionStopFinished(Result<CommandSessionSnapshot, String>),
    /// Returns from the currently opened subagent task panel back to the parent session transcript.
    /// 从当前已打开的子代理任务面板返回父会话转录视图。
    ReturnToSelectedSessionTranscript,
    /// Starts or completes closing the centered thinking modal.
    /// 启动或完成关闭居中思考弹层。
    CloseSelectedSessionThinkingModal,
    /// Starts or completes closing the centered tool modal.
    /// 启动或完成关闭居中工具弹层。
    CloseSelectedSessionToolModal,
    /// Applies one read-only action to the unified structured transcript buffer of the selected session.
    /// 向当前选中会话的统一结构化转录缓冲区应用一次只读动作。
    SelectedSessionDocumentStructuredAction(text_editor::Action),
    /// Applies one read-only action to the unified raw-source transcript buffer of the selected session.
    /// 向当前选中会话的统一原始源码转录缓冲区应用一次只读动作。
    SelectedSessionDocumentRawSourceAction(text_editor::Action),
    /// Applies one read-only action to the last-API-request preview buffer of the selected session.
    /// 向当前选中会话的最后一次 API 请求预览缓冲区应用一次只读动作。
    SelectedSessionDocumentApiRequestAction(text_editor::Action),
    /// Records the current scroll viewport of the selected-session transcript panel.
    /// 记录当前选中会话转录面板的滚动视口状态。
    SelectedSessionTranscriptScrolled {
        /// Session identifier owning the visible transcript panel.
        /// 拥有当前可见转录面板的会话标识。
        session_id: SessionId,
        /// Latest viewport reported by the visible transcript scrollable.
        /// 当前可见转录滚动容器回传的最新视口。
        viewport: scrollable::Viewport,
    },
    /// Snaps the currently selected session transcript back to the newest bottom tail.
    /// 将当前选中会话的转录重新吸附回最新底部尾端。
    JumpSelectedSessionTranscriptToBottom,
    /// Requests loading one older transcript page for the visible root session.
    /// 请求为当前可见根会话加载一页更早转录历史。
    LoadOlderSessionHistoryRequested(SessionId),
    /// Opens the dedicated workspace-history search modal from the left task drawer trigger.
    /// 通过左侧任务抽屉触发器打开专用工作台历史搜索弹窗。
    OpenWorkspaceHistorySearchPanel,
    /// Closes the dedicated workspace-history search modal without clearing the current query state.
    /// 在不清空当前查询状态的前提下关闭专用工作台历史搜索弹窗。
    CloseWorkspaceHistorySearchPanel,
    /// Updates the current left-panel workspace-history query text.
    /// 更新左侧工作台历史查询文本。
    WorkspaceHistorySearchQueryChanged(String),
    /// Requests running one workspace-scoped BM25 history search.
    /// 请求执行一次工作区级 BM25 历史搜索。
    SubmitWorkspaceHistorySearch,
    /// Requests loading the next page of the current workspace-history search result set.
    /// 请求继续加载当前工作台历史搜索结果的下一页。
    LoadMoreWorkspaceHistorySearchResults,
    /// Applies the completed workspace-scoped BM25 history search result page.
    /// 应用已完成的工作区级 BM25 历史搜索结果页。
    WorkspaceHistorySearchLoaded {
        /// Whether the loaded page should be appended to the current result set.
        /// 本次加载完成的结果页是否应追加到当前结果集中。
        append: bool,
        /// Final page payload or the failure returned by the controller-backed search task.
        /// 控制器承载搜索任务返回的最终结果页或失败信息。
        result: Result<WorkspaceHistorySearchPage, String>,
    },
    /// Toggles the project-scope dropdown used by the left search panel.
    /// 切换左侧搜索面板使用的项目范围下拉区。
    ToggleWorkspaceHistorySearchProjectPicker,
    /// Updates the project keyword filter shown inside the left search panel scope dropdown.
    /// 更新左侧搜索面板范围下拉区中的项目关键字过滤文本。
    WorkspaceHistorySearchProjectFilterChanged(String),
    /// Applies one concrete workspace-history search scope chosen from the left dropdown.
    /// 应用从左侧下拉区选择的一条具体工作台历史搜索范围。
    WorkspaceHistorySearchScopeSelected(WorkspaceHistorySearchScope),
    /// Confirms the currently highlighted project-scope filter selection.
    /// 确认当前项目范围过滤列表中的高亮选择。
    ConfirmWorkspaceHistorySearchScopeSelection,
    /// Opens one workspace-history search hit and navigates the transcript to the matched message.
    /// 打开一条工作台历史搜索命中并将转录定位到匹配消息。
    OpenWorkspaceHistorySearchHit {
        /// Stable session identifier owning the matched message row.
        /// 拥有该命中消息行的稳定会话标识。
        session_id: SessionId,
        /// Stable message identifier that should become visible in the transcript.
        /// 应在转录中变为可见的稳定消息标识。
        message_id: String,
        /// Stable ordinal used to decide whether older history still needs loading.
        /// 用于判断是否仍需补载更早历史的稳定序号。
        ordinal: i64,
        /// Stable turn index used for user-facing status copy.
        /// 用于面向用户状态文案的稳定轮次索引。
        turn_index: i64,
    },
    /// Updates the active drag auto-scroll request emitted by the main rendered transcript surface.
    /// 更新主渲染转录表面发出的当前拖拽自动滚动请求。
    SelectedSessionTranscriptDragAutoScrollChanged {
        /// Session identifier owning the main rendered transcript surface.
        /// 拥有主渲染转录表面的会话标识。
        session_id: SessionId,
        /// Latest drag auto-scroll request, or `None` when the pointer has returned inside the viewport.
        /// 最新拖拽自动滚动请求；当指针回到视口内时为 `None`。
        request: Option<SelectableTranscriptDragAutoScroll>,
    },
    /// Updates the active drag auto-scroll request emitted by the floating thinking overlay body.
    /// 更新流式思考浮层正文发出的当前拖拽自动滚动请求。
    SelectedSessionThinkingOverlayDragAutoScrollChanged {
        /// Session identifier owning the floating thinking overlay.
        /// 拥有流式思考浮层的会话标识。
        session_id: SessionId,
        /// Latest drag auto-scroll request, or `None` when the pointer has returned inside the viewport.
        /// 最新拖拽自动滚动请求；当指针回到视口内时为 `None`。
        request: Option<SelectableTranscriptDragAutoScroll>,
    },
    /// Drives one drag auto-scroll tick for the main rendered transcript surface.
    /// 驱动主渲染转录表面的一次拖拽自动滚动 Tick。
    SelectedSessionTranscriptDragAutoScrollTick,
    /// Drives one coalesced tail-follow tick for the main rendered transcript surface.
    /// 驱动主渲染转录表面的一次合并尾部追随 Tick。
    SelectedSessionTranscriptFollowTailTick,
    /// Records the current scroll viewport of the floating streaming-thinking overlay.
    /// 记录流式思考浮层的当前滚动视口状态。
    SelectedSessionThinkingOverlayScrolled {
        /// Session identifier owning the visible floating thinking overlay.
        /// 拥有当前可见流式思考浮层的会话标识。
        session_id: SessionId,
        /// Latest viewport reported by the floating thinking scrollable.
        /// 当前流式思考滚动容器回传的最新视口。
        viewport: scrollable::Viewport,
    },
    /// Drives one drag auto-scroll tick for the floating thinking overlay body.
    /// 驱动流式思考浮层正文的一次拖拽自动滚动 Tick。
    SelectedSessionThinkingOverlayDragAutoScrollTick,
    /// Drives one coalesced tail-follow tick for the floating thinking overlay body.
    /// 驱动流式思考浮层正文的一次合并尾部追随 Tick。
    SelectedSessionThinkingOverlayFollowTailTick,
    /// Drives one centered thinking-modal animation tick while it is opening or closing.
    /// 在居中思考弹层打开或关闭期间驱动一次动画 Tick。
    ThinkingModalAnimationTick,
    /// Drives one lightweight animation tick shared by request/thinking progress hints.
    /// 驱动“正在请求/正在思考”等轻量进度提示共用的动画 Tick。
    SessionRuntimeProgressTick,
    /// Applies one read-only text editor action to one inspector activity-log buffer.
    /// 向某个检查面板活动日志缓冲区应用一次只读文本编辑器动作。
    ActivityLogRawAction(usize, text_editor::Action),
    /// Create-session action requested.
    /// 请求创建新会话。
    CreateSession,
    /// Updates the hovered project node inside the main task-drawer tree.
    /// 更新主任务抽屉树中当前悬停的项目节点。
    TaskProjectHovered(Option<String>),
    /// Updates the drag-drop preview target inside one project row using local pointer movement.
    /// 使用项目行内部的局部鼠标移动更新拖拽插入目标预览。
    TaskProjectDropPreviewMoved(String, iced::Point),
    /// Updates the drop-target preview near the tail of one project section while dragging.
    /// 在拖拽期间更新某个项目分区尾部附近的插入目标预览。
    TaskProjectSectionTailHovered(Option<bool>),
    /// Updates the hovered session row inside the main task-drawer tree.
    /// 更新主任务抽屉树中当前悬停的会话行。
    TaskSessionHovered(Option<SessionId>),
    /// Starts one click-or-drag interaction from a project row inside the main task-drawer tree.
    /// 从主任务抽屉树中的某个项目行启动一次点击或拖拽交互。
    StartTaskProjectInteraction(String),
    /// Opens one project directory with the host operating system.
    /// 使用宿主操作系统打开某个项目目录。
    OpenTaskProjectDirectory(String),
    /// Creates a new session under one project node inside the main task-drawer tree.
    /// 在主任务抽屉树中的某个项目节点下创建新会话。
    CreateTaskProjectSession(String),
    /// Adds one new project node by picking a folder from the host file system.
    /// 通过从宿主文件系统选择目录来新增一个项目节点。
    AddTaskProjectFromFolder,
    /// Pins or unpins one project node inside the main task-drawer tree.
    /// 置顶或取消置顶主任务抽屉树中的某个项目节点。
    PinTaskProject(String),
    /// Renames one project node inside the main task-drawer tree using a lightweight placeholder flow.
    /// 使用轻量占位流程重命名主任务抽屉树中的某个项目节点。
    RenameTaskProject(String),
    /// Archives one project node inside the main task-drawer tree.
    /// 归档主任务抽屉树中的某个项目节点。
    ArchiveTaskProject(String),
    /// Restores one archived project node back into the visible main task-drawer tree.
    /// 将一个已归档项目节点恢复回可见的主任务抽屉树。
    RestoreTaskProject(String),
    /// Renames one session row inside the main task-drawer tree using a lightweight placeholder flow.
    /// 使用轻量占位流程重命名主任务抽屉树中的某个会话行。
    RenameTaskSession(SessionId),
    /// Archives one session row inside the main task-drawer tree.
    /// 归档主任务抽屉树中的某个会话行。
    ArchiveTaskSession(SessionId),
    /// Restores one archived session row back into its visible owning project.
    /// 将一个已归档会话行恢复回其可见的所属项目。
    RestoreTaskSession(SessionId),
    /// Requests the first-step destructive confirmation for one archived project.
    /// 请求针对某个归档项目的第一步破坏性确认。
    RequestPermanentDeleteArchivedTaskProject(String),
    /// Requests the first-step destructive confirmation for one archived session.
    /// 请求针对某个归档会话的第一步破坏性确认。
    RequestPermanentDeleteArchivedTaskSession(SessionId),
    /// Cancels the currently visible archived-item destructive confirmation flow.
    /// 取消当前可见的归档项破坏性确认流程。
    CancelPermanentDeleteArchivedItem,
    /// Confirms and starts the permanent deletion of the currently selected archived item.
    /// 确认并启动当前选中归档项的永久删除。
    ConfirmPermanentDeleteArchivedItem,
    /// Applies the final result returned by one archived-item permanent deletion task.
    /// 应用单次归档项永久删除任务返回的最终结果。
    PermanentDeleteArchivedItemFinished(Result<ArchivedDeleteOutcome, String>),
    /// Workspace scan action requested.
    /// 请求扫描工作区。
    ScanWorkspace,
    /// Toggles one file section in the diff panel.
    /// 切换 diff 面板中单个文件区块的折叠状态。
    ToggleDiffFile(String),
    /// Applies a read-only text editor action to one diff file editor.
    /// 向某个 diff 文件编辑器应用只读文本编辑器动作。
    DiffCodeAction(String, text_editor::Action),
    /// Records the current viewport of the structured diff review flow.
    /// 记录结构化 diff 审查流的当前视口状态。
    DiffPanelScrolled(scrollable::Viewport),
    /// Toggles one diff file card and then scrolls back to its header position.
    /// 切换某个 diff 文件卡片后回滚到它的头部位置。
    ToggleAndFocusDiffFile(String),
    /// Updates the currently hovered visual line inside one diff hunk body.
    /// 更新某个 diff hunk 主体内当前悬停的可视行。
    DiffCodeHoverMoved(String, usize),
    /// Clears the hovered visual line when the cursor leaves one diff hunk body.
    /// 当光标离开某个 diff hunk 主体时清除悬停可视行。
    DiffCodeHoverExited(String),
    /// Primary execution action requested.
    /// 请求执行主任务动作。
    StartRun,
    /// Dispatches one existing queued prompt immediately, interrupting the current run first when needed.
    /// 立即分发一条现有排队提示词；若当前仍在运行则先中断当前执行。
    PendingPromptQueueInsertNow(String),
    /// Restores one queued prompt back into the composer editor for further editing.
    /// 将某条已排队提示词恢复回提示词编辑器，以便继续修改。
    PendingPromptQueueEdit(String),
    /// Removes one queued prompt from the selected session queue without dispatching it.
    /// 从当前选中会话队列中移除某条已排队提示词且不再发送。
    PendingPromptQueueRemove(String),
    /// Opens the blocking editor modal for the final sent user-prompt message.
    /// 打开最后一条已发送用户提示词的阻塞式编辑弹窗。
    OpenLastUserMessageEdit(String),
    /// Applies one text-editor action inside the final-user-message edit modal.
    /// 在最后用户消息编辑弹窗内应用一次文本编辑器动作。
    LastUserMessageEditAction(text_editor::Action),
    /// Updates the edit-modal scrollbar drag preview from a local pointer y position.
    /// 使用局部指针纵向位置更新编辑弹窗滚动条拖拽预览。
    LastUserMessageEditScrollbarPointerMoved(f32),
    /// Starts one drag interaction on the edit-modal scrollbar.
    /// 在编辑弹窗滚动条上启动一次拖拽交互。
    StartLastUserMessageEditScrollbarDrag,
    /// Closes the final-user-message edit modal without mutating conversation history.
    /// 关闭最后用户消息编辑弹窗且不修改会话历史。
    CancelLastUserMessageEdit,
    /// Confirms the edited final user message, then rolls back and re-dispatches that turn.
    /// 确认编辑后的最后用户消息，然后回滚并重新派发该轮输入。
    SubmitLastUserMessageEdit,
    /// Starts one click-or-drag interaction from a pending-prompt queue handle.
    /// 从某条待发送队列拖拽柄启动一次点击或拖拽交互。
    StartPendingPromptQueueDrag(String),
    /// Updates the drop-target preview inside one pending-prompt queue row using local pointer movement.
    /// 使用队列行内部的局部鼠标移动更新待发送队列的插入目标预览。
    PendingPromptQueueDropPreviewMoved(String, iced::Point),
    /// Updates whether the tail drop zone of the pending-prompt queue is currently hovered.
    /// 更新待发送队列尾部落点当前是否处于悬停状态。
    PendingPromptQueueTailHovered(bool),
    /// Applies one incremental execution-stream event returned by the async runtime stream.
    /// 应用异步运行时流返回的一条增量执行事件。
    RunExecutionStreamEvent {
        /// Monotonic execution-stream identifier used to discard stale events after stop or restart.
        /// 用于在停止或重启后丢弃过期事件的单调执行流标识。
        run_id: u64,
        /// Concrete incremental event emitted by the async runtime stream.
        /// 由异步运行时流发出的具体增量事件。
        event: ExecutionStreamEvent,
    },
    /// Applies one streamed event returned by one internal prompt-based session-compaction task.
    /// 应用单次内部提示词式会话压缩任务返回的一条流式事件。
    SessionPromptCompactionStreamEvent {
        /// Monotonic compaction-task identifier used to discard stale events after stop or restart.
        /// 用于在停止或重启后丢弃过期事件的单调压缩任务标识。
        run_id: u64,
        /// Session identifier that owns the compaction task.
        /// 拥有该压缩任务的会话标识。
        session_id: SessionId,
        /// Trigger source that started the compaction task.
        /// 启动该压缩任务的触发来源。
        trigger: SessionPromptCompactionTrigger,
        /// Human-readable budget note captured when the compaction task was scheduled.
        /// 在调度压缩任务时捕获的人类可读预算说明。
        budget_note: String,
        /// Planned compaction window used to build a fallback result when the model summary is unavailable.
        /// 当模型总结不可用时用于构建回退结果的已规划压缩窗口。
        plan: CompactionPlan,
        /// Incremental compaction event returned by the internal summary task.
        /// 由内部总结任务返回的增量压缩事件。
        event: PromptCompactionStreamEvent,
    },
    /// Applies one detached background subagent event emitted after the parent run has already resumed or finished.
    /// 应用一条在父运行恢复或结束之后发出的分离后台子代理事件。
    DetachedSubagentTaskEvent {
        /// Stable detached child-task identifier that owns the listener stream.
        /// 拥有该监听流的稳定分离子任务标识。
        task_id: SessionId,
        /// Concrete detached background event published by the task runtime.
        /// 由任务运行时发布的具体分离后台事件。
        event: DetachedTaskRuntimeEvent,
    },
    /// Stop action requested.
    /// 请求停止当前动作。
    StopRun,
    /// Pane received focus through click.
    /// 面板通过点击获得焦点。
    PaneFocused(pane_grid::Pane),
    /// Pane grid split was resized.
    /// 分栏网格的分割线被调整。
    PaneResized(pane_grid::ResizeEvent),
}
