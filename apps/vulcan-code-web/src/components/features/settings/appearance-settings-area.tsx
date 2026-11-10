import { useState } from "react";
import { ArrowUp, MessageSquare, Sparkles } from "lucide-react";
import { PageBody, PageRow, PageSection } from "@/components/page";
import { APPEARANCE_THEME_ICONS, useChooseAppearance } from "@/components/shell/app-chrome";
import { type TranslationKey, useI18n } from "@/i18n";
import {
  APPEARANCE_AMBIENCES,
  APPEARANCE_TEXT_TIERS,
  APPEARANCE_THEMES,
  APPEARANCE_WIDTHS,
  type AppearanceAmbience,
  type AppearancePreferences,
  type AppearanceText,
  type AppearanceTheme,
  type AppearanceWidth,
  useAppearance,
} from "@/ui-kit/appearance";
import { Button } from "@/ui-kit/primitives/button";
import { Switch } from "@/ui-kit/primitives/switch";
import { SettingsPreviewChoices } from "./settings-preview-choices";
import "@/ui-kit/styles/features/appearance-settings.css";

// AppearanceSettingsArea gathers the device-local display choices: theme, text tier, reading width, ambience
// and motion. Every change applies at once and is kept only on this device.
// AppearanceSettingsArea 汇集本机显示选择：主题、字号、阅读宽度、氛围与动效。
// 每次修改立即生效，且只保存在此设备上。
// Parameters: none. Returns the settings sections.
// 参数：无。返回设置分区。
export function AppearanceSettingsArea() {
  // t selects user-facing copy.
  // t 选择面向用户的文案。
  const { t } = useI18n();
  // appearance is the live preference; choose applies one change and reports a refused save.
  // appearance 是实时偏好；choose 应用一项变更并提示保存被拒绝。
  const appearance = useAppearance();
  const choose = useChooseAppearance();
  return (
    <PageBody>
      <PageSection title={t("settings.appearance.preview")} description={t("settings.appearance.display.description")}>
        <AppearanceScene appearance={appearance} />
      </PageSection>
      <PageSection title={t("settings.appearance.display")}>
        <SettingsPreviewChoices
          label={t("appChrome.theme")}
          description={t("settings.appearance.theme.description")}
          value={appearance.theme}
          onSelect={(value: AppearanceTheme) => choose("theme", value)}
          options={APPEARANCE_THEMES.map((theme) => ({
            value: theme, label: t(`appChrome.theme.${theme}` as TranslationKey),
            preview: <AppearanceThemeExample theme={theme} />,
          }))}
        />
        <SettingsPreviewChoices
          label={t("appChrome.text")}
          description={t("settings.appearance.text.description")}
          value={appearance.text}
          onSelect={(value: AppearanceText) => choose("text", value)}
          options={APPEARANCE_TEXT_TIERS.map((tier) => ({
            value: tier, label: t(`appChrome.text.${tier}` as TranslationKey),
            preview: <span className="appearance-text-sample" data-vk-preview-text={tier}>{t("settings.appearance.previewText")}</span>,
          }))}
        />
        <SettingsPreviewChoices
          label={t("appChrome.width")}
          description={t("settings.appearance.width.description")}
          value={appearance.width}
          onSelect={(value: AppearanceWidth) => choose("width", value)}
          options={APPEARANCE_WIDTHS.map((width) => ({
            value: width, label: t(`appChrome.width.${width}` as TranslationKey),
            preview: <span className="appearance-width-sample" data-vk-preview-width={width}>
              <span className="settings-preview-lines"><i /><i /><i /></span>
            </span>,
          }))}
        />
        <SettingsPreviewChoices
          label={t("settings.appearance.ambience")}
          description={t("settings.appearance.ambience.description")}
          value={appearance.ambience}
          onSelect={(value: AppearanceAmbience) => choose("ambience", value)}
          options={APPEARANCE_AMBIENCES.map((ambience) => ({
            value: ambience, label: t(`settings.appearance.ambience.${ambience}` as TranslationKey),
            preview: <span className="appearance-ambience appearance-ambience-sample" data-vk-preview-ambience={ambience} data-active="">
              <span className="appearance-scene-head"><Sparkles size={13} />{t("assistantDock.thinkingLive")}</span>
              <span className="appearance-scene-input"><i /><ArrowUp size={13} /></span>
            </span>,
          }))}
        />
        <PageRow label={t("appChrome.motion.reduce")} description={t("settings.appearance.motion.description")}>
          <Switch
            aria-label={t("appChrome.motion.reduce")}
            checked={appearance.motion === "reduce"}
            onCheckedChange={(checked) => choose("motion", checked ? "reduce" : "system")}
          />
        </PageRow>
      </PageSection>
    </PageBody>
  );
}

// Render a sample conversation with the current appearance; the activity button affects only this scene.
// 按当前外观渲染一段示例会话；工作状态按钮仅影响这个场景。
function AppearanceScene({ appearance }: { appearance: AppearancePreferences }) {
  // active previews the AI-working state without submitting a message.
  // active 模拟 AI 工作状态，不会发送消息。
  const [active, setActive] = useState(false);
  // Both the title and the sample body use the selected locale.
  // 示例标题和正文均使用当前语言。
  const { t } = useI18n();
  return <div className="appearance-scene-wrap">
    <div className="appearance-ambience appearance-scene"
      data-vk-preview-text={appearance.text}
      data-vk-preview-width={appearance.width}
      data-vk-preview-ambience={appearance.ambience}
      data-active={active || undefined}>
      <div className="appearance-scene-head">
        <MessageSquare size={15} />{t("settings.preview.generatedSample")}
        {active ? <span role="status"><Sparkles size={13} />{t("assistantDock.thinkingLive")}</span> : null}
      </div>
      <div className="appearance-scene-body">
        <strong>{t("settings.appearance.previewText")}</strong>
        <p>{t("settings.preview.thought")}</p>
        <div aria-hidden="true" className="appearance-scene-input"><i /><ArrowUp size={14} /></div>
      </div>
    </div>
    <Button aria-pressed={active} onClick={() => setActive(!active)} size="sm" type="button" variant="outline">
      <Sparkles />{t("settings.appearance.previewActivity")}
    </Button>
  </div>;
}

// Return a miniature window using the real palette for this theme, including system mode.
// 使用该主题的真实配色返回迷你窗口，也支持跟随系统。
function AppearanceThemeExample({ theme }: { theme: AppearanceTheme }) {
  // The icon follows the same map as the quick appearance menu.
  // 图标沿用外观快捷菜单的映射。
  const Icon = APPEARANCE_THEME_ICONS[theme];
  return <span className="appearance-theme-sample" data-vk-preview-theme={theme}>
    <span className="appearance-scene-head"><Icon size={13} /><i /></span>
    <span className="settings-preview-lines"><i /><i /><i /></span>
  </span>;
}
