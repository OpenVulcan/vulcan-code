import React from "react";
import { BrainCircuit, Database } from "lucide-react";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs";
import { useI18n } from "@/i18n";
import type {
  ContextInjectionCardSummary,
  ContextInjectionSnapshot,
} from "@/types";

// WorkspaceMemoryPanel renders iced-aligned dynamic and static context injection lists.
// WorkspaceMemoryPanel 渲染对齐 iced 的动态与静态上下文注入列表。
export function WorkspaceMemoryPanel(props: {
  // Selected-session context injection snapshot from the service.
  // 来自服务的当前选中会话上下文注入快照。
  snapshot: ContextInjectionSnapshot | null;
}) {
  // t translates stable side-panel labels.
  // t 翻译稳定侧面板标签。
  const { t } = useI18n();
  const dynamicCards = props.snapshot?.dynamicCards ?? [];
  const staticCards = props.snapshot?.staticCards ?? [];

  return (
    <Tabs className="context-injection-tabs" defaultValue="dynamic">
      <TabsList className="context-injection-tabs-list">
        <TabsTrigger value="dynamic">{t("contextPanel.dynamic")}</TabsTrigger>
        <TabsTrigger value="static">{t("contextPanel.static")}</TabsTrigger>
      </TabsList>
      <TabsContent className="context-injection-tab-content" value="dynamic">
        <ContextInjectionCardList
          cards={dynamicCards}
          emptyIcon={<BrainCircuit size={20} />}
          emptyText={t("contextPanel.dynamicEmpty")}
          title={t("contextPanel.dynamic")}
        />
      </TabsContent>
      <TabsContent className="context-injection-tab-content" value="static">
        <ContextInjectionCardList
          cards={staticCards}
          emptyIcon={<Database size={20} />}
          emptyText={t("contextPanel.staticEmpty")}
          title={t("contextPanel.static")}
        />
      </TabsContent>
    </Tabs>
  );
}

// ContextInjectionCardList renders one injection card collection or its empty state.
// ContextInjectionCardList 渲染一组注入卡片或其空状态。
function ContextInjectionCardList(props: {
  // Injection cards to display.
  // 需要展示的注入卡片。
  cards: ContextInjectionCardSummary[];
  // Empty state icon.
  // 空状态图标。
  emptyIcon: React.ReactNode;
  // Empty state title.
  // 空状态标题。
  title: string;
  // Empty state message.
  // 空状态消息。
  emptyText: string;
}) {
  if (props.cards.length === 0) {
    return (
      <div className="context-empty-state">
        {props.emptyIcon}
        <strong>{props.title}</strong>
        <span>{props.emptyText}</span>
      </div>
    );
  }

  return (
    <div className="context-injection-card-list">
      {props.cards.map((card) => (
        <ContextInjectionCardView card={card} key={card.tag} />
      ))}
    </div>
  );
}

// ContextInjectionCardView renders one injection source group and its items.
// ContextInjectionCardView 渲染单个注入来源分组及其条目。
function ContextInjectionCardView(props: {
  // Injection source card to render.
  // 要渲染的注入来源卡片。
  card: ContextInjectionCardSummary;
}) {
  return (
    <article className="context-injection-card">
      <header className="context-injection-card-header">
        <span className="context-injection-tag">{props.card.tag}</span>
        <p>{props.card.description}</p>
      </header>
      <div className="context-injection-item-list">
        {props.card.items.map((item, index) => (
          <details className="context-injection-item" key={`${props.card.tag}-${index}`}>
            <summary>
              <span className="context-injection-item-content">{item.content}</span>
              <span className="context-injection-item-meta">
                {item.turnLabel ? <span>{item.turnLabel}</span> : null}
                <span>{item.ttlLabel}</span>
                {item.sourceDisplay ? <span>{item.sourceDisplay}</span> : null}
              </span>
            </summary>
            <pre>{item.detail}</pre>
          </details>
        ))}
      </div>
    </article>
  );
}
