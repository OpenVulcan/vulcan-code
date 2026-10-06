//! Atomic chronological freezing of host-owned user prompt media parameters.
//! 宿主用户提示词媒体参数的原子时序冻结。

use super::{DirtyFile, SessionDocument, validate_timestamp};
use crate::v1::DomainError;
use crate::v1::event::{ConversationEvent, EventBody};
use crate::v1::user_prompt::{
    UserPromptMediaBinding, UserPromptMediaDelivery, UserPromptResourceBinding,
    invalid_prompt_media,
};

impl SessionDocument {
    /// Returns pending prompt sequences that may still freeze before their first API in the current open turn.
    /// 返回当前开放回合中仍可在首次API前冻结的待准备提示词序号。
    /// The chronological list is side-effect-free and lets the host reject historical uploads before any I/O.
    /// 此时序列表没有副作用，使宿主能在任何IO之前拒绝历史上传。
    pub fn preparing_user_prompt_media_seqs(&self) -> Vec<crate::v1::Seq> {
        // No open turn or an already active API means no new physical preparation is eligible.
        // 没有开放回合或已有活动API时，不存在可执行的新物理准备。
        let Some(turn) = self.active_turn().filter(|turn| !turn.closed) else {
            return Vec::new();
        };
        if self.active_api().is_some() {
            return Vec::new();
        }
        // The last API boundary excludes every earlier pending prompt, regardless of cache state.
        // 最后一个API边界排除全部更早待准备提示词，与缓存状态无关。
        let last_api_seq = self
            .events
            .iter()
            .rev()
            .find(|event| matches!(event.event, EventBody::ApiStarted(_)))
            .map(ConversationEvent::seq);
        self.events
            .iter()
            .filter_map(|event| {
                if event.turn_id != Some(turn.turn_id)
                    || event.seq() < self.active_tail_start()
                    || last_api_seq.is_some_and(|last| event.seq() <= last)
                {
                    return None;
                }
                let EventBody::UserInput(input) = &event.event else {
                    return None;
                };
                input
                    .parameters
                    .as_ref()
                    .filter(|parameters| {
                        matches!(
                            parameters.media_delivery,
                            UserPromptMediaDelivery::Pending
                                | UserPromptMediaDelivery::Preparing { .. }
                        )
                    })
                    .map(|_| event.seq())
            })
            .collect()
    }

