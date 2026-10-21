import { PageBody, PageRow, PageSection } from "@/components/page";
import { APPEARANCE_THEME_ICONS, useChooseAppearance } from "@/components/shell/app-chrome";
import { type TranslationKey, useI18n } from "@/i18n";
import {
  APPEARANCE_AMBIENCES,
  APPEARANCE_TEXT_TIERS,
  APPEARANCE_THEMES,
  APPEARANCE_WIDTHS,
  type AppearanceAmbience,
  type AppearanceText,
  type AppearanceTheme,
  type AppearanceWidth,
  useAppearance,
} from "@/ui-kit/appearance";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Switch } from "@/ui-kit/primitives/switch";

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
      <PageSection layout="rows" title={t("settings.appearance.display")} description={t("settings.appearance.display.description")}>
        <PageRow label={t("appChrome.theme")} description={t("settings.appearance.theme.description")}>
          <SegmentedControl
            aria-label={t("appChrome.theme")}
            value={appearance.theme}
            onValueChange={(value) => choose("theme", value as AppearanceTheme)}
          >
            {APPEARANCE_THEMES.map((theme) => {
              const Icon = APPEARANCE_THEME_ICONS[theme];
              return (
                <SegmentedControlItem key={theme} value={theme}>
                  <Icon aria-hidden="true" />
                  <span>{t(`appChrome.theme.${theme}` as TranslationKey)}</span>
                </SegmentedControlItem>
              );
            })}
          </SegmentedControl>
        </PageRow>
        <PageRow label={t("appChrome.text")} description={t("settings.appearance.text.description")}>
          <SegmentedControl
            aria-label={t("appChrome.text")}
            value={appearance.text}
            onValueChange={(value) => choose("text", value as AppearanceText)}
          >
            {APPEARANCE_TEXT_TIERS.map((tier) => (
              <SegmentedControlItem key={tier} value={tier}>{t(`appChrome.text.${tier}` as TranslationKey)}</SegmentedControlItem>
            ))}
          </SegmentedControl>
        </PageRow>
        <PageRow label={t("appChrome.width")} description={t("settings.appearance.width.description")}>
          <SegmentedControl
            aria-label={t("appChrome.width")}
            value={appearance.width}
            onValueChange={(value) => choose("width", value as AppearanceWidth)}
          >
            {APPEARANCE_WIDTHS.map((width) => (
              <SegmentedControlItem key={width} value={width}>{t(`appChrome.width.${width}` as TranslationKey)}</SegmentedControlItem>
            ))}
          </SegmentedControl>
        </PageRow>
        <PageRow label={t("settings.appearance.ambience")} description={t("settings.appearance.ambience.description")}>
          <SegmentedControl
            aria-label={t("settings.appearance.ambience")}
            value={appearance.ambience}
            onValueChange={(value) => choose("ambience", value as AppearanceAmbience)}
          >
            {APPEARANCE_AMBIENCES.map((ambience) => (
              <SegmentedControlItem key={ambience} value={ambience}>{t(`settings.appearance.ambience.${ambience}` as TranslationKey)}</SegmentedControlItem>
            ))}
          </SegmentedControl>
        </PageRow>
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
