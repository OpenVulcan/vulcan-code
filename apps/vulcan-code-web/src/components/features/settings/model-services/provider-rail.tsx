import * as React from "react"
import { ChevronDown, ChevronUp, GripVertical, Plus, Search } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { localizedProviderName } from "@/components/features/settings/provider-presentation"
import { PageEmptyState } from "@/components/page"
import { Button } from "@/components/ui/button"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/components/ui/hover-card"
import { Input } from "@/components/ui/input"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { useI18n } from "@/i18n"
import { cn } from "@/lib/utils"
import type { PublicAiProviderDescriptor } from "@/types"

import {
  moveItem,
  PROVIDER_SERVICE_TAGS,
  providerMatchesFilter,
  type ProviderDirectory,
  type ProviderTagFilter,
} from "./provider-directory"
import { ProviderIntroCard, serviceTagLabel } from "./provider-intro"

// ProviderRailProps feeds the directory and reports selection, filters and reorders.
// ProviderRailProps 提供目录，并回报选择、筛选与重新排序。
export interface ProviderRailProps {
  // directory is null while the snapshot loads.
  // directory 在快照加载期间为 null。
  directory: ProviderDirectory | null
  // accountCounts maps provider id to its account count.
  // accountCounts 将供应商标识映射到其账号数量。
  accountCounts: Readonly<Record<string, number>>
  // attention marks providers with an alert.
  // attention 标记有提醒的供应商。
  attention: ReadonlySet<string>
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
    const count = props.accountCounts[provider.id] ?? 0
    return (
      <li
        className={cn("model-services-rail-item", dragId === provider.id && "dragging")}
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
        {reorderable ? <GripVertical aria-hidden="true" className="model-services-rail-grip" /> : null}
        <RailRowButton
          active={props.selectedId === provider.id}
          attentionLabel={props.attention.has(provider.id) ? t("settings.modelServices.alerts.title") : null}
          count={count}
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
        />
        {reorderable ? (
          <span className="model-services-rail-move">
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
    <nav aria-label={t("settings.modelServices.rail.title")} className="model-services-rail">
      <div className="model-services-rail-search">
        <Search aria-hidden="true" size={14} />
        <Input
          aria-label={t("settings.modelServices.rail.search")}
          onChange={(event) => props.onQuery(event.target.value)}
          placeholder={t("settings.modelServices.rail.search")}
          value={props.query}
        />
      </div>
      <div className="model-services-tag-chips" role="group" aria-label={t("settings.modelServices.rail.tagFilter")}>
        {(["all", ...PROVIDER_SERVICE_TAGS] as const).map((tag) => (
          <button
            aria-pressed={props.tag === tag}
            className="model-services-tag-chip"
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
        <div className="model-services-rail-sections">
          {configured.length > 0 ? (
            <section>
              <p className="model-services-rail-heading" title={t("settings.modelServices.rail.reorderHint")}>{t("settings.modelServices.rail.configured")}</p>
              <ul>{configured.map((provider) => row(provider, true))}</ul>
            </section>
          ) : null}
          {available.length > 0 ? (
            <section>
              <p className="model-services-rail-heading">{t("settings.modelServices.rail.available")}</p>
              <ul>{available.map((provider) => row(provider, false))}</ul>
            </section>
          ) : null}
          {configured.length === 0 && available.length === 0 ? (
            <PageEmptyState icon={<Search />} title={t("settings.modelExperience.noProviders")} />
          ) : null}
        </div>
      )}
      <Button className="model-services-rail-add" onClick={props.onAddCustom} size="sm" type="button" variant="outline">
        <Plus aria-hidden="true" />
        {t("settings.modelServices.rail.addCustom")}
      </Button>
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
  count: number
  active: boolean
  attentionLabel: string | null
  onSelect: () => void
  onKeyDown: (event: React.KeyboardEvent<HTMLButtonElement>) => void
}) {
  const [open, setOpen] = React.useState(false)
  // pressed suppresses the focus-driven reopen that a pointer click would otherwise schedule until the pointer leaves.
  // pressed 抑制指针点击引发的聚焦重新打开，直到指针离开该行；键盘聚焦仍会显示介绍。
  const pressed = React.useRef(false)
  return (
    <HoverCard closeDelay={80} onOpenChange={(next) => { if (!next || !pressed.current) setOpen(next) }} open={open} openDelay={450}>
      <HoverCardTrigger asChild>
        <button
          aria-current={props.active ? "page" : undefined}
          className={cn("model-services-rail-row", props.active && "active")}
          onClick={props.onSelect}
          onKeyDown={props.onKeyDown}
          onPointerDown={() => {
            pressed.current = true
            setOpen(false)
          }}
          onPointerLeave={() => { pressed.current = false }}
          type="button"
        >
          <ProviderIcon category={props.provider.category} className="size-5 shrink-0" providerId={props.provider.id} />
          <span className="model-services-rail-name" title={props.name}>{props.name}</span>
          {props.attentionLabel ? <span aria-label={props.attentionLabel} className="model-services-rail-alert" role="img" /> : null}
          {props.count > 0 ? <small>{props.count}</small> : null}
        </button>
      </HoverCardTrigger>
      <HoverCardContent align="start" side="right" sideOffset={10} width="lg">
        <ProviderIntroCard provider={props.provider} />
      </HoverCardContent>
    </HoverCard>
  )
}
