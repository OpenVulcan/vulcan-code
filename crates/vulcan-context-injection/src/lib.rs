//! Shared context-injection primitives for hook-driven provider context overlays.
//! 面向 hook 驱动 Provider 上下文覆盖层的共享上下文注入原语。

#[path = "context_injection/render.rs"]
mod render;
#[path = "context_injection/state.rs"]
mod state;
#[path = "context_injection/types.rs"]
mod types;
#[path = "context_injection/util.rs"]
mod util;

pub use render::{
    InjectionItemRenderer, InjectionPanelCard, InjectionPanelItem, InjectionRenderOptions,
    InjectionRendererRegistry, ResolvedInjectionHostBlock, ResolvedInjectionView,
};
pub use state::{PERSISTENT_SESSION_SCOPE, TRANSIENT_TURN_SCOPE};
pub use types::{
    ContextInjectionState, InjectionCompactionPolicy, InjectionExpiryEvent, InjectionHelper,
    InjectionInsert, InjectionInsertItem, InjectionKind, InjectionPruneReport, InjectionScope,
    InjectionStateGroup, InjectionStateItem,
};

#[cfg(test)]
#[path = "context_injection/tests.rs"]
mod tests;
