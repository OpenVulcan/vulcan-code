import React from "react";
import type { ComposerModelSelectionSnapshot } from "@/components/composer/composer-model";
import { useComposerMediaUploadProgresses } from "@/components/composer/composer-media-upload-context";
import {
  cancelConversationSubmission,
  planComposerAttachments,
  prepareComposerAttachment,
  reconcileComposerAttachmentDraftLease,
} from "@/service/conversation-actions-service";
import type {
  ComposerAttachmentPreparationSummary,
  ComposerAttachmentSummary,
  MediaUploadProgressChangedDelta,
} from "@/types";

// ComposerAttachmentPreparationController exposes draft preparation state and stable user actions.
// ComposerAttachmentPreparationController 暴露草稿准备状态与稳定用户动作。
export interface ComposerAttachmentPreparationController {
  // Preparation states indexed by canonical attachment identity.
  // 按规范附件身份索引的准备状态。
  preparations: Readonly<Record<string, ComposerAttachmentPreparationSummary>>;
  // Active progress rows indexed by canonical attachment identity.
  // 按规范附件身份索引的活跃进度行。
  progresses: Readonly<Record<string, MediaUploadProgressChangedDelta>>;
  // Whether route planning or a required transfer currently blocks prompt submission.
  // 路线规划或必需传输当前是否阻止提示词提交。
  blocksSubmission: boolean;
  // Number of canonical attachments currently covered by the submission barrier.
  // 当前被发送屏障覆盖的规范附件数量。
  blockingCount: number;
  // Number of attachments that will be disclosed only as metadata.
  // 仅以元数据形式告知模型的附件数量。
  metadataOnlyCount: number;
  // Projects transient delivery decisions onto the exact submission copy of canonical attachments.
  // 将临时传输决策投影到规范附件的精确提交副本。
  projectForSubmission: (attachments: ComposerAttachmentSummary[]) => ComposerAttachmentSummary[];
  // Retries one failed or cancelled provider upload under the same frozen route plan.
  // 在相同冻结路线规划下重试单个失败或已取消的供应商上传。
  retry: (attachmentId: string) => Promise<void>;
  // Cancels one active provider upload before its attachment is removed.
  // 在移除附件前取消单个活跃供应商上传。
  cancel: (attachmentId: string) => Promise<void>;
  // Reconciles the exact draft owner before a local attachment-set replacement is published.
  // 在发布本地附件集合替换前协调精确草稿所有者。
  reconcileDraft: (attachments: ComposerAttachmentSummary[]) => Promise<void>;
}

// attachmentIdentity resolves the service-issued stable identity required by planning and cancellation.
// attachmentIdentity 解析规划与取消所需的服务签发稳定身份。
// Parameters: attachment is one canonical staged composer resource.
// 参数：attachment 是一条规范已暂存输入区资源。
// Returns: the stable asset id, or null when the service violated the staging contract.
// 返回值：稳定资源标识；若服务违反暂存契约则返回 null。
function attachmentIdentity(attachment: ComposerAttachmentSummary): string | null {
  const identity = attachment.assetId?.trim();
  return identity ? identity : null;
}

// composerAttachmentRouteReadyForPlanning closes every optimistic or unresolved route window.
// composerAttachmentRouteReadyForPlanning 封闭全部乐观或未解析路线窗口。
// Parameters: selection is the rendered route tuple, revision is backend-authored, and pending tracks persistence.
// 参数：selection 是渲染路线元组，revision 由后端生成，pending 跟踪持久化状态。
// Returns: true only when the exact route is durable and content-addressed.
// 返回值：仅当精确路线已持久且具备内容身份时返回 true。
export function composerAttachmentRouteReadyForPlanning(
  selection: ComposerModelSelectionSnapshot | null,
  routeRevision: string,
  routeSelectionPending: boolean,
): selection is ComposerModelSelectionSnapshot {
  return !routeSelectionPending && selection !== null && routeRevision.trim().length > 0;
}

