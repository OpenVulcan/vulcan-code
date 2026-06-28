import * as React from "react"
import {
  closestCenter,
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core"
import {
  arrayMove,
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable"
import {
  SlidersHorizontal,
} from "lucide-react"

import { PanelTitle, StatusPill } from "@/components/shared/status"
import type {
  AiCapabilityTargetSaveInput,
  SettingsActionProps,
} from "@/components/features/settings/settings-action-types"
import { useI18n } from "@/i18n"
import type {
  AiCapabilitySummary,
  AiCapabilityTargetSummary,
  AiCapabilityVendorSummary,
  SettingsSubAreaId,
} from "@/types"

import { AiCapabilityTargetsDialog } from "./ai-capability-target-dialog"
import { SortableAiCapabilityTargetRow } from "./ai-capability-target-row"
import { localizedLiteral } from "./ai-capability-settings-utils"

// AiCapabilitySettingsArea renders one semantic AI capability page.
// AiCapabilitySettingsArea 渲染单个语义 AI 能力页面。
export function AiCapabilitySettingsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Active settings sub area.
  // 当前设置子页面。
  activeArea: SettingsSubAreaId
  // Backend capability kind resolved by the parent settings registry.
  // 父级设置注册表解析出的后端能力类型。
  capabilityKind: string
  // Localized active page title.
  // 本地化当前页面标题。
  headerTitle: string
  // Localized active page description.
  // 本地化当前页面说明。
  headerDescription: string
  // Semantic AI capability summaries.
  // 语义 AI 能力摘要。
  capabilities: AiCapabilitySummary[]
  // Whether the target configuration dialog is open.
  // 目标配置弹窗是否打开。
  configDialogOpen: boolean
  // Updates the target configuration dialog open state.
  // 更新目标配置弹窗打开状态。
  onConfigDialogOpenChange: (open: boolean) => void
  // Configured AI capability vendors with target choices.
  // 带目标选项的已配置 AI 能力供应商。
  vendors: AiCapabilityVendorSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  // displayedTargets keeps drag sorting optimistic while the service snapshot catches up.
  // displayedTargets 在服务快照追上前保持拖拽排序的乐观展示顺序。
  const [displayedTargets, setDisplayedTargets] = React.useState<AiCapabilityTargetSummary[]>([])
  // pendingRemoveSelector stores the route waiting for a second remove click.
  // pendingRemoveSelector 保存等待第二次点击确认移除的路由。
  const [pendingRemoveSelector, setPendingRemoveSelector] = React.useState<string | null>(null)
  const capability = props.capabilities.find((entry) => entry.kind === props.capabilityKind)
  const targetDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }))
  const snapshotTargets = capability?.targets ?? []
  const snapshotTargetSignature = React.useMemo(() => aiCapabilityTargetSummarySignature(snapshotTargets), [snapshotTargets])
  React.useEffect(() => {
    setDisplayedTargets(snapshotTargets)
    setPendingRemoveSelector(null)
  }, [capability?.kind, snapshotTargetSignature])
  const saveTargets = React.useCallback(async (targets: AiCapabilityTargetSaveInput[]) => {
    if (!capability) {
      return
    }
    await props.actions.onSaveAiCapabilityTargets({
      capabilityKind: capability.kind,
      targets,
    })
  }, [capability, props.actions])
  const targetInputs = React.useMemo(() => (
    displayedTargets.map((target) => aiCapabilityTargetInputFromSummary(target))
  ), [displayedTargets])
  const targetSortableIds = React.useMemo(() => (
    targetInputs.map((target) => aiCapabilityTargetInputSelector(target))
  ), [targetInputs])
  const handleTargetDragEnd = React.useCallback((event: DragEndEvent) => {
    const activeId = String(event.active.id)
    const overId = event.over ? String(event.over.id) : null
    if (!overId || activeId === overId) {
      return
    }
    const oldIndex = targetSortableIds.indexOf(activeId)
    const newIndex = targetSortableIds.indexOf(overId)
    if (oldIndex < 0 || newIndex < 0) {
      return
    }
    const nextDisplayedTargets = arrayMove(displayedTargets, oldIndex, newIndex)
    const nextTargetInputs = arrayMove(targetInputs, oldIndex, newIndex)
    setDisplayedTargets(nextDisplayedTargets)
    void saveTargets(nextTargetInputs).catch(() => setDisplayedTargets(snapshotTargets))
  }, [displayedTargets, saveTargets, snapshotTargets, targetInputs, targetSortableIds])
  if (!capability) {
    return (
      <div className="settings-direct-content">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title={props.headerTitle || text("AI 能力", "AI capability")} />
        <article className="settings-card">
          <StatusPill status="warning" label="reserved" />
          <strong>{props.headerTitle || props.activeArea}</strong>
          <p>{props.headerDescription || text("该能力入口已预留，当前 snapshot 没有可用目标。", "This capability entry is reserved and has no available target in the current snapshot.")}</p>
        </article>
      </div>
    )
  }
  return (
    <div className="settings-direct-content settings-capability-page">
      {displayedTargets.length === 0 ? (
        <div className="composer-capability-dialog-empty settings-capability-empty">
          {text("该能力还没有配置目标。", "This capability has no configured targets.")}
        </div>
      ) : (
        <DndContext sensors={targetDragSensors} collisionDetection={closestCenter} onDragEnd={handleTargetDragEnd}>
          <SortableContext items={targetSortableIds} strategy={verticalListSortingStrategy}>
            <div className="settings-capability-route-list">
              {displayedTargets.map((target, index) => (
                <SortableAiCapabilityTargetRow
                  confirmRemove={pendingRemoveSelector === target.targetSelector}
                  index={index}
                  key={target.targetSelector}
                  onCancelRemove={() => setPendingRemoveSelector(null)}
                  onRemove={() => {
                    const nextTargets = targetInputs.filter((item) => !(item.vendorId === target.vendorId && item.targetAlias === target.targetAlias))
                    setPendingRemoveSelector(null)
                    setDisplayedTargets((current) => current.filter((item) => item.targetSelector !== target.targetSelector))
                    void saveTargets(nextTargets).catch(() => setDisplayedTargets(snapshotTargets))
                  }}
                  onRequestRemove={() => setPendingRemoveSelector(target.targetSelector)}
                  onToggle={(enabled) => {
                    setPendingRemoveSelector(null)
                    const nextTargets = targetInputs.map((item) => (
                      item.vendorId === target.vendorId && item.targetAlias === target.targetAlias
                        ? { ...item, enabled }
                        : item
                    ))
                    setDisplayedTargets((current) => current.map((item) => (
                      item.targetSelector === target.targetSelector ? { ...item, enabled } : item
                    )))
                    void saveTargets(nextTargets).catch(() => setDisplayedTargets(snapshotTargets))
                  }}
                  target={target}
                  text={text}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      )}
      <AiCapabilityTargetsDialog
        capability={capability}
        onOpenChange={props.onConfigDialogOpenChange}
        onSave={saveTargets}
        open={props.configDialogOpen}
        vendors={props.vendors}
      />
    </div>
  )
}

// aiCapabilityTargetInputFromSummary converts one rendered summary into a save input.
// aiCapabilityTargetInputFromSummary 将渲染摘要转换为保存输入。
function aiCapabilityTargetInputFromSummary(target: AiCapabilityTargetSummary): AiCapabilityTargetSaveInput {
  return {
    vendorId: target.vendorId,
    targetAlias: target.targetAlias,
    modelId: target.modelId,
    enabled: target.enabled,
  }
}

// aiCapabilityTargetInputSelector builds the same stable selector used by rendered summaries.
// aiCapabilityTargetInputSelector 构建与渲染摘要一致的稳定选择器。
function aiCapabilityTargetInputSelector(input: AiCapabilityTargetSaveInput): string {
  return `${input.vendorId}/${input.targetAlias}`
}

// aiCapabilityTargetSummarySignature tracks meaningful snapshot changes without depending on array identity.
// aiCapabilityTargetSummarySignature 跟踪快照的关键变化，避免依赖数组引用身份。
function aiCapabilityTargetSummarySignature(targets: AiCapabilityTargetSummary[]): string {
  return targets.map((target) => [
    target.targetSelector,
    target.vendorId,
    target.targetAlias,
    target.modelId,
    target.enabled ? "1" : "0",
  ].join(":")).join("|")
}
