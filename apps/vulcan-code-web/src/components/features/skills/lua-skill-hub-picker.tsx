import * as React from "react";
import { Search } from "lucide-react";
import { PageNotice } from "@/components/page";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip";
import { formatMessage, useI18n } from "@/i18n";
import { searchLuaSkillsHub } from "@/service/skills-actions-service";
import type { LuaSkillsHubSkill } from "@/types";
import { LuaSkillHubDetailSheet } from "./lua-skill-hub-detail-sheet";

// LuaSkillHubPicker searches the configured LuaSkills Hub and selects one skill to install.
// LuaSkillHubPicker 搜索已配置的 LuaSkills Hub 并选择一个待安装技能。
export function LuaSkillHubPicker(props: {
  // Selected skill identifier, empty when nothing is selected.
  // 已选技能标识，未选择时为空。
  selectedSkillId: string;
  // Reports the selected skill identifier.
  // 回报已选技能标识。
  onSelect: (skillId: string) => void;
  // Whether an install request is running.
  // 安装请求是否正在执行。
  disabled: boolean;
}) {
  const { t, locale } = useI18n();
  // query is the search text typed by the user; typing alone never sends a request.
  // query 是用户输入的搜索文本；仅输入不会发出请求。
  const [query, setQuery] = React.useState("");
  // submittedRef holds the query last submitted with Enter, which "Load more" and a language change continue.
  // submittedRef 保存最近一次按 Enter 提交的查询，供“加载更多”与语言切换沿用。
  const submittedRef = React.useRef("");
  // items accumulates every loaded page for the current query.
  // items 累积当前查询已载入的全部页面。
  const [items, setItems] = React.useState<LuaSkillsHubSkill[]>([]);
  // nextCursor continues the current query, null on the last page.
  // nextCursor 用于继续当前查询，最后一页为 null。
  const [nextCursor, setNextCursor] = React.useState<string | null>(null);
  // hubBaseUrl names the Hub that answered, empty before the first answer.
  // hubBaseUrl 标明已响应的 Hub，首次响应前为空。
  const [hubBaseUrl, setHubBaseUrl] = React.useState("");
  const [loading, setLoading] = React.useState(false);
  const [searchError, setSearchError] = React.useState("");
  // detailOpen shows the selected skill's detail drawer.
  // detailOpen 显示已选技能的详情抽屉。
  const [detailOpen, setDetailOpen] = React.useState(false);
  // requestRef identifies the newest request so late answers never overwrite newer results.
  // requestRef 标识最新请求，确保迟到响应不会覆盖较新结果。
  const requestRef = React.useRef(0);

  // load fetches one page in the interface language; a null cursor starts the query over.
  // load 以界面语言拉取一页；null 游标表示重新开始查询。
  const load = React.useCallback(async (text: string, cursor: string | null) => {
    const request = ++requestRef.current;
    setLoading(true);
    setSearchError("");
    try {
      const page = await searchLuaSkillsHub(text, cursor, locale);
      if (request !== requestRef.current) return;
      setHubBaseUrl(page.hubBaseUrl);
      setItems((current) => (cursor === null ? page.items : [...current, ...page.items]));
      setNextCursor(page.nextCursor);
    } catch (error) {
      if (request !== requestRef.current) return;
      if (cursor === null) {
        setItems([]);
        setNextCursor(null);
      }
      setSearchError(error instanceof Error ? error.message : String(error));
    } finally {
      if (request === requestRef.current) setLoading(false);
    }
  }, [locale]);

  // Opening the picker browses the first page once; an interface-language change reloads the submitted query.
  // The request waits one task so a StrictMode rehearsal mount does not spend a rate-limited search.
  // 打开选择器时浏览一次首页；界面语言变化时重新载入已提交的查询。
  // 请求推迟一个任务，避免 StrictMode 演练挂载消耗受限流的搜索次数。
  React.useEffect(() => {
    const timer = window.setTimeout(() => void load(submittedRef.current, null), 0);
    return () => window.clearTimeout(timer);
  }, [load]);

  // submitSearch starts the typed query over; the Hub rate-limits searches, so only Enter sends one.
  // submitSearch 重新开始输入的查询；Hub 对搜索限流，因此只有 Enter 才会发出请求。
  const submitSearch = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    // The picker sits inside the install form; Enter must search, never submit the install.
    // 选择器位于安装表单内；Enter 只能搜索，绝不能提交安装。
    event.preventDefault();
    submittedRef.current = query;
    void load(query, null);
  };

  // Invalidate any in-flight answer when the picker unmounts.
  // 选择器卸载时作废任何进行中的响应。
  React.useEffect(() => () => { requestRef.current += 1; }, []);

  return (
    <div className="lua-skill-hub-picker">
      <label className="lua-skill-install-field">
        <span>{t("skills.lua.hubSearch")}</span>
        <span className="lua-skill-hub-search">
          <Search size={14} aria-hidden="true" />
          <input
            autoFocus
            disabled={props.disabled}
            placeholder={t("skills.lua.hubSearchPlaceholder")}
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={submitSearch}
          />
        </span>
      </label>
      <div className="lua-skill-hub-results" role="listbox" aria-label={t("skills.lua.hubSearch")} aria-busy={loading}>
        {items.map((skill) => {
          const selected = skill.skillId === props.selectedSkillId;
          const summary = skill.content?.summary || skill.summary;
          const intro = skill.content?.intro.trim() ?? "";
          const row = (
            <button
              key={skill.skillId}
              aria-selected={selected}
              className="lua-skill-hub-result"
              disabled={props.disabled}
              role="option"
              type="button"
              onClick={() => props.onSelect(selected ? "" : skill.skillId)}
            >
              <span className="lua-skill-hub-result-title">
                <strong>{skill.displayName || skill.skillId}</strong>
                {skill.latestVersion ? <code>{skill.latestVersion}</code> : null}
                {skill.content?.kind === "translation" ? (
                  <Badge variant="outline" title={t("skills.lua.hubTranslatedTitle")}>{t("skills.lua.hubTranslatedBadge")}</Badge>
                ) : null}
              </span>
              <span className="lua-skill-hub-result-meta">
                {[skill.skillId, skill.publisher, skill.license].filter(Boolean).join(" · ")}
              </span>
              {summary ? <span className="lua-skill-hub-result-summary">{summary}</span> : null}
            </button>
          );
          // The AI-refined introduction shows on hover or keyboard focus when the Hub provides one.
          // Hub 提供 AI 精炼介绍时，在悬停或键盘聚焦时显示。
          if (!intro) return row;
          return (
            <Tooltip key={skill.skillId}>
              <TooltipTrigger asChild>{row}</TooltipTrigger>
              <TooltipContent side="right">{intro}</TooltipContent>
            </Tooltip>
          );
        })}
        {!loading && !searchError && items.length === 0 ? (
          <p className="lua-skill-hub-status">{t("skills.lua.hubEmpty")}</p>
        ) : null}
        {loading ? <p className="lua-skill-hub-status">{t("skills.lua.hubSearching")}</p> : null}
      </div>
      {searchError ? (
        <PageNotice details={searchError} detailsLabel={t("skills.lua.errorDetails")} tone="error">
          {t("skills.lua.hubSearchFailed")}
        </PageNotice>
      ) : null}
      <div className="lua-skill-hub-footer">
        <span>{hubBaseUrl ? formatMessage(t("skills.lua.hubSourceLabel"), { hub: hubBaseUrl }) : ""}</span>
        <span className="lua-skill-hub-footer-actions">
          {props.selectedSkillId ? (
            <Button size="sm" type="button" variant="outline" onClick={() => setDetailOpen(true)}>
              {t("skills.lua.hubDetails")}
            </Button>
          ) : null}
          {nextCursor && !loading ? (
            <Button disabled={props.disabled} size="sm" type="button" variant="outline" onClick={() => void load(submittedRef.current, nextCursor)}>
              {t("skills.lua.hubLoadMore")}
            </Button>
          ) : null}
        </span>
      </div>
      {props.selectedSkillId ? (
        <LuaSkillHubDetailSheet open={detailOpen} skillId={props.selectedSkillId} onOpenChange={setDetailOpen} />
      ) : null}
    </div>
  );
}