// composerAttachmentPlansMatchDraft validates one complete service plan before removing the send barrier.
// composerAttachmentPlansMatchDraft 在解除发送屏障前校验一份完整服务规划。
// Parameters: planned contains service rows, attachmentIds is the exact draft set, and generation/revision freeze the route.
// 参数：planned 包含服务端记录，attachmentIds 是精确草稿集合，generation/revision 冻结路线。
// Returns: true only for a one-to-one, current-generation, exact-route response.
// 返回值：仅当响应一一对应、代际当前且路线精确时返回 true。
export function composerAttachmentPlansMatchDraft(
  planned: readonly ComposerAttachmentPreparationSummary[],
  attachmentIds: readonly string[],
  generation: number,
  routeRevision: string,
): boolean {
  if (planned.length !== attachmentIds.length) {
    return false;
  }
  const expectedIds = new Set(attachmentIds);
  const plannedIds = new Set<string>();
  for (const preparation of planned) {
    const attachmentId = preparation.attachmentId.trim();
    if (
      !attachmentId
      || !expectedIds.has(attachmentId)
      || plannedIds.has(attachmentId)
      || preparation.generation !== generation
      || preparation.routeRevision !== routeRevision
      || !preparation.transferId.trim()
    ) {
      return false;
    }
    plannedIds.add(attachmentId);
  }
  return plannedIds.size === expectedIds.size;
}

// composerAttachmentUploadTransferIds returns every backend-owned upload plan in one response.
// composerAttachmentUploadTransferIds 返回单份响应中全部由后端持有的上传规划标识。
// Parameters: planned contains the exact rows returned by one planning RPC.
// 参数：planned 包含单次规划 RPC 返回的精确记录。
// Returns: unique non-empty transfer ids that must be consumed or explicitly cancelled.
// 返回值：必须被消费或显式取消的唯一非空传输标识。
export function composerAttachmentUploadTransferIds(
  planned: readonly ComposerAttachmentPreparationSummary[],
): string[] {
  return Array.from(new Set(planned
    .filter((preparation) => preparation.deliveryPlan === "upload_reference")
    .map((preparation) => preparation.transferId.trim())
    .filter(Boolean)));
}

// cancelUnconsumedComposerAttachmentPlans releases backend plans rejected by the current draft generation.
// cancelUnconsumedComposerAttachmentPlans 释放被当前草稿代际拒绝的后端上传规划。
// Parameters: planned is a complete or malformed planning response that will not be consumed.
// 参数：planned 是一份不会被消费的完整或异常规划响应。
// Returns: a settled boundary after every unique backend plan has received cancellation.
// 返回值：全部唯一后端规划都收到取消请求后的已完成边界。
async function cancelUnconsumedComposerAttachmentPlans(
  planned: readonly ComposerAttachmentPreparationSummary[],
): Promise<void> {
  await Promise.allSettled(composerAttachmentUploadTransferIds(planned)
    .map((transferId) => cancelConversationSubmission(transferId)));
}

// composerAttachmentCancelledState revokes one active transfer while preserving metadata disclosure.
// composerAttachmentCancelledState 撤销一个活跃传输，同时保留元数据告知能力。
// Parameters: preparation is the active plan and cancellationId is a newly issued terminal identity.
// 参数：preparation 是活跃规划，cancellationId 是新签发的终止身份。
// Returns: a cancelled, non-blocking projection that rejects completion from the former transfer.
// 返回值：一个已取消、不阻塞的投影，并拒绝原传输的迟到完成结果。
export function composerAttachmentCancelledState(
  preparation: ComposerAttachmentPreparationSummary,
  cancellationId: string,
): ComposerAttachmentPreparationSummary {
  return {
    ...preparation,
    transferId: cancellationId,
    phase: "cancelled",
    blocksSubmission: false,
    reason: "draft_upload_cancelled_metadata_only",
  };
}

// queueComposerDraftLeaseAfterCancellation keeps resource release behind one provider cancellation boundary.
// queueComposerDraftLeaseAfterCancellation 将资源释放严格排在供应商取消边界之后。
// Parameters: current is the existing lease queue and cancellation is the active transfer cancellation.
// 参数：current 是现有租约队列，cancellation 是活跃传输取消任务。
// Returns: a non-rejecting queue tail that settles only after both boundaries complete.
// 返回值：一个不拒绝的队列尾，仅在两个边界都完成后结束。
export function queueComposerDraftLeaseAfterCancellation(
  current: Promise<void>,
  cancellation: Promise<unknown>,
): Promise<void> {
  return current.then(
    () => cancellation,
    () => cancellation,
  ).then(
    () => undefined,
    () => undefined,
  );
}

