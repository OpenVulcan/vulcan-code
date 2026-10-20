import * as React from "react"
import { ChevronDown, ChevronUp, GripVertical, Plus, Search } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { localizedProviderName } from "@/components/features/settings/provider-presentation"
import { PageEmptyState } from "@/components/page"
import { Button } from "@/ui-kit/primitives/button"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/ui-kit/primitives/hover-card"
import { Input } from "@/ui-kit/primitives/input"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import { useI18n } from "@/i18n"
import { cx } from "@/ui-kit/behavior/core";
import type { PublicAiProviderDescriptor } from "@/types"

import type { HealthState } from "./provider-alerts"
import {
  moveItem,
  PROVIDER_SERVICE_TAGS,
  providerMatchesFilter,
  type ProviderDirectory,
  type ProviderTagFilter,
} from "./provider-directory"
import { AffiliateChip, providerCategoryLabel, ProviderIntroCard, ProviderTagChips, serviceTagLabel } from "./provider-intro"

// ProviderRailProps feeds the directory and reports selection, filters and reorders.
// ProviderRailProps 提供目录，并回报选择、筛选与重新排序。
export interface ProviderRailProps {
  // directory is null while the snapshot loads.
  // directory 在快照加载期间为 null。
  directory: ProviderDirectory | null
  // accountCounts maps provider id to its account count; modelCounts to its models enabled in conversations.
  // accountCounts 将供应商标识映射到账号数量；modelCounts 映射到会话中启用的模型数量。
  accountCounts: Readonly<Record<string, number>>
  modelCounts: Readonly<Record<string, number>>
  // health is the folded account state of each configured provider.
  // health 是每个已配置供应商合并后的账号状态。
  health: Readonly<Record<string, HealthState>>
  selectedId: string | null
  onSelect: (providerId: string) => void
  tag: ProviderTagFilter
  onTag: (tag: ProviderTagFilter) => void
  query: string
  onQuery: (query: string) => void
  // reorderDisabled blocks reordering while a write runs.
  // reorderDisabled 在写入进行时阻止重新排序。
  reorderDisabled: boolean
  // onReorder receives the complete new configured order.
  // onReorder 接收完整的新已配置顺序。
  onReorder: (order: string[]) => void
  onAddCustom: () => void
}

