//! Compatibility-oriented custom iced widget entrypoints for Vulcan surfaces.
//! 面向 Vulcan 界面的自定义 iced 组件兼容入口。

pub mod text_editor;

/// Widget namespace that mirrors the iced export layout for drop-in migration.
/// 对齐 iced 导出布局以便平滑迁移的组件命名空间。
pub mod widget {
    /// Text editor compatibility namespace for future Vulcan widget overrides.
    /// 预留给 Vulcan 后续组件重写的文本编辑器兼容命名空间。
    pub mod text_editor {
        pub use crate::text_editor::*;
    }
}

pub use text_editor::{
    Action, Binding, Catalog, Content, Cursor, Edit, KeyPress, Line, LineEnding, Motion, Position,
    Selection, Status, Style, TextEditor, text_editor,
};