// useComposerAttachmentPreparation plans and prepares attachments before prompt submission.
// useComposerAttachmentPreparation 在提示词提交前规划并准备附件。
// Parameters: conversation and model identities freeze upload scope; attachments are the canonical draft set.
// 参数：会话与模型身份冻结上传作用域；attachments 是规范草稿集合。
// Returns: current preparation facts plus retry and cancellation actions.
// 返回值：当前准备事实以及重试与取消动作。
export function useComposerAttachmentPreparation(options: {
  // Stable composer draft slot that owns unsent managed attachments across app restarts.
  // 跨应用重启持有未发送托管附件的稳定输入区草稿槽。
  draftSlotKey: string;
  // Current project id owning the staged attachment asset store.
  // 当前持有已暂存附件资产存储的项目标识。
  projectId: string | null;
  // Current persisted conversation id, or null for a local draft.
  // 当前已持久化会话标识；本地草稿为 null。
  conversationId: string | null;
  // Exact rendered provider, model, and credential selection.
  // 当前渲染的精确供应商、模型与凭据选择。
  modelSelection: ComposerModelSelectionSnapshot | null;
  // Backend-resolved content revision of the exact rendered executable route.
  // 后端解析出的当前精确可执行路线内容修订号。
  routePlanningRevision: string;
  // Whether model or secondary-route persistence has not reached its durable fixed point.
  // 模型或二级路由持久化是否尚未到达持久不动点。
  routeSelectionPending: boolean;
  // Canonical staged draft attachments.
  // 规范已暂存草稿附件。
  attachments: ComposerAttachmentSummary[];
}): ComposerAttachmentPreparationController {
  // serviceProgresses provides ordered byte facts from the desktop upload bridge.
  // serviceProgresses 提供桌面上传桥发出的有序字节事实。
  const serviceProgresses = useComposerMediaUploadProgresses();
  // preparations stores the latest closed state for each canonical attachment.
  // preparations 保存每条规范附件的最新封闭状态。
  const [preparations, setPreparations] = React.useState<Record<string, ComposerAttachmentPreparationSummary>>({});
  // planningGeneration identifies the latest attachment/model plan and rejects late async results.
  // planningGeneration 标识最新附件/模型规划并拒绝迟到异步结果。
  const planningGeneration = React.useRef(0);
  // retryPlanningTransfers records the exact retry plan currently allowed to publish per attachment.
  // retryPlanningTransfers 记录每个附件当前获准发布结果的精确重试规划。
  const retryPlanningTransfers = React.useRef(new Map<string, string>());
  // planningPending exposes the plan interval as an explicit send barrier.
  // planningPending 把规划区间作为显式发送屏障暴露。
  const [planningPending, setPlanningPending] = React.useState(false);
  // leaseQueue serializes exact-set replacements so an older async request cannot reacquire a removed attachment.
  // leaseQueue 串行化精确集合替换，避免较早异步请求重新取得已移除附件。
  const leaseQueue = React.useRef<Promise<void>>(Promise.resolve());
  // leaseCommittedSignature identifies the exact draft set durably protected by the backend index.
  // leaseCommittedSignature 标识已由后端索引持久保护的精确草稿集合。
  const [leaseCommittedSignature, setLeaseCommittedSignature] = React.useState("");
  // leaseError exposes one controlled, retryable draft-persistence failure instead of an opaque permanent barrier.
  // leaseError 暴露一条受控且可重试的草稿持久化失败，避免形成无提示的永久屏障。
  const [leaseError, setLeaseError] = React.useState<string | null>(null);
  // expectedLeaseSignatureRef prevents a superseded lease response from publishing failure or success into a newer draft.
  // expectedLeaseSignatureRef 防止已过期的租约响应向更新草稿发布失败或成功。
  const expectedLeaseSignatureRef = React.useRef("");
  // preparationsRef lets cleanup and stable callbacks inspect the exact latest transfer identities.
  // preparationsRef 让清理与稳定回调读取精确的最新传输身份。
  const preparationsRef = React.useRef(preparations);
  // commitPreparations publishes one synchronous authoritative snapshot before React renders it.
  // commitPreparations 在 React 渲染前同步发布一份权威快照。
  const commitPreparations = React.useCallback((
    update: (
      current: Record<string, ComposerAttachmentPreparationSummary>,
    ) => Record<string, ComposerAttachmentPreparationSummary>,
  ) => {
    const current = preparationsRef.current;
    const next = update(current);
    if (next === current) {
      return;
    }
    preparationsRef.current = next;
    setPreparations(next);
  }, []);

  // attachmentSignature changes only when the canonical draft set changes.
  // attachmentSignature 仅在规范草稿集合变化时改变。
  const attachmentSignature = React.useMemo(
    () => JSON.stringify(options.attachments.map((attachment) => [
      attachmentIdentity(attachment) ?? "missing",
      attachment.byteLen ?? -1,
      attachment.mimeType,
      attachment.sourceSha256 ?? "",
    ])),
    [options.attachments],
  );
  // expectedLeaseSignature binds the durable owner to this exact slot, workspace, and canonical attachment set.
  // expectedLeaseSignature 将持久所有者绑定到当前精确草稿槽、工作区与规范附件集合。
  const expectedLeaseSignature = canonicalDraftLeaseSignature(
    options.draftSlotKey,
    options.projectId,
    options.conversationId,
    options.attachments,
  );
  expectedLeaseSignatureRef.current = expectedLeaseSignature;
  // modelSignature changes only when the exact route-selection tuple changes.
  // modelSignature 仅在精确路线选择元组变化时改变。
  const modelSignature = options.modelSelection
    ? JSON.stringify([
      options.modelSelection.providerId,
      options.modelSelection.modelId,
      options.modelSelection.connectionId,
      options.routePlanningRevision,
    ])
    : "";
  // canonicalAttachments retains an equal draft snapshot across parent renders that rebuild the array.
  // canonicalAttachments 在父组件重建等值数组的渲染之间保留同一份草稿快照。
  const canonicalAttachments = React.useMemo(
    () => options.attachments,
    [attachmentSignature],
  );
  // modelSelectionSnapshot retains an equal route tuple across parent renders that rebuild the object.
  // modelSelectionSnapshot 在父组件重建等值对象的渲染之间保留同一份路由元组。
  const modelSelectionSnapshot = React.useMemo<ComposerModelSelectionSnapshot | null>(
    () => options.modelSelection,
    [modelSignature],
  );
  // attachmentById provides one unique canonical resource for each service-issued identity.
  // attachmentById 为每个服务签发身份提供唯一规范资源。
  const attachmentById = React.useMemo(() => {
    const entries = canonicalAttachments.flatMap((attachment) => {
      const identity = attachmentIdentity(attachment);
      return identity ? [[identity, attachment] as const] : [];
    });
    return new Map(entries);
  }, [canonicalAttachments]);

  // enqueueDraftLeaseReconciliation orders one exact backend owner replacement after every earlier replacement.
  // enqueueDraftLeaseReconciliation 将单次精确后端所有者替换排在全部先前替换之后。
  const enqueueDraftLeaseReconciliation = React.useCallback((
    attachments: ComposerAttachmentSummary[],
  ) => {
    const requestedSignature = canonicalDraftLeaseSignature(
      options.draftSlotKey,
      options.projectId,
      options.conversationId,
      attachments,
    );
    const request = async () => {
      try {
        await reconcileComposerAttachmentDraftLease(
          options.projectId,
          options.conversationId,
          options.draftSlotKey,
          attachments,
        );
        if (expectedLeaseSignatureRef.current === requestedSignature) {
          setLeaseCommittedSignature(requestedSignature);
          setLeaseError(null);
        }
      } catch (error) {
        if (expectedLeaseSignatureRef.current === requestedSignature) {
          setLeaseError(error instanceof Error
            ? error.message
            : "Attachment draft persistence failed; retry before sending.");
        }
        throw error;
      }
    };
    const result = leaseQueue.current.then(request, request);
    leaseQueue.current = result.catch(() => undefined);
    return result;
  }, [options.conversationId, options.draftSlotKey, options.projectId]);

  React.useEffect(() => {
    if (!canonicalAttachments.length) {
      setLeaseCommittedSignature("");
      setLeaseError(null);
      return;
    }
    void enqueueDraftLeaseReconciliation(canonicalAttachments).catch(() => {
      // A failed lease remains a closed send barrier; a later attachment or slot revision retries it.
      // 失败租约保持封闭发送屏障；后续附件或草稿槽修订会重新尝试。
    });
  }, [canonicalAttachments, enqueueDraftLeaseReconciliation]);

  // executePreparation performs one upload plan and ignores completion from superseded generations.
  // executePreparation 执行单条上传规划，并忽略已被后续代际取代的完成结果。
  const executePreparation = React.useCallback(async (
    generation: number,
    attachment: ComposerAttachmentSummary,
    preparation: ComposerAttachmentPreparationSummary,
  ) => {
    const attachmentId = attachmentIdentity(attachment);
    if (!attachmentId || !modelSelectionSnapshot) {
      return;
    }
    commitPreparations((current) => ({
      ...current,
      [attachmentId]: {
        ...preparation,
        phase: "uploading",
        blocksSubmission: true,
        error: null,
      },
    }));
    try {
      const terminal = await prepareComposerAttachment(
        options.projectId,
        options.conversationId,
        modelSelectionSnapshot,
        attachment,
        preparation,
      );
      if (planningGeneration.current !== generation) {
        return;
      }
      commitPreparations((current) => {
        const active = current[attachmentId];
        return active?.transferId === preparation.transferId
          ? { ...current, [attachmentId]: terminal }
          : current;
      });
    } catch (error) {
      if (planningGeneration.current !== generation) {
        return;
      }
      // failureText is user-visible diagnostic text while the canonical attachment remains available as metadata.
      // failureText 是面向用户的诊断文本，规范附件仍可按元数据方式提供。
      const failureText = error instanceof Error
        ? error.message
        : "Attachment upload failed; the attachment will be disclosed as metadata only.";
      commitPreparations((current) => {
        const active = current[attachmentId];
        if (active?.transferId !== preparation.transferId) {
          return current;
        }
        return {
          ...current,
          [attachmentId]: {
            ...preparation,
            phase: "failed",
            blocksSubmission: false,
            reason: "draft_upload_failed_metadata_only",
            error: failureText,
          },
        };
      });
    }
  }, [commitPreparations, modelSelectionSnapshot, options.conversationId, options.projectId]);

  React.useEffect(() => {
    const generation = planningGeneration.current + 1;
    planningGeneration.current = generation;
    retryPlanningTransfers.current.clear();
    const formerTransfers = Object.values(preparationsRef.current)
      .filter((preparation) => preparation.blocksSubmission)
      .map((preparation) => preparation.transferId);
    for (const transferId of formerTransfers) {
      void cancelConversationSubmission(transferId);
    }
    if (!canonicalAttachments.length) {
      setPlanningPending(false);
      commitPreparations(() => ({}));
      return;
    }
    if (leaseCommittedSignature !== expectedLeaseSignature) {
      setPlanningPending(true);
      commitPreparations(() => ({}));
      return;
    }
    if (!composerAttachmentRouteReadyForPlanning(
      modelSelectionSnapshot,
      options.routePlanningRevision,
      options.routeSelectionPending,
    )) {
      setPlanningPending(true);
      commitPreparations(() => ({}));
      return;
    }
    if (attachmentById.size !== canonicalAttachments.length) {
      setPlanningPending(true);
      commitPreparations(() => ({}));
      return;
    }
    let disposed = false;
    setPlanningPending(true);
    commitPreparations(() => ({}));
    void planComposerAttachments(
      options.projectId,
      options.conversationId,
      generation,
      modelSelectionSnapshot,
      canonicalAttachments,
    )
      .then(async (planned) => {
        if (disposed || planningGeneration.current !== generation) {
          await cancelUnconsumedComposerAttachmentPlans(planned);
          return;
        }
        if (!composerAttachmentPlansMatchDraft(
          planned,
          Array.from(attachmentById.keys()),
          generation,
          options.routePlanningRevision,
        )) {
          await cancelUnconsumedComposerAttachmentPlans(planned);
          throw new Error("Attachment route planning returned a mismatched draft or route identity.");
        }
        const next = Object.fromEntries(planned.map((preparation) => [preparation.attachmentId, preparation]));
        commitPreparations(() => next);
        setPlanningPending(false);
        for (const preparation of planned) {
          if (preparation.deliveryPlan !== "upload_reference") {
            continue;
          }
          const attachment = attachmentById.get(preparation.attachmentId);
          if (attachment) {
            void executePreparation(generation, attachment, preparation);
          }
        }
      })
      .catch(() => {
        if (disposed || planningGeneration.current !== generation) {
          return;
        }
        const degraded = Object.fromEntries(canonicalAttachments.map((attachment) => {
          const attachmentId = attachmentIdentity(attachment) as string;
          return [attachmentId, {
            attachmentId,
            generation,
            transferId: `draft-media-plan-failed:${generation}:${attachmentId}`,
            deliveryPlan: "metadata_only" as const,
            phase: "metadata_only" as const,
            blocksSubmission: false,
            routeRevision: null,
            reason: "draft_route_planning_failed_metadata_only",
            error: "Attachment route planning failed; the attachment will be disclosed as metadata only.",
          }];
        }));
        commitPreparations(() => degraded);
        setPlanningPending(false);
      });
    return () => {
      disposed = true;
      const activeTransferIds = Object.values(preparationsRef.current)
        .filter((preparation) => preparation.blocksSubmission)
        .map((preparation) => preparation.transferId);
      for (const transferId of activeTransferIds) {
        void cancelConversationSubmission(transferId);
      }
    };
  }, [
    attachmentById,
    canonicalAttachments,
    commitPreparations,
    executePreparation,
    modelSelectionSnapshot,
    options.conversationId,
    options.projectId,
    options.routePlanningRevision,
    options.routeSelectionPending,
    expectedLeaseSignature,
    leaseCommittedSignature,
  ]);

  // progresses selects only transfer rows that still belong to the current canonical draft.
  // progresses 仅选择仍属于当前规范草稿的传输行。
  const progresses = React.useMemo(() => {
    const rows: Record<string, MediaUploadProgressChangedDelta> = {};
    for (const progress of Object.values(serviceProgresses)) {
      const attachmentId = progress.attachmentId?.trim();
      const preparation = attachmentId ? preparations[attachmentId] : undefined;
      if (
        attachmentId
        && attachmentById.has(attachmentId)
        && preparation?.transferId === progress.clientSubmissionId
      ) {
        rows[attachmentId] = progress;
      }
    }
    return rows;
  }, [attachmentById, preparations, serviceProgresses]);

  React.useEffect(() => {
    if (!Object.keys(progresses).length) {
      return;
    }
    commitPreparations((current) => {
      const next = { ...current };
      let changed = false;
      for (const [attachmentId, progress] of Object.entries(progresses)) {
        const preparation = next[attachmentId];
        if (!preparation || preparation.transferId !== progress.clientSubmissionId) {
          continue;
        }
        const phase = progress.phase === "awaiting_provider_confirmation"
          ? "awaiting_provider_confirmation"
          : "uploading";
        if (preparation.phase !== phase) {
          next[attachmentId] = { ...preparation, phase };
          changed = true;
        }
      }
      return changed ? next : current;
    });
  }, [commitPreparations, progresses]);

  // retry replans one closed upload under the current exact route and installs a new transfer identity.
  // retry 在当前精确路由下重新规划单条已封闭上传，并安装新的传输身份。
  const retry = React.useCallback(async (attachmentId: string) => {
    if (leaseError && attachmentById.has(attachmentId)) {
      try {
        await enqueueDraftLeaseReconciliation(canonicalAttachments);
      } catch {
        // enqueueDraftLeaseReconciliation already publishes the exact lease failure for retry UI.
        // enqueueDraftLeaseReconciliation 已经为重试界面发布了精确的租约失败状态。
      }
      return;
    }
    const preparation = preparationsRef.current[attachmentId];
    const attachment = attachmentById.get(attachmentId);
    if (
      !preparation
      || !attachment
      || preparation.deliveryPlan !== "upload_reference"
      || !composerAttachmentRouteReadyForPlanning(
        modelSelectionSnapshot,
        options.routePlanningRevision,
        options.routeSelectionPending,
      )
    ) {
      return;
    }
    const generation = planningGeneration.current;
    // retryPlanningTransferId prevents a late former upload from replacing the retry-planning state.
    // retryPlanningTransferId 防止迟到的旧上传结果覆盖重试规划状态。
    const retryPlanningTransferId = `draft-media-retry-planning:${crypto.randomUUID()}`;
    retryPlanningTransfers.current.set(attachmentId, retryPlanningTransferId);
    commitPreparations((current) => ({
      ...current,
      [attachmentId]: {
        ...preparation,
        transferId: retryPlanningTransferId,
        phase: "uploading",
        blocksSubmission: true,
        error: null,
      },
    }));
    try {
      // plannedRows retains the complete service response so malformed extra upload plans are
      // cancelled instead of being discarded by array destructuring.
      // plannedRows 保留完整服务响应，确保异常附带的额外上传规划会被取消，而不会因数组解构被丢弃。
      const plannedRows = await planComposerAttachments(
        options.projectId,
        options.conversationId,
        generation,
        modelSelectionSnapshot,
        [attachment],
      );
      if (
        planningGeneration.current !== generation
        || retryPlanningTransfers.current.get(attachmentId) !== retryPlanningTransferId
      ) {
        await cancelUnconsumedComposerAttachmentPlans(plannedRows);
        return;
      }
      if (
        !composerAttachmentPlansMatchDraft(
          plannedRows,
          [attachmentId],
          generation,
          options.routePlanningRevision,
        )
      ) {
        await cancelUnconsumedComposerAttachmentPlans(plannedRows);
        throw new Error("Attachment retry planning returned a mismatched draft or route identity.");
      }
      const planned = plannedRows[0];
      if (!planned) {
        // composerAttachmentPlansMatchDraft already rejects an empty response; this guard keeps the
        // narrowed row explicit for TypeScript without adding a fallback execution path.
        // composerAttachmentPlansMatchDraft 已拒绝空响应；本守卫只为 TypeScript 显式收窄记录，
        // 不增加任何兜底执行路径。
        throw new Error("Attachment retry planning returned no plan.");
      }
      retryPlanningTransfers.current.delete(attachmentId);
      commitPreparations((current) => ({ ...current, [attachmentId]: planned }));
      if (planned.deliveryPlan === "upload_reference") {
        await executePreparation(generation, attachment, planned);
      }
    } catch (error) {
      if (
        planningGeneration.current !== generation
        || retryPlanningTransfers.current.get(attachmentId) !== retryPlanningTransferId
      ) {
        return;
      }
      retryPlanningTransfers.current.delete(attachmentId);
      // failureText explains why the retry degraded while retaining the canonical attachment.
      // failureText 说明重试为何降级，同时保留规范附件。
      const failureText = error instanceof Error
        ? error.message
        : "Attachment upload retry failed; the attachment will be disclosed as metadata only.";
      commitPreparations((current) => ({
        ...current,
        [attachmentId]: {
          ...preparation,
          transferId: retryPlanningTransferId,
          phase: "failed",
          blocksSubmission: false,
          reason: "draft_upload_retry_failed_metadata_only",
          error: failureText,
        },
      }));
    }
  }, [
    attachmentById,
    canonicalAttachments,
    commitPreparations,
    enqueueDraftLeaseReconciliation,
    executePreparation,
    leaseError,
    modelSelectionSnapshot,
    options.conversationId,
    options.projectId,
    options.routePlanningRevision,
    options.routeSelectionPending,
  ]);

  // cancel stops the exact active transfer and closes its UI state without removing canonical attachment data.
  // cancel 停止精确活跃传输并封闭其界面状态，但不移除规范附件数据。
  const cancel = React.useCallback(async (attachmentId: string) => {
    const preparation = preparationsRef.current[attachmentId];
    if (!preparation) {
      return;
    }
    retryPlanningTransfers.current.delete(attachmentId);
    // transferId is captured before publishing cancellation so no later retry can be cancelled accidentally.
    // transferId 在发布取消前捕获，避免意外取消后续重试。
    const transferId = preparation.transferId;
    // cancelledTransferId revokes the active transfer identity before the asynchronous cancellation reaches the provider.
    // cancelledTransferId 在异步取消到达供应商前撤销活跃传输身份。
    const cancelledTransferId = `draft-media-cancelled:${crypto.randomUUID()}`;
    commitPreparations((current) => {
      const active = current[attachmentId];
      if (!active || active.transferId !== transferId) {
        return current;
      }
      return {
        ...current,
        [attachmentId]: composerAttachmentCancelledState(active, cancelledTransferId),
      };
    });
    if (transferId.startsWith("draft-media-retry-planning:")) {
      return;
    }
    // cancellation begins immediately while leaseQueue prevents the managed file from being released early.
    // cancellation 立即开始，同时 leaseQueue 防止托管文件被提前释放。
    const cancellation = cancelConversationSubmission(transferId);
    leaseQueue.current = queueComposerDraftLeaseAfterCancellation(leaseQueue.current, cancellation);
    try {
      await cancellation;
    } catch {
      // The revoked transfer identity keeps any late completion non-authoritative after local cancellation failure.
      // 已撤销的传输身份保证本地取消失败后的迟到完成结果仍无权威性。
    }
  }, [commitPreparations]);

  // reconcileDraft publishes one explicit local set replacement, including an empty set after user removal.
  // reconcileDraft 发布一次显式本地集合替换，包括用户移除后的空集合。
  const reconcileDraft = React.useCallback(async (attachments: ComposerAttachmentSummary[]) => {
    await enqueueDraftLeaseReconciliation(attachments);
  }, [enqueueDraftLeaseReconciliation]);

  // projectForSubmission keeps canonical draft state untouched while freezing current delivery outcomes.
  // projectForSubmission 在冻结当前传输结果的同时保持规范草稿状态不变。
  const projectForSubmission = React.useCallback((attachments: ComposerAttachmentSummary[]) => {
    return attachments.map((attachment) => {
      const attachmentId = attachmentIdentity(attachment);
      const preparation = attachmentId ? preparationsRef.current[attachmentId] : undefined;
      if (!preparation) {
        return attachment;
      }
      const deliveryPlan = preparation.phase === "failed" || preparation.phase === "cancelled"
        ? "metadata_only"
        : preparation.deliveryPlan;
      return { ...attachment, deliveryPlan };
    });
  }, []);

  // visiblePreparations projects one retryable lease failure onto every affected attachment tile.
  // visiblePreparations 将一条可重试租约失败投影到每个受影响附件卡片。
  const visiblePreparations = React.useMemo(() => {
    if (!leaseError) {
      return preparations;
    }
    return Object.fromEntries(Array.from(attachmentById.keys()).map((attachmentId) => [
      attachmentId,
      {
        attachmentId,
        generation: planningGeneration.current,
        transferId: `draft-lease:${attachmentId}`,
        deliveryPlan: "upload_reference" as const,
        phase: "failed" as const,
        blocksSubmission: true,
        routeRevision: null,
        reason: "draft_lease_persistence_failed",
        error: leaseError,
      },
    ]));
  }, [attachmentById, leaseError, preparations]);
  const blocksSubmission = planningPending
    || leaseError !== null
    || Object.values(preparations).some((preparation) => preparation.blocksSubmission);
  const blockingCount = planningPending
    ? canonicalAttachments.length
    : leaseError
      ? canonicalAttachments.length
      : Object.values(preparations).filter((preparation) => preparation.blocksSubmission).length;
  const metadataOnlyCount = Object.values(preparations).filter((preparation) => (
    preparation.reason !== "draft_lease_persistence_failed"
    && (preparation.deliveryPlan === "metadata_only"
      || preparation.phase === "failed"
      || preparation.phase === "cancelled")
  )).length;
  return {
    preparations: visiblePreparations,
    progresses,
    blocksSubmission,
    blockingCount,
    metadataOnlyCount,
    projectForSubmission,
    retry,
    cancel,
    reconcileDraft,
  };
}

// canonicalDraftLeaseSignature returns a collision-safe serialized identity for one exact draft owner set.
// canonicalDraftLeaseSignature 返回单个精确草稿所有者集合的防碰撞序列化身份。
// Parameters: slot/workspace identify ownership and attachments contain canonical service facts.
// 参数：slot/workspace 标识所有权，attachments 包含规范服务事实。
// Returns: one deterministic JSON signature.
// 返回值：一条确定性 JSON 签名。
function canonicalDraftLeaseSignature(
  draftSlotKey: string,
  projectId: string | null,
  conversationId: string | null,
  attachments: readonly ComposerAttachmentSummary[],
): string {
  if (!attachments.length) {
    return "";
  }
  return JSON.stringify([
    draftSlotKey,
    projectId,
    conversationId,
    attachments.map((attachment) => [
      attachmentIdentity(attachment),
      attachment.byteLen ?? null,
      attachment.sourceSha256 ?? null,
    ]),
  ]);
}
