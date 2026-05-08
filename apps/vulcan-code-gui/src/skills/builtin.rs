//! Built-in compose skills derived from persisted runtime configuration.
//! 从持久化运行时配置派生出的内建 compose 技能集合。

use crate::agent_runtime::config::{RuntimeSkillDefinition, RuntimeSkillKind};
use async_trait::async_trait;
use rig_compose::context::{Evidence, InvestigationContext, Signal};
use rig_compose::registry::{KernelError, SkillRegistry, ToolRegistry};
use rig_compose::skill::{Skill, SkillOutcome};
use serde_json::json;
use std::sync::Arc;

/// One lightweight descriptor for a registered built-in skill.
/// 单个已注册内建技能的轻量描述。
#[derive(Debug, Clone)]
pub struct SkillDescriptor {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub id: String,
    /// Human-readable skill name.
    /// 人类可读的技能名称。
    pub name: String,
    /// Skill family label.
    /// 技能类型标签。
    pub kind: RuntimeSkillKind,
}

/// Aggregate built-in skill catalog built for the current runtime configuration.
/// 为当前运行时配置构建的内建技能目录聚合体。
#[derive(Clone)]
pub struct BuiltinSkillCatalog {
    /// Shared skill registry instance.
    /// 共享技能注册表实例。
    pub registry: SkillRegistry,
    /// Lightweight skill descriptors used by placeholder panels.
    /// 占位面板使用的轻量技能描述集合。
    pub descriptors: Vec<SkillDescriptor>,
}

/// Builds the built-in skill catalog from runtime skill definitions.
/// 根据运行时技能定义构建内建技能目录。
pub fn build_builtin_skill_catalog(definitions: &[RuntimeSkillDefinition]) -> BuiltinSkillCatalog {
    let registry = SkillRegistry::new();
    let mut descriptors = Vec::new();

    for definition in definitions.iter().filter(|definition| definition.enabled) {
        descriptors.push(SkillDescriptor {
            id: definition.id.clone(),
            name: definition.name.clone(),
            kind: definition.kind,
        });
        registry.register(Arc::new(CatalogBackedSkill::new(definition.clone())));
    }

    BuiltinSkillCatalog {
        registry,
        descriptors,
    }
}

/// Stateless skill implementation backed by one runtime configuration entry.
/// 由单条运行时配置条目驱动的无状态技能实现。
struct CatalogBackedSkill {
    /// Immutable skill definition.
    /// 不可变技能定义。
    definition: RuntimeSkillDefinition,
}

impl CatalogBackedSkill {
    /// Creates a new stateless skill from one runtime definition.
    /// 根据单条运行时定义创建新的无状态技能。
    fn new(definition: RuntimeSkillDefinition) -> Self {
        Self { definition }
    }
}

#[async_trait]
impl Skill for CatalogBackedSkill {
    /// Returns the stable skill identifier.
    /// 返回稳定技能标识。
    fn id(&self) -> &str {
        &self.definition.id
    }

    /// Returns the human-readable skill description.
    /// 返回人类可读的技能描述。
    fn description(&self) -> &str {
        &self.definition.description
    }

    /// Returns whether the skill is willing to run under the current signal set.
    /// 返回该技能是否愿意在当前信号集合下运行。
    fn applies(&self, ctx: &InvestigationContext) -> bool {
        if self.definition.trigger_signals.is_empty() {
            true
        } else {
            self.definition
                .trigger_signals
                .iter()
                .any(|signal| ctx.has_signal(signal))
        }
    }

    /// Executes the stateless skill against the shared investigation context.
    /// 针对共享调查上下文执行该无状态技能。
    async fn execute(
        &self,
        ctx: &mut InvestigationContext,
        tools: &ToolRegistry,
    ) -> Result<SkillOutcome, KernelError> {
        let runtime_snapshot = tools
            .invoke("workspace.describe", json!({ "skill": self.definition.id }))
            .await
            .ok();

        ctx.evidence.push(
            Evidence::new(self.definition.id.clone(), self.definition.name.clone()).with_detail(
                json!({
                    "kind": self.definition.kind.label(),
                    "triggerSignals": self.definition.trigger_signals,
                    "runtimeSnapshot": runtime_snapshot,
                }),
            ),
        );

        if let Some(signal) = &self.definition.emitted_signal {
            if !ctx.has_signal(signal) {
                ctx.signals.push(Signal::new(signal.clone()));
            }
        }

        Ok(SkillOutcome::default().with_delta(self.definition.confidence_delta))
    }
}