    /// Freezes `bindings` in strictly increasing prompt sequence order before their first physical API.
    /// 在各提示词首次物理API之前，按严格递增提示词序号冻结`bindings`。
    /// `now_ms` records the mutation time; event sequence, not wall-clock sorting, owns chronology.
    /// `now_ms`记录修改时间；时序由事件序号持有，不按墙上时钟排序。
    /// Returns whether any prompt changed; identical repeats are idempotent and all errors leave state unchanged.
    /// 返回是否有提示词变化；相同重复保持幂等，全部错误均保持状态不变。
    pub fn freeze_user_prompt_media(
        &mut self,
        bindings: Vec<UserPromptMediaBinding>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let staged = self.stage_user_prompt_media(bindings, now_ms)?;
        if staged.is_empty() {
            return Ok(false);
        }
        self.ensure_transition_capacity(0, 0)?;
        // Commit every prevalidated prompt together and publish their existing event positions as one revision.
        // 一起提交全部预校验提示词，并将其既有事件位置作为一个修订发布。
        let changed_seqs = staged
            .iter()
            .map(|(_, event)| event.seq())
            .collect::<Vec<_>>();
        for (index, event) in staged {
            self.events.replace(index, event);
        }
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &changed_seqs);
        Ok(true)
    }

    /// Records selected or completed `binding` under the captured `segment_generation` at `now_ms`, without freezing a request route.
    /// 在捕获的`segment_generation`下于`now_ms`记录已选或已完成`binding`，不冻结请求路线。
    /// Returns whether the original event changed; identical repeats are idempotent and errors leave all state unchanged.
    /// 返回原事件是否变化；相同重复幂等，错误保持全部状态不变。
    /// The host must retain its preparation lease; generation, occurrence and first-API boundaries are checked again here.
    /// 宿主必须保留其准备租约；此处再次检查代际、出现位置及首次API边界。
    /// Only the original runtime-selected upload may fill its missing receipt; imported admission proof and frozen history remain immutable.
    /// 仅原运行时所选上传可补齐缺失回执；导入准入证明及冻结历史保持不可变。
    pub fn record_user_prompt_resource_delivery(
        &mut self,
        segment_generation: u64,
        binding: UserPromptResourceBinding,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("user_prompt_media.updated_at_ms", now_ms)?;
        if self.segment_generation() != segment_generation {
            return Err(invalid_prompt_media("segment_generation"));
        }
        // The event sequence and resource position jointly own the result, including duplicate file ids.
        // 事件序号和资源位置共同持有结果，包括重复文件ID。
        let index = self
            .events
            .binary_search_by_key(&binding.prompt_seq, ConversationEvent::seq)
            .map_err(|_| invalid_prompt_media("prompt_missing"))?;
        // Keep the live event untouched until every invariant and capacity check succeeds.
        // 在全部不变量和容量检查成功之前，保持实时事件不变。
        let event = &self.events[index];
        let EventBody::UserInput(input) = &event.event else {
            return Err(invalid_prompt_media("not_user_prompt"));
        };
        if input.resource_ids.get(binding.resource_position) != Some(&binding.resource.resource_id)
        {
            return Err(invalid_prompt_media("source_resource_occurrence"));
        }
        // An absent legacy parameter never acquires new-intent authority through a late completion.
        // 旧缺失参数绝不通过迟到完成获得新意图权限。
        let parameters = input
            .parameters
            .as_ref()
            .ok_or_else(|| invalid_prompt_media("legacy_origin_unknown"))?;
        parameters.validate(&input.resource_ids)?;
        if let Some(original) = parameters.original_resource(binding.resource_position) {
            if original == &binding.resource {
                return Ok(false);
            }
            // A live original upload may fill its one missing receipt; no route, contract or completed result may be replaced.
            // 活动原上传可补齐其唯一缺失回执；不能替换路线、契约或已完成结果。
            if !binding.resource.completes_upload(original) {
                return Err(invalid_prompt_media("completed_delivery_is_immutable"));
            }
        }
        if event.seq() < self.active_tail_start() {
            return Err(invalid_prompt_media("sealed_prompt"));
        }
        if self.events[index + 1..]
            .iter()
            .any(|later| matches!(later.event, EventBody::ApiStarted(_)))
        {
            return Err(invalid_prompt_media("first_request_boundary_passed"));
        }
        // Seed only from declared admission proof or the exact existing partial state, never from a newer cache.
        // 仅从声明的准入证明或精确既有部分状态初始化，绝不使用更新缓存。
        let mut resources = match &parameters.media_delivery {
            UserPromptMediaDelivery::Pending if parameters.prepared_resources.is_empty() => {
                vec![None; input.resource_ids.len()]
            }
            UserPromptMediaDelivery::Pending => parameters.prepared_resources.clone(),
            UserPromptMediaDelivery::Preparing { resources } => resources.clone(),
            UserPromptMediaDelivery::Frozen { .. } => {
                return Err(invalid_prompt_media("original_delivery_is_immutable"));
            }
        };
        resources[binding.resource_position] = Some(binding.resource);
        // Initial submission proof remains immutable, so an exact submit retry still compares the same input.
        // 初始提交证明保持不可变，使精确提交重试仍比较同一输入。
        let mut preparing = parameters.clone();
        preparing.media_delivery = UserPromptMediaDelivery::Preparing { resources };
        preparing.validate(&input.resource_ids)?;
        // Only the original envelope's update time changes; no new event sequence or request route is invented.
        // 仅原信封更新时间变化；不虚构新事件序号或请求路线。
        let mut updated = event.clone();
        updated.mutate_in_place(now_ms.max(updated.updated_at_ms), |body| {
            let EventBody::UserInput(input) = body else {
                unreachable!("the exact prompt event was validated before staging")
            };
            input.parameters = Some(preparing);
        })?;
        self.ensure_transition_capacity(0, 0)?;
        self.events.replace(index, updated);
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[binding.prompt_seq]);
        Ok(true)
    }

    /// Prevalidates ordered `bindings` at `now_ms` and returns changed event copies without mutating this owner.
    /// 在`now_ms`预校验有序`bindings`，返回已修改事件副本，不修改本所有者。
    /// The physical API transaction can commit these prompt-owned fields with its own start in one revision.
    /// 物理API事务可在一个修订内提交这些提示词所有字段及自身起始。
    pub(super) fn stage_user_prompt_media(
        &self,
        bindings: Vec<UserPromptMediaBinding>,
        now_ms: i64,
    ) -> Result<Vec<(usize, ConversationEvent)>, DomainError> {
        validate_timestamp("user_prompt_media.updated_at_ms", now_ms)?;
        if bindings
            .windows(2)
            .any(|pair| pair[0].prompt_seq >= pair[1].prompt_seq)
        {
            return Err(invalid_prompt_media("prompt_order"));
        }
        // Staged event copies are transaction before-images, not a second live session authority.
        // 暂存事件副本是事务前镜像，不是第二套实时会话权威。
        let mut staged = Vec::<(usize, ConversationEvent)>::new();
        for binding in bindings {
            // Resolve the exact existing event once; never substitute the latest prompt or a matching file elsewhere.
            // 只解析精确既有事件，绝不替换为最新提示词或其他位置的相同文件。
            let index = self
                .events
                .binary_search_by_key(&binding.prompt_seq, ConversationEvent::seq)
                .map_err(|_| invalid_prompt_media("prompt_missing"))?;
            let event = &self.events[index];
            let EventBody::UserInput(input) = &event.event else {
                return Err(invalid_prompt_media("not_user_prompt"));
            };
            binding.origin.validate(&input.resource_ids)?;
            let parameters = input
                .parameters
                .as_ref()
                .ok_or_else(|| invalid_prompt_media("legacy_origin_unknown"))?;
            parameters.validate(&input.resource_ids)?;
            parameters.validate_preserved_preparation(&binding.origin)?;
            if let UserPromptMediaDelivery::Frozen { origin } = &parameters.media_delivery {
                if origin != &binding.origin {
                    return Err(invalid_prompt_media("original_delivery_is_immutable"));
                }
                continue;
            }
            if event.seq() < self.active_tail_start() {
                return Err(invalid_prompt_media("sealed_prompt"));
            }
            // An API anywhere after this prompt proves its first request boundary has already passed.
            // 本提示词之后任意API都证明其首次请求边界已经过去。
            // Later snapshots or upload cache entries must never backfill the old prompt's original identity.
            // 后来的快照或上传缓存绝不能回填旧提示词的原身份。
            if self.events[index + 1..]
                .iter()
                .any(|later| matches!(&later.event, EventBody::ApiStarted(_)))
            {
                return Err(invalid_prompt_media("first_request_boundary_passed"));
            }
            let turn = self.require_open_turn()?;
            if event.turn_id != Some(turn.turn_id) || self.active_api().is_some() {
                return Err(invalid_prompt_media("prompt_turn_not_preparing"));
            }
            // Clamp only update time to the existing envelope; sequence and creation time remain untouched.
            // 仅将更新时间约束到既有信封时间；序号及创建时间保持不变。
            let mut updated = event.clone();
            let updated_at_ms = now_ms.max(updated.updated_at_ms);
            updated.mutate_in_place(updated_at_ms, |body| {
                let EventBody::UserInput(input) = body else {
                    unreachable!("the exact prompt event was validated before staging")
                };
                // Keep the admitted preparation proof unchanged for exact submission retries after freezing.
                // 冻结后为精确提交重试保持准入准备证据不变。
                let mut frozen = parameters.clone();
                frozen.media_delivery = UserPromptMediaDelivery::Frozen {
                    origin: binding.origin,
                };
                input.parameters = Some(frozen);
            })?;
            staged.push((index, updated));
        }
        Ok(staged)
    }
}
