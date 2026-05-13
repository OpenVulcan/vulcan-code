//! Selection-oriented editor primitives adapted from Ferrite (MIT).
//! 改编自 Ferrite（MIT）的面向选择能力编辑器基础模块。

pub mod buffer;
pub mod cursor;
pub mod grapheme;
pub mod view;

pub use buffer::TextBuffer;
pub use cursor::{Cursor, Selection};
pub use view::ViewState;
