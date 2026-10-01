// App-level commands reachable from the palette, slash input and shortcuts.
// 可从命令面板、斜杠输入与快捷键触发的应用级命令。

use crate::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    NewConversation,
    TogglePlan,
    SelectModel,
    CycleReasoning,
    ToggleTheme,
    Language,
    ToggleSidebar,
    TogglePanel,
    ThinkingPlacement,
    OpenDiff,
    Commit,
    SwitchBranch,
    Push,
    OpenWeb,
    Help,
    Quit,
    Chat,
    Settings,
    Memory,
    Knowledge,
    AgentsArea,
    Skills,
    Plugins,
    ContextIndex,
    AreaPicker,
    History,
    Attach,
    AddProject,
    /// Opens the command palette; not listed inside the palette itself.
    /// 打开命令面板；不在命令面板自身中列出。
    Palette,
}

impl AppCommand {
    pub const ALL: [AppCommand; 28] = [
        AppCommand::NewConversation,
        AppCommand::AddProject,
        AppCommand::Attach,
        AppCommand::History,
        AppCommand::TogglePlan,
        AppCommand::SelectModel,
        AppCommand::CycleReasoning,
        AppCommand::OpenDiff,
        AppCommand::Commit,
        AppCommand::SwitchBranch,
        AppCommand::Push,
        AppCommand::ToggleSidebar,
        AppCommand::TogglePanel,
        AppCommand::ThinkingPlacement,
        AppCommand::ToggleTheme,
        AppCommand::Language,
        AppCommand::AreaPicker,
        AppCommand::Chat,
        AppCommand::Settings,
        AppCommand::Memory,
        AppCommand::Knowledge,
        AppCommand::AgentsArea,
        AppCommand::Skills,
        AppCommand::Plugins,
        AppCommand::ContextIndex,
        AppCommand::OpenWeb,
        AppCommand::Help,
        AppCommand::Quit,
    ];

    /// Slash name typed in the composer, e.g. `/plan`.
    /// 在输入框中输入的斜杠名称，例如 `/plan`。
    pub fn slash(self) -> &'static str {
        match self {
            AppCommand::NewConversation => "new",
            AppCommand::TogglePlan => "plan",
            AppCommand::SelectModel => "model",
            AppCommand::CycleReasoning => "effort",
            AppCommand::ToggleTheme => "theme",
            AppCommand::Language => "language",
            AppCommand::ToggleSidebar => "sidebar",
            AppCommand::TogglePanel => "panel",
            AppCommand::ThinkingPlacement => "thinking",
            AppCommand::OpenDiff => "diff",
            AppCommand::Commit => "commit",
            AppCommand::SwitchBranch => "branch",
            AppCommand::Push => "push",
            AppCommand::OpenWeb => "web",
            AppCommand::Help => "help",
            AppCommand::Quit => "quit",
            AppCommand::Chat => "chat",
            AppCommand::Settings => "settings",
            AppCommand::Memory => "memory",
            AppCommand::Knowledge => "knowledge",
            AppCommand::AgentsArea => "agents",
            AppCommand::Skills => "skills",
            AppCommand::Plugins => "plugins",
            AppCommand::ContextIndex => "index",
            AppCommand::AreaPicker => "areas",
            AppCommand::History => "history",
            AppCommand::Attach => "attach",
            AppCommand::AddProject => "project",
            AppCommand::Palette => "palette",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            AppCommand::NewConversation => "Ctrl+N",
            AppCommand::TogglePlan => "Shift+Tab",
            AppCommand::SelectModel => "Ctrl+O",
            AppCommand::CycleReasoning => "Ctrl+R",
            AppCommand::ToggleSidebar => "Ctrl+B",
            AppCommand::TogglePanel => "Ctrl+\\",
            AppCommand::ThinkingPlacement => "Ctrl+T",
            AppCommand::OpenDiff => "Alt+D",
            AppCommand::Commit => "Alt+C",
            AppCommand::SwitchBranch => "Alt+B",
            AppCommand::Help => "F1",
            AppCommand::Quit => "Ctrl+Q",
            AppCommand::AreaPicker => "F2",
            AppCommand::History => "Ctrl+F",
            AppCommand::Attach => "Alt+A",
            AppCommand::Palette => "Ctrl+P",
            _ => "",
        }
    }

    pub fn label(self) -> String {
        match self {
            AppCommand::NewConversation => tr!("projectMenu.newConversation"),
            AppCommand::TogglePlan => tr!("composer.planMode"),
            AppCommand::SelectModel => tr!("composer.modelPicker.tooltipTitle"),
            AppCommand::CycleReasoning => tr!("composer.reasoning.subtitle"),
            AppCommand::ToggleTheme => tr!("appChrome.theme"),
            AppCommand::Language => tr!("global.language"),
            AppCommand::ToggleSidebar => tr!("sidebar.toggle"),
            AppCommand::TogglePanel => tr!("tui.command.togglePanel"),
            AppCommand::ThinkingPlacement => tr!("composer.thinkingProcess"),
            AppCommand::OpenDiff => tr!("workspaceContext.launcher.reviewDescription"),
            AppCommand::Commit => tr!("gitCommitDialog.open"),
            AppCommand::SwitchBranch => tr!("gitBranchMenu.label"),
            AppCommand::Push => tr!("gitCommitDialog.push"),
            AppCommand::OpenWeb => tr!("tui.command.openWeb"),
            AppCommand::Help => tr!("tui.help.title"),
            AppCommand::Quit => tr!("tui.command.quit"),
            AppCommand::Chat => tr!("area.chat"),
            AppCommand::Settings => tr!("global.openSettings"),
            AppCommand::Memory => tr!("area.memory"),
            AppCommand::Knowledge => tr!("area.knowledge"),
            AppCommand::AgentsArea => tr!("area.agents"),
            AppCommand::Skills => tr!("area.agentSkills"),
            AppCommand::Plugins => tr!("area.skills"),
            AppCommand::ContextIndex => tr!("codeContext.project.projectSemanticIndex"),
            AppCommand::AreaPicker => tr!("tui.area.switch"),
            AppCommand::History => tr!("tui.history.title"),
            AppCommand::Attach => tr!("tui.attach.title"),
            AppCommand::AddProject => tr!("sidebar.addProject"),
            AppCommand::Palette => tr!("tui.palette.title"),
        }
    }

    pub fn from_slash(name: &str) -> Option<AppCommand> {
        AppCommand::ALL.into_iter().find(|c| c.slash() == name)
    }
}
