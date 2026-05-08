//! Built-in skill registration surfaces for the compose runtime foundation.
//! compose 运行时基础架构使用的内建技能注册面。

pub mod builtin;
pub mod session;

pub use builtin::{BuiltinSkillCatalog, build_builtin_skill_catalog};
pub use session::{
    CompactionSkillInput, ExploreSkillInput, SessionDerivedRecord, SessionDerivedSkillChain,
    SummarySkillInput, TitleAgentInput,
};