// ProviderRail is the left directory: configured providers first (user order), then recommendations.
// ProviderRail 是左侧目录：先显示已配置供应商（用户顺序），再显示推荐供应商。
export function ProviderRail(props: ProviderRailProps) {
  const { t } = useI18n()
  const [dragId, setDragId] = React.useState<string | null>(null)
  const name = (provider: PublicAiProviderDescriptor) => localizedProviderName(provider.id, provider.display_name, t)
  const matches = (provider: PublicAiProviderDescriptor) => providerMatchesFilter(provider, name(provider), props.tag, props.query)
  const configured = props.directory?.configured.filter(matches) ?? []
  const available = props.directory?.available.filter(matches) ?? []
  const configuredIds = props.directory?.configured.map((provider) => provider.id) ?? []

  // moveRelative moves one provider before or after a visible neighbour within the full order.
  // moveRelative 在完整顺序中将供应商移动到可见相邻项之前或之后。
  const moveRelative = (providerId: string, targetId: string) => {
    const from = configuredIds.indexOf(providerId)
    const to = configuredIds.indexOf(targetId)
    if (from < 0 || to < 0 || from === to) return
    props.onReorder(moveItem(configuredIds, from, to))
  }
  const neighbour = (providerId: string, offset: -1 | 1) => {
    const index = configured.findIndex((provider) => provider.id === providerId)
    return configured[index + offset]?.id
  }

  const row = (provider: PublicAiProviderDescriptor, reorderable: boolean) => {
    const above = reorderable ? neighbour(provider.id, -1) : undefined
    const below = reorderable ? neighbour(provider.id, 1) : undefined
    return (
      <li
        className={cx("vk-ms-rail-item", dragId === provider.id && "dragging")}
        draggable={reorderable && !props.reorderDisabled}
        key={provider.id}
        onDragEnd={() => setDragId(null)}
        onDragOver={(event) => {
          if (reorderable && dragId && dragId !== provider.id) event.preventDefault()
        }}
        onDragStart={(event) => {
          event.dataTransfer.effectAllowed = "move"
          setDragId(provider.id)
        }}
        onDrop={(event) => {
          event.preventDefault()
          if (dragId) moveRelative(dragId, provider.id)
          setDragId(null)
        }}
      >
        <GripVertical aria-hidden="true" className="vk-ms-rail-grip" data-hidden={reorderable ? undefined : "true"} />
        <RailRowButton
          active={props.selectedId === provider.id}
          configured={reorderable}
          name={name(provider)}
          onKeyDown={(event) => {
            if (!reorderable || props.reorderDisabled || !event.altKey) return
            const target = event.key === "ArrowUp" ? above : event.key === "ArrowDown" ? below : undefined
            if (!target) return
            event.preventDefault()
            moveRelative(provider.id, target)
          }}
          onSelect={() => props.onSelect(provider.id)}
          provider={provider}
        >
          {reorderable ? (
            <span className="vk-ms-rail-meta">
              <span><i aria-hidden="true" className="vk-ms-dot" data-state={props.health[provider.id] ?? "ok"} />
                {t("settings.modelServices.rail.accounts").replace("{count}", String(props.accountCounts[provider.id] ?? 0))}</span>
              {props.modelCounts[provider.id] ? (
                <span>{t("settings.modelServices.rail.modelsInUse").replace("{count}", String(props.modelCounts[provider.id]))}</span>
              ) : null}
            </span>
          ) : null}
        </RailRowButton>
        {reorderable ? (
          <span className="vk-ms-rail-move">
            <Button
              aria-label={t("settings.modelServices.rail.moveUp").replace("{provider}", name(provider))}
              disabled={!above || props.reorderDisabled}
              onClick={() => above && moveRelative(provider.id, above)}
              size="icon-xs"
              type="button"
              variant="ghost"
            ><ChevronUp /></Button>
            <Button
              aria-label={t("settings.modelServices.rail.moveDown").replace("{provider}", name(provider))}
              disabled={!below || props.reorderDisabled}
              onClick={() => below && moveRelative(provider.id, below)}
              size="icon-xs"
              type="button"
              variant="ghost"
            ><ChevronDown /></Button>
          </span>
        ) : null}
      </li>
    )
  }

  return (
    <nav aria-label={t("settings.modelServices.rail.title")} className="vk-ms-rail">
      <div className="vk-ms-rail-search">
        <Search aria-hidden="true" size={14} />
        <Input
          aria-label={t("settings.modelServices.rail.search")}
          onChange={(event) => props.onQuery(event.target.value)}
          placeholder={t("settings.modelServices.rail.search")}
          value={props.query}
        />
      </div>
      <div className="vk-ms-tag-chips" role="group" aria-label={t("settings.modelServices.rail.tagFilter")}>
        {(["all", ...PROVIDER_SERVICE_TAGS] as const).map((tag) => (
          <button
            aria-pressed={props.tag === tag}
            className="vk-ms-tag-chip"
            data-tag={tag}
            key={tag}
            onClick={() => props.onTag(tag)}
            type="button"
          >
            {tag === "all" ? t("settings.modelServices.tag.all") : serviceTagLabel(tag, t)}
          </button>
        ))}
      </div>
      {!props.directory ? <StreamPlaceholder decorative shape="rows" count={6} /> : (
        <div className="vk-ms-rail-sections">
          {configured.length > 0 ? (
            <section>
              <p className="vk-ms-rail-heading" title={t("settings.modelServices.rail.reorderHint")}>
                <span>{t("settings.modelServices.rail.configured")} · {configured.length}</span>
                <small>{t("settings.modelServices.rail.dragHint")}</small>
              </p>
              <ul>{configured.map((provider) => row(provider, true))}</ul>
            </section>
          ) : null}
          {available.length > 0 ? (
            <section>
              <p className="vk-ms-rail-heading">
                <span>{t("settings.modelServices.rail.available")} · {available.length}</span>
                <small>{t("settings.modelServices.rail.recommended")}</small>
              </p>
              <ul>{available.map((provider) => row(provider, false))}</ul>
            </section>
          ) : null}
          {configured.length === 0 && available.length === 0 ? (
            <PageEmptyState icon={<Search />} title={t("settings.modelExperience.noProviders")} />
          ) : null}
        </div>
      )}
      <div className="vk-ms-rail-foot">
        <Button onClick={props.onAddCustom} size="sm" type="button" variant="outline">
          <Plus aria-hidden="true" />
          {t("settings.modelServices.rail.addCustom")}
        </Button>
      </div>
    </nav>
  )
}

