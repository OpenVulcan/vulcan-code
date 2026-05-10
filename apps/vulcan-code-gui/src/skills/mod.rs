//! Session-derived skill input and record models shared by the desktop workbench.
//! 桌面工作台共享的会话派生技能输入与记录模型。

pub mod session;
pub use session::{
    CompactionSkillInput, ExploreSkillInput, SessionDerivedRecord, SessionDerivedSkillChain,
    SummarySkillInput, TitleAgentInput,
};
