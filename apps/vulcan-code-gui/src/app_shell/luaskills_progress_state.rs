//! LuaSkills progress state extracted from the main workbench module.
//! 从主工作台模块中抽取的 LuaSkills 进度状态。

use luaskills::{RuntimeSkillOperationProgressEvent, SkillInstallSourceType};

/// Visual lifecycle kind used by the LuaSkills management panel progress surface.
/// LuaSkills 管理面板进度表面使用的可视化生命周期类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LuaSkillsOperationVisualKind {
    /// Catalog refresh initiated from the panel header.
    /// 从面板标题栏发起的目录刷新。
    Refresh,
    /// Managed skill install initiated from the install form.
    /// 从安装表单发起的受管技能安装。
    Install,
    /// Managed skill update initiated from one visible skill row.
    /// 从某条可见技能行发起的受管技能更新。
    Update,
    /// Enable action initiated from one visible skill row.
    /// 从某条可见技能行发起的启用操作。
    Enable,
    /// Disable action initiated from one visible skill row.
    /// 从某条可见技能行发起的停用操作。
    Disable,
    /// Uninstall action initiated from one visible skill row.
    /// 从某条可见技能行发起的卸载操作。
    Uninstall,
}

impl LuaSkillsOperationVisualKind {
    /// Returns the user-facing verb label of the current operation kind.
    /// 返回当前操作类型的面向用户动词标签。
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Refresh => "刷新",
            Self::Install => "安装",
            Self::Update => "更新",
            Self::Enable => "启用",
            Self::Disable => "停用",
            Self::Uninstall => "卸载",
        }
    }

    /// Returns the in-progress label rendered by compact badges while the operation is still running.
    /// 返回操作仍在进行时供紧凑徽标渲染的进行中标签。
    pub(super) fn active_label(self) -> &'static str {
        match self {
            Self::Refresh => "刷新中",
            Self::Install => "安装中",
            Self::Update => "更新中",
            Self::Enable => "启用中",
            Self::Disable => "停用中",
            Self::Uninstall => "卸载中",
        }
    }
}

/// Latest LuaSkills lifecycle progress snapshot mirrored from the runtime callback queue.
/// 从运行时回调队列镜像得到的最新 LuaSkills 生命周期进度快照。
#[derive(Debug, Clone)]
pub(super) struct LuaSkillsOperationProgressState {
    /// Stable operation identifier shared by all runtime events from the current lifecycle action.
    /// 当前生命周期动作全部运行时事件共享的稳定操作标识符。
    pub(super) operation_id: String,
    /// High-level GUI operation kind currently represented by the snapshot.
    /// 当前快照所表示的高层图形界面操作类型。
    pub(super) action_kind: LuaSkillsOperationVisualKind,
    /// Optional stable target skill identifier.
    /// 可选的稳定目标技能标识符。
    pub(super) skill_id: Option<String>,
    /// Optional named runtime root targeted by the current operation.
    /// 当前操作目标的可选命名运行时根。
    pub(super) root_name: Option<String>,
    /// Machine-readable progress phase mirrored from the runtime callback.
    /// 从运行时回调镜像而来的机器可读进度阶段。
    pub(super) phase: String,
    /// Machine-readable progress status mirrored from the runtime callback.
    /// 从运行时回调镜像而来的机器可读进度状态。
    pub(super) status: String,
    /// Optional determinate completion percentage for the current phase.
    /// 当前阶段的可选确定性完成百分比。
    pub(super) percent: Option<f64>,
    /// Optional completed byte count for download-oriented phases.
    /// 面向下载阶段的可选已完成字节数。
    pub(super) bytes_done: Option<u64>,
    /// Optional total byte count for download-oriented phases.
    /// 面向下载阶段的可选总字节数。
    pub(super) bytes_total: Option<u64>,
    /// Optional managed source type involved in the current phase.
    /// 当前阶段涉及的可选受管来源类型。
    pub(super) source_type: Option<SkillInstallSourceType>,
    /// Optional managed source locator involved in the current phase.
    /// 当前阶段涉及的可选受管来源定位值。
    pub(super) source_locator: Option<String>,
    /// Optional human-readable runtime message attached to the current phase.
    /// 附着在当前阶段上的可选人类可读运行时消息。
    pub(super) message: Option<String>,
}

impl LuaSkillsOperationProgressState {
    /// Builds one synthetic queued snapshot before the runtime emits finer-grained progress events.
    /// 在运行时发出更细粒度进度事件之前构造一条合成排队快照。
    pub(super) fn queued(
        action_kind: LuaSkillsOperationVisualKind,
        skill_id: Option<String>,
        root_name: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        let stable_skill_id = skill_id.as_deref().unwrap_or("catalog");
        Self {
            operation_id: format!("gui-{}-{stable_skill_id}", action_kind.label()),
            action_kind,
            skill_id,
            root_name,
            phase: String::from("queued"),
            status: String::from("started"),
            percent: None,
            bytes_done: None,
            bytes_total: None,
            source_type: None,
            source_locator: None,
            message: Some(message.into()),
        }
    }

    /// Mirrors one runtime progress event onto the current GUI snapshot.
    /// 将一条运行时进度事件镜像到当前图形界面快照。
    pub(super) fn apply_runtime_event(&mut self, event: &RuntimeSkillOperationProgressEvent) {
        self.operation_id = event.operation_id.clone();
        self.skill_id = event.skill_id.clone().or_else(|| self.skill_id.clone());
        self.root_name = event.root_name.clone().or_else(|| self.root_name.clone());
        self.phase = event.phase.clone();
        self.status = event.status.clone();
        self.percent = event.percent;
        self.bytes_done = event.bytes_done;
        self.bytes_total = event.bytes_total;
        self.source_type = event.source_type;
        self.source_locator = event.source_locator.clone();
        self.message = event.message.clone();
    }

