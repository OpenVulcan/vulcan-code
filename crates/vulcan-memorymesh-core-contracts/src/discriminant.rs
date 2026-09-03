//! Deterministic closed discriminant conversions shared by ABI and canonical codecs.
//! 供 ABI 与规范编解码器共享的确定性封闭判别值转换。

/// Generates one `try_from_u32` and `as_u32` pair for a closed `#[repr(u32)]` enum.
/// 为封闭 `#[repr(u32)]` 枚举生成一对 `try_from_u32` 与 `as_u32`。
#[macro_export]
macro_rules! closed_discriminant {
    ($name:ident { $($variant:ident = $value:literal),+ $(,)? }) => {
        impl $name {
            /// Parses one closed discriminant into the enum without any fallback value.
            /// 将一个封闭判别值解析为枚举且不进行任何兜底。
            #[must_use]
            pub const fn try_from_u32(value: u32) -> Option<Self> {
                match value {
                    $($value => Some($name::$variant),)+
                    _ => None,
                }
            }

            /// Reports the stable numeric discriminant of one value.
            /// 报告一个值的稳定数值判别。
            #[must_use]
            pub const fn as_u32(self) -> u32 {
                self as u32
            }
        }
    };
}

use crate::{
    Admission, CommandStatus, ContextEvidenceRelation, EffectDeadlineClass, EntityKind, LlmPurpose,
    MemoryAccessClass, MemoryCategory, MemoryLevel, MemoryScopeClass, MemorySourceKind,
    MemoryStability, MemoryState, MutationSourceKind, NoiseDecisionSource, PostActionInputKind,
    ProfileLevel, ProfileNodeState, ProfileSourceKind, ProfileViewMode, RawScoreKind, RecallMode,
    RetentionReasonCode, Role, RrfMathProfile, RrfScoreMode, ScoreDirection, SectionRole,
    SessionRecallMode, StructuredOutputContractId, TurnInputKind, TurnStatus,
};

closed_discriminant!(Role { User = 1, Assistant = 2, System = 3, Tool = 4 });
closed_discriminant!(RrfMathProfile { Signed64WrappingThenFloat64V1 = 1, Float64AddV1 = 2 });
closed_discriminant!(MemoryCategory {
    General = 0,
    ArchitectureDecision = 1,
    TechSpecApi = 2,
    BusinessLogic = 3,
    RequirementTodo = 4,
    ProjectContext = 5,
    LogicalBugDebt = 6,
    SecurityPolicy = 7
});
closed_discriminant!(MemorySourceKind { Turn = 1, Direct = 2, Import = 3, SystemSeed = 4, Legacy = 5 });
closed_discriminant!(MemoryStability { Working = 1, Stable = 2, Archival = 3 });
closed_discriminant!(MemoryScopeClass { Session = 1, Environment = 2, Subject = 3 });
closed_discriminant!(MemoryAccessClass { SubjectPrivate = 1, ScopeShared = 2 });
closed_discriminant!(MemoryLevel { Session = 1, Phase = 2, Stable = 3, Persistent = 4 });
closed_discriminant!(MemoryState {
    Active = 1,
    Superseded = 2,
    Deleted = 3,
    Expired = 4,
    Recycled = 5
});
closed_discriminant!(ProfileLevel { Transient = 1, Situational = 2, Stable = 3, Persistent = 4 });
closed_discriminant!(ProfileNodeState { Active = 1, Inactive = 2, Pending = 3 });
closed_discriminant!(ProfileSourceKind {
    TurnExtract = 1,
    ManualInstruction = 2,
    SystemSeed = 3,
    RetainedAfterSubjectDelete = 4
});
closed_discriminant!(TurnStatus {
    Pending = 1,
    Completed = 2,
    ProcessedUnknown = 3,
    AnalysisBypassed = 4,
    Corrupted = 5
});
closed_discriminant!(SectionRole { Environment = 1, Subject = 2 });
closed_discriminant!(TurnInputKind { Question = 1, Statement = 2, Mixed = 3 });
closed_discriminant!(ProfileViewMode { Full = 1, Split = 2 });
closed_discriminant!(RecallMode { Auto = 1, Force = 2, Disabled = 3 });
closed_discriminant!(SessionRecallMode { CompactAware = 1, Full = 2 });
closed_discriminant!(PostActionInputKind { SingleTurn = 1, Timeline = 2 });
closed_discriminant!(NoiseDecisionSource { Disabled = 1, TimelineBypass = 2, Regex = 3, Semantic = 4, RegexOnlyFallback = 5 });
closed_discriminant!(CommandStatus { Waiting = 1, Completed = 2, Cancelled = 3, ResultUncertain = 4, Failed = 5 });
closed_discriminant!(Admission { Accept = 1, Drop = 2 });
closed_discriminant!(ContextEvidenceRelation { Support = 1, Rebuttal = 2 });
closed_discriminant!(LlmPurpose { PreCheckIntent = 1, PreCheckReview = 2, PostActionAnalysis = 3, PostActionReview = 6, ProfileInstruction = 4 });
closed_discriminant!(EffectDeadlineClass { Interactive = 1, Background = 2, Maintenance = 3 });
closed_discriminant!(StructuredOutputContractId { PreCheckIntent = 1, PreCheckReview = 2, PostActionAnalysis = 3, PostActionReview = 6, ProfileInstruction = 4 });
closed_discriminant!(EntityKind { Turn = 1, Memory = 2, ProfileNode = 3, ContextEvidence = 4 });
closed_discriminant!(MutationSourceKind { HostRpc = 1, BackgroundWorker = 2, Import = 3, InvocationScoped = 4 });
closed_discriminant!(RawScoreKind { CosineDistance = 1, CosineSimilarity = 2, Bm25 = 3, TokenOverlap = 4, TrigramSimilarity = 5, ReciprocalRankFusion = 6, L2Distance = 7 });
closed_discriminant!(ScoreDirection { LowerIsBetter = 1, HigherIsBetter = 2 });
closed_discriminant!(RrfScoreMode { RankNormalized = 1, FusedRankOnly = 2 });
closed_discriminant!(RetentionReasonCode { TerminalMemoryRecycle = 1, ColdTurnRecycle = 2, IdleSessionRecycle = 3, TrashExpiryPurge = 4, VectorGcRetry = 5 });
