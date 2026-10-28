import * as React from "react";
import { Search } from "lucide-react";
import { PageNotice } from "@/components/page";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { formatMessage, useI18n } from "@/i18n";
import { searchLuaSkillsHub } from "@/service/skills-actions-service";
import type { LuaSkillsHubSkill } from "@/types";
import { LuaSkillHubDetailSheet } from "./lua-skill-hub-detail-sheet";

// HUB_SEARCH_DEBOUNCE_MS delays a search until typing pauses.
// HUB_SEARCH_DEBOUNCE_MS 在输入停顿后才发起搜索。
const HUB_SEARCH_DEBOUNCE_MS = 300;

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
  // query is the search text typed by the user.
  // query 是用户输入的搜索文本。
  const [query, setQuery] = React.useState("");
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

  // A query or interface-language change starts the search over.
  // 查询或界面语言变化时重新开始搜索。
  React.useEffect(() => {
    const timer = window.setTimeout(() => void load(query, null), HUB_SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [load, query]);

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
          />
        </span>
      </label>
      <div className="lua-skill-hub-results" role="listbox" aria-label={t("skills.lua.hubSearch")} aria-busy={loading}>
        {items.map((skill) => {
          const selected = skill.skillId === props.selectedSkillId;
          const summary = skill.content?.summary || skill.summary;
          return (
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
            <Button disabled={props.disabled} size="sm" type="button" variant="outline" onClick={() => void load(query, nextCursor)}>
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