    /// Returns whether the current snapshot refers to the provided visible skill row.
    /// 返回当前快照是否指向给定的可见技能行。
    pub(super) fn matches_skill(&self, skill_id: &str) -> bool {
        self.skill_id.as_deref() == Some(skill_id)
    }

    /// Returns the optional normalized progress-bar value expected by iced.
    /// 返回 iced 进度条期望的可选归一化进度值。
    pub(super) fn progress_fraction(&self) -> Option<f32> {
        self.percent
            .map(|percent| (percent.clamp(0.0, 100.0) / 100.0) as f32)
    }

    /// Returns the primary user-facing heading rendered by the panel progress card.
    /// 返回渲染在面板进度卡片中的主要面向用户标题。
    pub(super) fn title_text(&self) -> String {
        match self.skill_id.as_deref() {
            Some(skill_id) => format!("{}技能 {skill_id}", self.action_kind.label()),
            None => format!("{} LuaSkills 目录", self.action_kind.label()),
        }
    }

    /// Returns the compact badge label rendered inside visible skill rows.
    /// 返回渲染在可见技能行内部的紧凑徽标标签。
    pub(super) fn badge_text(&self) -> String {
        if let Some(percent) = self.percent {
            format!("{} · {:.0}%", self.action_kind.active_label(), percent)
        } else {
            format!(
                "{} · {}",
                self.action_kind.active_label(),
                self.phase_label()
            )
        }
    }

    /// Returns the normalized human-readable phase label for the current snapshot.
    /// 返回当前快照对应的规范化人类可读阶段标签。
    pub(super) fn phase_label(&self) -> String {
        format_luaskills_progress_phase_label(&self.phase)
    }

    /// Returns the compact status line rendered above the optional detail rows.
    /// 返回渲染在可选详情行上方的紧凑状态文本。
    pub(super) fn status_line(&self) -> String {
        let mut parts = vec![self.phase_label()];
        if let Some(percent) = self.percent {
            parts.push(format!("{percent:.0}%"));
        } else if self.status == "cached" {
            parts.push(String::from("命中缓存"));
        } else if self.status != "started" {
            parts.push(format_luaskills_progress_status_label(&self.status).to_string());
        }
        if let (Some(done), Some(total)) = (self.bytes_done, self.bytes_total) {
            parts.push(format!(
                "{} / {}",
                format_luaskills_progress_bytes(done),
                format_luaskills_progress_bytes(total)
            ));
        }
        parts.join(" · ")
    }

    /// Returns the optional managed source summary rendered in the progress detail area.
    /// 返回渲染在进度详情区域中的可选受管来源摘要。
    pub(super) fn source_line(&self) -> Option<String> {
        let source_locator = self.source_locator.as_ref()?;
        let source_label = match self.source_type {
            Some(SkillInstallSourceType::Github) => "GitHub",
            Some(SkillInstallSourceType::Url) => "包地址",
            Some(SkillInstallSourceType::OfficialHub) => "Official Hub",
            Some(SkillInstallSourceType::PrivateUrlManifest) => "Private URL Manifest",
            None => "来源",
        };
        Some(format!("{source_label} · {source_locator}"))
    }

    /// Returns the single-line panel notice mirrored into the header message area.
    /// 返回镜像到标题消息区域中的单行面板提示文本。
    pub(super) fn notice_text(&self) -> String {
        let mut summary = format!("{} · {}", self.title_text(), self.status_line());
        if let Some(message) = self
            .message
            .as_ref()
            .map(|message| message.trim())
            .filter(|message| !message.is_empty())
        {
            summary.push_str(" · ");
            summary.push_str(message);
        }
        summary
    }
}

/// Formats one machine-readable LuaSkills progress phase into a user-facing Chinese label.
/// 将一条机器可读 LuaSkills 进度阶段格式化为面向用户的中文标签。
fn format_luaskills_progress_phase_label(phase: &str) -> String {
    match phase {
        "queued" => String::from("已排队"),
        "validating_request" => String::from("校验请求"),
        "resolving_source" => String::from("解析来源"),
        "resolving_manifest" => String::from("解析清单"),
        "downloading_archive" => String::from("下载归档"),
        "extracting_archive" => String::from("解压归档"),
        "validating_package" => String::from("校验包内容"),
        "staging_install" => String::from("写入安装目录"),
        "staging_update" => String::from("替换技能目录"),
        "reloading_runtime" => String::from("重载运行时"),
        "committing" => String::from("提交变更"),
        "completed" => String::from("已完成"),
        "failed" => String::from("失败"),
        other => other.replace('_', " "),
    }
}

/// Formats one machine-readable LuaSkills progress status into a user-facing Chinese label.
/// 将一条机器可读 LuaSkills 进度状态格式化为面向用户的中文标签。
fn format_luaskills_progress_status_label(status: &str) -> &'static str {
    match status {
        "started" => "已开始",
        "progress" => "进行中",
        "completed" => "已完成",
        "cached" => "命中缓存",
        "failed" => "失败",
        "blocked" => "已阻止",
        _ => "进行中",
    }
}

/// Formats one raw byte count into a compact user-facing progress label.
/// 将一段原始字节数格式化为紧凑的面向用户进度标签。
fn format_luaskills_progress_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}
