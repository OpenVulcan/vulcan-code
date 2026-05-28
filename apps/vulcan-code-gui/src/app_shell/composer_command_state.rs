//! Composer slash-command state extracted from the main workbench module.
//! 从主工作台模块中抽取的输入框 slash 指令状态。

/// Supported slash commands that may be triggered directly from the main AI Coding composer.
/// 可直接从主 AI Coding 输入框触发的受支持 slash 指令集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ComposerSlashCommand {
    /// Opens the existing workbench model picker.
    /// 打开现有工作台模型选择器。
    Model,
    /// Immediately compacts the current session context.
    /// 立即压缩当前会话上下文。
    Compact,
    /// Opens the existing workbench reasoning-effort picker.
    /// 打开现有工作台思考级别选择器。
    Reasoning,
    /// Inserts a prompt-skill loading hint into the composer.
    /// 向输入框插入提示词技能加载提示。
    Skill,
    /// Opens the Prompt Skills management panel.
    /// 打开提示词技能管理面板。
    Skills,
}

impl ComposerSlashCommand {
    /// Returns every supported slash command in stable display order.
    /// 返回按稳定显示顺序排列的全部受支持 slash 指令。
    pub(super) const ALL: [Self; 5] = [
        Self::Model,
        Self::Compact,
        Self::Reasoning,
        Self::Skill,
        Self::Skills,
    ];

    /// Returns the canonical slash trigger text used by exact activation checks and list rendering.
    /// 返回供精确激活检查与列表渲染使用的规范 slash 触发文本。
    pub(super) const fn trigger(self) -> &'static str {
        match self {
            Self::Model => "/model",
            Self::Compact => "/compact",
            Self::Reasoning => "/reasoning",
            Self::Skill => "/skill",
            Self::Skills => "/skills",
        }
    }

    /// Returns the keyword portion after the leading slash for filter matching.
    /// 返回用于过滤匹配的去掉前导斜杠后的关键字部分。
    pub(super) const fn keyword(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Compact => "compact",
            Self::Reasoning => "reasoning",
            Self::Skill => "skill",
            Self::Skills => "skills",
        }
    }

    /// Returns the concise human-readable action summary shown in the inline suggestion panel.
    /// 返回展示在内联建议面板中的简短人类可读动作摘要。
    pub(super) const fn summary(self) -> &'static str {
        match self {
            Self::Model => "选择当前模型",
            Self::Compact => "立即压缩当前会话上下文",
            Self::Reasoning => "选择当前思考级别",
            Self::Skill => "插入提示词技能加载指令",
            Self::Skills => "打开提示词技能面板",
        }
    }

    /// Returns whether the command should stay visible for the provided slash filter prefix.
    /// 返回在给定 slash 过滤前缀下该指令是否应继续可见。
    pub(super) fn matches_filter(self, filter_text: &str) -> bool {
        self.keyword().starts_with(filter_text)
    }

    /// Resolves one command from an exact composer input string when direct activation is allowed.
    /// 当允许直接激活时，根据输入框中的精确文本解析对应指令。
    pub(super) fn from_exact_input(input: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|command| command.trigger().eq_ignore_ascii_case(input))
    }
}

/// Runtime-only slash-command panel state derived from the current composer text.
/// 根据当前输入框文本推导出的仅运行时存在的 slash 指令面板状态。
#[derive(Debug, Clone)]
pub(super) struct ComposerSlashCommandState {
    /// Current raw composer text that triggered the slash-command suggestion flow.
    /// 触发 slash 指令建议流的当前原始输入框文本。
    pub(super) raw_input: String,
    /// Current normalized filter text after stripping the leading slash.
    /// 去掉前导斜杠后得到的当前规范化过滤文本。
    pub(super) filter_text: String,
    /// Current visible command list after applying the filter.
    /// 应用过滤条件后当前可见的命令列表。
    pub(super) visible_commands: Vec<ComposerSlashCommand>,
    /// Currently highlighted visible command index used by keyboard navigation.
    /// 供键盘导航使用的当前高亮可见命令索引。
    pub(super) highlighted_index: Option<usize>,
}

impl ComposerSlashCommandState {
    /// Returns the currently highlighted command when the inline panel is visible.
    /// 返回内联面板可见时当前高亮的命令。
    pub(super) fn highlighted_command(&self) -> Option<ComposerSlashCommand> {
        let highlighted_index = self.highlighted_index?;
        self.visible_commands.get(highlighted_index).copied()
    }
}

/// Builds one runtime slash-command panel state from the latest composer text when the text still represents a supported slash query.
/// 当文本仍表示受支持的 slash 查询时，根据最新输入框文本构建运行时 slash 指令面板状态。
pub(super) fn build_composer_slash_command_state(
    composer_text: &str,
) -> Option<ComposerSlashCommandState> {
    if !composer_text.starts_with('/') {
        return None;
    }

    let filter_text = composer_text
        .strip_prefix('/')
        .unwrap_or_default()
        .to_ascii_lowercase();
    let visible_commands = ComposerSlashCommand::ALL
        .into_iter()
        .filter(|command| filter_text.is_empty() || command.matches_filter(filter_text.as_str()))
        .collect::<Vec<_>>();
    if visible_commands.is_empty() {
        return None;
    }

    Some(ComposerSlashCommandState {
        raw_input: composer_text.to_string(),
        filter_text,
        visible_commands,
        highlighted_index: Some(0),
    })
}