// RailRowButton is one directory row whose hover introduction closes as soon as the row is chosen.
// The card is portaled and controlled, so opening it never re-renders sibling rows or delays the click.
// RailRowButton 是一行目录项；选中该行后悬停介绍立即关闭。
// 介绍卡通过门户渲染且受控，打开时不会重新渲染相邻行，也不会延迟点击。
function RailRowButton(props: {
  provider: PublicAiProviderDescriptor
  name: string
  active: boolean
  configured: boolean
  children?: React.ReactNode
  onSelect: () => void
  onKeyDown: (event: React.KeyboardEvent<HTMLButtonElement>) => void
}) {
  const { t } = useI18n()
  const [open, setOpen] = React.useState(false)
  // pressed suppresses the focus-driven reopen that a pointer click would otherwise schedule until the pointer leaves.
  // pressed 抑制指针点击引发的聚焦重新打开，直到指针离开该行；键盘聚焦仍会显示介绍。
  const pressed = React.useRef(false)
  const category = providerCategoryLabel(props.provider.category, t)
  const select = () => {
    setOpen(false)
    props.onSelect()
  }
  return (
    <HoverCard onOpenChange={(next) => { if (!next || !pressed.current) setOpen(next) }} open={open}>
      <HoverCardTrigger asChild>
        <button
          aria-current={props.active ? "page" : undefined}
          className={cx("vk-ms-rail-row", props.active && "active")}
          onClick={props.onSelect}
          onKeyDown={props.onKeyDown}
          onPointerDown={() => {
            pressed.current = true
            setOpen(false)
          }}
          onPointerLeave={() => { pressed.current = false }}
          type="button"
        >
          <ProviderIcon category={props.provider.category} size={28} providerId={props.provider.id} />
          <span className="vk-ms-rail-copy">
            <span className="vk-ms-rail-name" title={props.name}>{props.name}</span>
            {/* Configured rows show health and counts on their second line; their service tags stay in the header and filter. */}
            {/* 已配置行第二行显示健康状态与数量；其服务标签保留在详情标题与筛选中。 */}
            {props.configured ? null : <span className="vk-ms-rail-sub">
              <ProviderTagChips provider={props.provider} small />
              {category ? <span className="vk-ms-chip" data-variant="category">{category}</span> : null}
              <AffiliateChip provider={props.provider} />
            </span>}
          </span>
          {props.children}
        </button>
      </HoverCardTrigger>
      <HoverCardContent align="start" side="right" sideOffset={12} width="lg">
        <ProviderIntroCard
          action={(
            <Button onClick={select} size="xs" type="button" variant={props.configured ? "outline" : "default"}>
              {t(props.configured ? "settings.modelServices.intro.view" : "settings.modelServices.intro.connect")}
            </Button>
          )}
          provider={props.provider}
        />
      </HoverCardContent>
    </HoverCard>
  )
}
