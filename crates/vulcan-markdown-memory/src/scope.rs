//! The three memory scopes and their preset sections.
//! 三个记忆层及其预置分区。

use serde::{Deserialize, Serialize};

/// One memory scope.
/// 一个记忆层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    /// Who the user is; always injected in full.
    /// 用户是谁；始终全文注入。
    User,
    /// Rules and environment that hold for every project.
    /// 对所有项目都成立的规则与环境。
    Space,
    /// Facts, decisions and constraints of one project.
    /// 单个项目的事实、决策与约束。
    Project,
}

impl MemoryScope {
    /// Every scope in display order.
    /// 按显示顺序排列的全部层。
    pub const ALL: [Self; 3] = [Self::User, Self::Space, Self::Project];

    /// Returns the stable lowercase key used in tool arguments and output.
    /// 返回工具参数与输出中使用的稳定小写键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Space => "space",
            Self::Project => "project",
        }
    }

    /// Parses a stable scope key.
    /// 解析稳定层键。
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "user" => Some(Self::User),
            "space" => Some(Self::Space),
            "project" => Some(Self::Project),
            _ => None,
        }
    }

    /// Returns the conflict priority; higher wins (project > space > user).
    /// 返回冲突优先级；数值越大越优先（项目 > 空间 > 用户）。
    pub const fn priority(self) -> u8 {
        match self {
            Self::User => 0,
            Self::Space => 1,
            Self::Project => 2,
        }
    }

    /// Returns whether the scope separates an injected base section from a catalog.
    /// 返回该层是否区分全文注入的基础分区与目录。
    pub const fn has_base_section(self) -> bool {
        !matches!(self, Self::User)
    }
}

/// Stable key of the base section injected in full for space and project scopes.
/// 空间与项目层全文注入的基础分区稳定键。
pub const BASE_SECTION_KEY: &str = "base";

/// Returns the preset `(key, default title)` sections created with a new file of the scope.
/// 返回新建该层文件时创建的预置 `(键, 默认标题)` 分区。
pub fn preset_sections(scope: MemoryScope) -> &'static [(&'static str, &'static str)] {
    match scope {
        MemoryScope::User => &[
            ("identity", "Identity"),
            ("communication", "Communication"),
            ("expertise", "Expertise"),
        ],
        MemoryScope::Space => &[
            (BASE_SECTION_KEY, "Base"),
            ("conventions", "Conventions"),
            ("environment", "Environment"),
            ("references", "References"),
        ],
        MemoryScope::Project => &[
            (BASE_SECTION_KEY, "Base"),
            ("architecture", "Architecture"),
            ("constraints", "Constraints"),
            ("pitfalls", "Pitfalls"),
            ("in-progress", "In progress"),
        ],
    }
}
