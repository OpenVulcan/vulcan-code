import * as React from "react";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { PageNotice } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/ui-kit/primitives/sheet";
import { formatMessage, SUPPORTED_LOCALES, useI18n } from "@/i18n";
import { getLuaSkillsHubSkill } from "@/service/skills-actions-service";
import type { LuaSkillsHubSkillDetail } from "@/types";

// DetailState is the drawer's load result for one skill and language.
// DetailState 是抽屉针对一个技能与语言的加载结果。
type DetailState =
  | { status: "loading" }
  | { status: "ready"; detail: LuaSkillsHubSkillDetail }
  | { status: "error"; message: string };

// LuaSkillHubDetailSheet shows a Hub skill's AI overview and README in the interface language, with the original one click away.
// LuaSkillHubDetailSheet 以界面语言显示 Hub 技能的 AI 概览与 README，原文一键可见。
export function LuaSkillHubDetailSheet(props: {
  // Hub skill identifier to show.
  // 要显示的 Hub 技能标识。
  skillId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t, locale } = useI18n();
  const [state, setState] = React.useState<DetailState>({ status: "loading" });
  // showOriginal switches a translation back to the publisher's own text for this skill and language.
  // showOriginal 针对当前技能与语言把译文切回发布者原文。
  const [showOriginal, setShowOriginal] = React.useState(false);

  // Each open, skill or language change reads the detail again; a late answer for an earlier target is dropped.
  // 每次打开、切换技能或语言都重新读取详情；针对旧目标的迟到响应被丢弃。
  React.useEffect(() => {
    if (!props.open) return;
    let current = true;
    setState({ status: "loading" });
    setShowOriginal(false);
    getLuaSkillsHubSkill(props.skillId, locale).then(
      (detail) => { if (current) setState({ status: "ready", detail }); },
      (error: unknown) => { if (current) setState({ status: "error", message: error instanceof Error ? error.message : String(error) }); },
    );
    return () => { current = false; };
  }, [locale, props.open, props.skillId]);

  const detail = state.status === "ready" ? state.detail : null;
  const localized = detail?.skill.content ?? null;
  const translated = localized?.kind === "translation";
  const content = translated && showOriginal ? detail?.original ?? localized : localized;
  const readme = translated && !showOriginal ? localized?.readme ?? detail?.latest.readme ?? "" : detail?.latest.readme ?? "";
  const sourceLanguage = SUPPORTED_LOCALES.find((entry) => entry.code === localized?.sourceLocale)?.nativeName ?? t("skills.lua.hubLanguageOther");

  return (
    <Sheet open={props.open} onOpenChange={props.onOpenChange}>
      <SheetContent className="vk-form-scroll" size="lg">
        <SheetHeader>
          <SheetTitle>{detail?.skill.displayName || props.skillId}</SheetTitle>
          <SheetDescription>
            {detail ? [detail.skill.skillId, detail.latest.version, detail.skill.publisher].filter(Boolean).join(" · ") : ""}
          </SheetDescription>
        </SheetHeader>
        <div className="lua-skill-hub-detail vk-form-loose vk-form-pad">
          {state.status === "loading" ? <p className="lua-skill-hub-status">{t("skills.lua.hubDetailLoading")}</p> : null}
          {state.status === "error" ? (
            <PageNotice details={state.message} detailsLabel={t("skills.lua.errorDetails")} tone="error">
              {t("skills.lua.hubDetailFailed")}
            </PageNotice>
          ) : null}
          {translated ? (
            <div className="lua-skill-hub-detail-language">
              <span>{formatMessage(t("skills.lua.hubMachineTranslated"), { language: sourceLanguage })}</span>
              <Button size="sm" type="button" variant="ghost" onClick={() => setShowOriginal((value) => !value)}>
                {showOriginal ? t("skills.lua.hubShowTranslation") : t("skills.lua.hubShowOriginal")}
              </Button>
            </div>
          ) : null}
          {localized?.kind === "original_fallback" ? (
            <p className="lua-skill-hub-detail-language">{t("skills.lua.hubNoTranslation")}</p>
          ) : null}
          {content && (content.intro || content.features.length > 0) ? (
            <section className="lua-skill-hub-detail-section">
              <h3>{t("skills.lua.hubAiOverview")}</h3>
              {content.intro ? <p>{content.intro}</p> : null}
              {content.features.length > 0 ? (
                <ul>{content.features.map((feature, index) => <li key={index}>{feature}</li>)}</ul>
              ) : null}
            </section>
          ) : null}
          {detail && readme ? (
            <section className="lua-skill-hub-detail-section">
              <h3>{t("skills.lua.hubReadme")}</h3>
              <MarkdownMessage content={readme} eager />
            </section>
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  );
}
