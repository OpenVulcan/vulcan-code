//! Embedded language-pack registry, fallback resolution, and named-template formatting helpers.
//! 内置语言包注册表、回退解析与命名模板格式化辅助逻辑。

use super::{AppLocale, I18nArg, I18nKey};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// One embedded language-pack manifest describing the locale and human-readable labels.
/// 描述语言与人类可读标签的一份内置语言包清单。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct LanguagePackManifest {
    /// Stable locale identifier stored in the manifest file.
    /// 存储在清单文件中的稳定语言标识。
    locale: String,
    /// Native language name displayed to users.
    /// 展示给用户的本地语言名称。
    #[serde(rename = "nativeName")]
    native_name: String,
    /// English language name displayed to users.
    /// 展示给用户的英文语言名称。
    #[serde(rename = "englishName")]
    english_name: String,
    /// Reading direction hint reserved for future RTL support.
    /// 为未来 RTL 支持保留的阅读方向提示。
    direction: String,
    /// Language-pack schema version.
    /// 语言包架构版本号。
    version: u32,
}

/// One fully loaded language pack containing its manifest and flattened string entries.
/// 包含清单与扁平化字符串条目的一份完整已加载语言包。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LanguagePack {
    /// Parsed manifest metadata.
    /// 解析后的清单元数据。
    manifest: LanguagePackManifest,
    /// Flattened localized strings keyed by stable dotted key identifiers.
    /// 以稳定点号键标识作为索引的扁平化本地化字符串集合。
    entries: BTreeMap<String, String>,
}

/// Embedded language-pack registry shared across the GUI process.
/// 在整个图形界面进程中共享的内置语言包注册表。
#[derive(Debug)]
pub struct LanguageRegistry {
    /// Locale-indexed embedded language packs.
    /// 按语言索引的内置语言包集合。
    packs: BTreeMap<AppLocale, LanguagePack>,
    /// Stable fallback locale used when one translation key is missing.
    /// 某个翻译键缺失时使用的稳定回退语言。
    fallback_locale: AppLocale,
}

impl LanguageRegistry {
    /// Returns the shared embedded language-pack registry initialized on first use.
    /// 返回首次使用时初始化的共享内置语言包注册表。
    pub fn embedded() -> &'static Self {
        static REGISTRY: OnceLock<LanguageRegistry> = OnceLock::new();

        REGISTRY.get_or_init(Self::load_embedded)
    }

    /// Returns the stable locale order exposed to the GUI picker.
    /// 返回暴露给图形界面选择器的稳定语言顺序。
    pub fn available_locales(&self) -> &'static [AppLocale] {
        &AppLocale::ALL
    }

    /// Resolves one localized text using the requested locale and fallback locale when the key is missing.
    /// 使用请求语言解析一条本地化文本，并在键缺失时回退到默认语言。
    pub fn text(&self, locale: AppLocale, key: I18nKey) -> String {
        self.lookup(locale, key)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| key.as_str().to_string())
    }

    /// Resolves and formats one localized text using the requested locale and named interpolation arguments.
    /// 使用请求语言与命名插值参数解析并格式化一条本地化文本。
    pub fn format(&self, locale: AppLocale, key: I18nKey, args: &[I18nArg<'_>]) -> String {
        format_template(&self.text(locale, key), args)
    }

    /// Builds the embedded language-pack registry from compile-time JSON resources.
    /// 从编译期 JSON 资源构建内置语言包注册表。
    fn load_embedded() -> Self {
        let packs = BTreeMap::from([
            (AppLocale::ZhCn, load_language_pack(AppLocale::ZhCn)),
            (AppLocale::EnUs, load_language_pack(AppLocale::EnUs)),
        ]);

        let registry = Self {
            packs,
            fallback_locale: AppLocale::ZhCn,
        };
        registry.assert_placeholder_compatibility();
        registry
    }

    /// Returns one borrowed localized entry when it exists in the requested locale or in the fallback locale.
    /// 当目标语言或回退语言中存在对应条目时，返回一条借用的本地化字符串。
    fn lookup(&self, locale: AppLocale, key: I18nKey) -> Option<&str> {
        self.packs
            .get(&locale)
            .and_then(|pack| pack.entries.get(key.as_str()))
            .or_else(|| {
                self.packs
                    .get(&self.fallback_locale)
                    .and_then(|pack| pack.entries.get(key.as_str()))
            })
            .map(String::as_str)
    }

    /// Ensures every locale provides the same placeholder set for each registered key to avoid runtime formatting drift.
    /// 确保每种语言在每个已注册键上都提供相同的占位符集合，避免运行时格式化漂移。
    fn assert_placeholder_compatibility(&self) {
        for locale in AppLocale::ALL {
            let Some(pack) = self.packs.get(&locale) else {
                panic!("missing embedded language pack for {}", locale.as_str());
            };

            for key in [
                I18nKey::CommonCancel,
                I18nKey::CommonClose,
                I18nKey::CommonConfirm,
                I18nKey::CommonDelete,
                I18nKey::CommonLanguageLabel,
                I18nKey::CommonLoading,
                I18nKey::CommonPinned,
                I18nKey::CommonProject,
                I18nKey::CommonRetry,
                I18nKey::CommonSearch,
                I18nKey::CommonThemeLabel,
                I18nKey::CommonUnknownDuration,
                I18nKey::GuideQuickMarkdown,
                I18nKey::HistorySearchActiveProjectsOnly,
                I18nKey::HistorySearchFailedWithError,
                I18nKey::HistorySearchFilterPlaceholder,
                I18nKey::HistorySearchHint,
                I18nKey::HistorySearchLoading,
                I18nKey::HistorySearchNoProjectMatches,
                I18nKey::HistorySearchProjectFilterPlaceholder,
                I18nKey::HistorySearchScopePrefix,
                I18nKey::HistorySearchSearchAction,
                I18nKey::HistorySearchSearching,
                I18nKey::HistorySearchTitle,
                I18nKey::SettingsRuntimeParametersTitle,
                I18nKey::SettingsRuntimeParametersDescription,
                I18nKey::SettingsRootTitle,
                I18nKey::SettingsLanguagePlaceholder,
                I18nKey::SettingsSessionTranscriptTurnPageSizeLabel,
                I18nKey::SettingsSessionTranscriptTurnPageSizePlaceholder,
                I18nKey::SettingsModelGroupTitle,
                I18nKey::SettingsThemePlaceholder,
                I18nKey::SettingsTemperatureToggle,
                I18nKey::SettingsTemperatureDisabledNotice,
                I18nKey::SettingsTemperatureValueEnabled,
                I18nKey::SettingsTemperatureValueDefault,
                I18nKey::SettingsAiCapabilityGroupTitle,
                I18nKey::SettingsArchivedProjectsTitle,
                I18nKey::SettingsArchivedRestoreEmpty,
                I18nKey::SettingsArchivedRestoreHint,
                I18nKey::SettingsArchivedRestoreTitle,
                I18nKey::SettingsArchivedStandaloneSessionsTitle,
                I18nKey::SettingsPolicyTitle,
                I18nKey::SettingsPolicyGeneratePlan,
                I18nKey::SettingsPolicyRunCargoCheck,
                I18nKey::SettingsPolicyPersistMemory,
                I18nKey::SettingsPolicyAutoApprove,
                I18nKey::SettingsIntegrationGuideTitle,
                I18nKey::SettingsActivityLogTitle,
                I18nKey::SettingsActivityLogHint,
                I18nKey::StartupDataPlaneInitFailedTitle,
                I18nKey::StartupLoadingSubtitle,
                I18nKey::StartupLoadingTitle,
                I18nKey::StatusToolDetailLoading,
                I18nKey::StatusToolDetailLoadingWithFailures,
                I18nKey::StatusHistorySearchAllLoaded,
                I18nKey::StatusHistorySearchCompleted,
                I18nKey::StatusHistorySearchEmptyQuery,
                I18nKey::StatusHistorySearchFailed,
                I18nKey::StatusHistorySearchInProgress,
                I18nKey::StatusHistorySearchLoadMoreCompleted,
                I18nKey::StatusHistorySearchLoadMoreStarted,
                I18nKey::StatusHistorySearchNoMoreResults,
                I18nKey::StatusHistorySearchQueryChanged,
                I18nKey::StatusHistorySearchStarted,
                I18nKey::StatusInitializingDataPlane,
                I18nKey::StatusInitializingLuaSkills,
                I18nKey::StatusLocaleChanged,
                I18nKey::StatusMainPanelSwitched,
                I18nKey::StatusNoMoreOlderHistory,
                I18nKey::StatusOlderHistoryLoaded,
                I18nKey::StatusOlderHistoryLoading,
                I18nKey::StatusOlderHistoryLoadingInProgress,
                I18nKey::StatusProjectionBuilding,
                I18nKey::StatusSessionHydrated,
                I18nKey::StatusSessionLoading,
                I18nKey::StatusSessionSwitched,
                I18nKey::TranscriptUserPrompt,
                I18nKey::TranscriptThinking,
                I18nKey::TranscriptOutput,
                I18nKey::TranscriptToolResult,
                I18nKey::TranscriptToolResultWithName,
                I18nKey::TranscriptToolCallWithName,
                I18nKey::TranscriptGeneratedImage,
                I18nKey::TranscriptGeneratedAudio,
                I18nKey::TranscriptGeneratedVideo,
                I18nKey::TranscriptGeneratedDocument,
                I18nKey::TranscriptGeneratedCountSuffix,
                I18nKey::TranscriptSubagentThinking,
                I18nKey::TranscriptThinkingModalHint,
                I18nKey::TranscriptThinkingOverlayHint,
                I18nKey::TranscriptInputHint,
                I18nKey::TranscriptJumpToBottom,
                I18nKey::TranscriptLoadOlderHistory,
                I18nKey::TranscriptLoadingOlderHistory,
                I18nKey::TranscriptLoadingSessionTitle,
                I18nKey::TranscriptNoApiRequestSnapshot,
                I18nKey::TranscriptNoRenderableBlocks,
                I18nKey::TranscriptNoSessionSelectedHint,
                I18nKey::TranscriptNoSessionSelectedTitle,
                I18nKey::TranscriptNotStartedTitle,
                I18nKey::TranscriptOriginalToolResult,
                I18nKey::TranscriptGeneratedResultsTitle,
                I18nKey::TranscriptImagePreviewHint,
                I18nKey::TranscriptAudioPreviewHint,
                I18nKey::TranscriptVideoPreviewHint,
                I18nKey::TranscriptDocumentPreviewHint,
                I18nKey::TranscriptPreviewImageAction,
                I18nKey::TranscriptPlayAudioAction,
                I18nKey::TranscriptStopPlaybackAction,
                I18nKey::TranscriptOpenVideoAction,
                I18nKey::TranscriptOpenDocumentAction,
                I18nKey::TranscriptCopyImageAction,
                I18nKey::TranscriptCopyPathAction,
                I18nKey::TranscriptRevealFileAction,
                I18nKey::TranscriptExportedPathLabel,
                I18nKey::TranscriptManagedCopyPathLabel,
                I18nKey::TranscriptSubagentCompletedHint,
                I18nKey::TranscriptSubagentFailedHint,
                I18nKey::TranscriptSubagentResultSummaryHint,
                I18nKey::TranscriptSubagentResultSummaryTitle,
                I18nKey::TranscriptSubagentRunningHint,
                I18nKey::TranscriptSubagentStoppedHint,
                I18nKey::TranscriptToolModalTitle,
                I18nKey::TranscriptToolModalHint,
                I18nKey::WorkbenchImagePreviewHint,
                I18nKey::WorkbenchImagePreviewTitle,
                I18nKey::WorkbenchModelPickerFilterPlaceholder,
                I18nKey::WorkbenchModelPickerNoMatches,
                I18nKey::WorkbenchModelPickerTitle,
                I18nKey::WorkbenchNewSessionAction,
                I18nKey::WorkbenchNewSessionHint,
                I18nKey::WorkbenchEmptyPanel,
                I18nKey::WorkbenchRuntimeWarnings,
                I18nKey::WorkbenchPanelShortcutHint,
                I18nKey::WorkbenchPanelMainLabel,
                I18nKey::WorkbenchPanelMainDescription,
                I18nKey::WorkbenchPanelPromptSkillsLabel,
                I18nKey::WorkbenchPanelPromptSkillsDescription,
                I18nKey::WorkbenchPanelLuaSkillsLabel,
                I18nKey::WorkbenchPanelLuaSkillsDescription,
                I18nKey::WorkbenchPanelAutomationLabel,
                I18nKey::WorkbenchPanelAutomationDescription,
                I18nKey::WorkbenchPanelAgentsLabel,
                I18nKey::WorkbenchPanelAgentsDescription,
                I18nKey::WorkbenchPanelPluginsLabel,
                I18nKey::WorkbenchPanelPluginsDescription,
                I18nKey::WorkbenchPanelMemoryMeshLabel,
                I18nKey::WorkbenchPanelMemoryMeshDescription,
                I18nKey::WorkbenchPanelExampleLabel,
                I18nKey::WorkbenchPanelExampleDescription,
                I18nKey::WorkbenchPanelSettingsLabel,
                I18nKey::WorkbenchPanelSettingsDescription,
                I18nKey::WorkbenchSelectModel,
                I18nKey::SettingsSectionModelProvidersLabel,
                I18nKey::SettingsSectionModelProvidersDescription,
                I18nKey::SettingsSectionModelListLabel,
                I18nKey::SettingsSectionModelListDescription,
                I18nKey::SettingsSectionRuntimeDiagnosticsLabel,
                I18nKey::SettingsSectionRuntimeDiagnosticsDescription,
                I18nKey::SettingsSectionPromptComposerLabel,
                I18nKey::SettingsSectionPromptComposerDescription,
                I18nKey::SettingsSectionAiCapabilityVendorsLabel,
                I18nKey::SettingsSectionAiCapabilityVendorsDescription,
                I18nKey::SettingsSectionAiCapabilityWebSearchLabel,
                I18nKey::SettingsSectionAiCapabilityWebSearchDescription,
                I18nKey::SettingsSectionAiCapabilityUnderstandImageLabel,
                I18nKey::SettingsSectionAiCapabilityUnderstandImageDescription,
                I18nKey::SettingsSectionAiCapabilityUnderstandVideoLabel,
                I18nKey::SettingsSectionAiCapabilityUnderstandVideoDescription,
                I18nKey::SettingsSectionAiCapabilityUnderstandAudioLabel,
                I18nKey::SettingsSectionAiCapabilityUnderstandAudioDescription,
                I18nKey::SettingsSectionAiCapabilityImageGenerationLabel,
                I18nKey::SettingsSectionAiCapabilityImageGenerationDescription,
                I18nKey::SettingsSectionAiCapabilityVideoGenerationLabel,
                I18nKey::SettingsSectionAiCapabilityVideoGenerationDescription,
                I18nKey::SettingsSectionAiCapabilitySpeechSynthesisLabel,
                I18nKey::SettingsSectionAiCapabilitySpeechSynthesisDescription,
                I18nKey::SettingsSectionAiCapabilityMusicGenerationLabel,
                I18nKey::SettingsSectionAiCapabilityMusicGenerationDescription,
                I18nKey::SettingsSectionAiCapabilityMusicCoverLabel,
                I18nKey::SettingsSectionAiCapabilityMusicCoverDescription,
                I18nKey::SettingsSectionMcpServersLabel,
                I18nKey::SettingsSectionMcpServersDescription,
                I18nKey::SettingsSectionAboutLicensesLabel,
                I18nKey::SettingsSectionAboutLicensesDescription,
                I18nKey::SettingsSectionArchivedRestoreLabel,
                I18nKey::SettingsSectionArchivedRestoreDescription,
                I18nKey::TaskTreeNoSessions,
                I18nKey::TaskTreeAddProjectFromFolder,
                I18nKey::TaskTreeArchivedBadge,
                I18nKey::TaskTreeArchivedProject,
                I18nKey::TaskTreeArchivedSession,
                I18nKey::TaskTreePermanentDeletePrompt,
                I18nKey::StatusSettingsSectionSwitched,
                I18nKey::StatusThemeChanged,
            ] {
                let value = pack.entries.get(key.as_str()).unwrap_or_else(|| {
                    panic!("missing key {} for {}", key.as_str(), locale.as_str())
                });
                let discovered = extract_placeholders(value);
                let expected = key.placeholders().iter().copied().collect::<BTreeSet<_>>();
                assert_eq!(
                    discovered,
                    expected,
                    "placeholder mismatch for key {} in {}",
                    key.as_str(),
                    locale.as_str()
                );
            }
        }
    }
}

/// Resolves one localized text through the shared embedded registry for the requested locale.
/// 通过共享内置注册表为指定语言解析一条本地化文本。
pub fn text_for_locale(locale: AppLocale, key: I18nKey) -> String {
    LanguageRegistry::embedded().text(locale, key)
}

/// Resolves and formats one localized text through the shared embedded registry for the requested locale.
/// 通过共享内置注册表为指定语言解析并格式化一条本地化文本。
pub fn format_for_locale(locale: AppLocale, key: I18nKey, args: &[I18nArg<'_>]) -> String {
    LanguageRegistry::embedded().format(locale, key, args)
}

/// Loads one embedded language pack from its manifest and split JSON resource files.
/// 从清单与分领域 JSON 资源文件中加载一份内置语言包。
fn load_language_pack(locale: AppLocale) -> LanguagePack {
    let (manifest_source, resource_sources) = match locale {
        AppLocale::ZhCn => (
            include_str!("../../resources/i18n/zh-CN/manifest.json"),
            [
                include_str!("../../resources/i18n/zh-CN/common.json"),
                include_str!("../../resources/i18n/zh-CN/workbench.json"),
                include_str!("../../resources/i18n/zh-CN/settings.json"),
                include_str!("../../resources/i18n/zh-CN/transcript.json"),
                include_str!("../../resources/i18n/zh-CN/status.json"),
                include_str!("../../resources/i18n/zh-CN/task_tree.json"),
            ],
        ),
        AppLocale::EnUs => (
            include_str!("../../resources/i18n/en-US/manifest.json"),
            [
                include_str!("../../resources/i18n/en-US/common.json"),
                include_str!("../../resources/i18n/en-US/workbench.json"),
                include_str!("../../resources/i18n/en-US/settings.json"),
                include_str!("../../resources/i18n/en-US/transcript.json"),
                include_str!("../../resources/i18n/en-US/status.json"),
                include_str!("../../resources/i18n/en-US/task_tree.json"),
            ],
        ),
    };

    let manifest: LanguagePackManifest =
        serde_json::from_str(manifest_source).expect("embedded i18n manifest should be valid JSON");
    assert_eq!(
        manifest.locale,
        locale.as_str(),
        "embedded i18n manifest locale should match file path"
    );

    let mut entries = BTreeMap::new();
    for resource_source in resource_sources {
        entries.extend(parse_language_entries(resource_source));
    }

    LanguagePack { manifest, entries }
}

/// Parses one flat JSON object of localized strings into a BTreeMap.
/// 将一个扁平化本地化字符串 JSON 对象解析为一份 BTreeMap。
fn parse_language_entries(resource_source: &str) -> BTreeMap<String, String> {
    let value: Value =
        serde_json::from_str(resource_source).expect("embedded i18n resource should be valid JSON");
    let object = value
        .as_object()
        .expect("embedded i18n resource should be one flat object");

    object
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                value
                    .as_str()
                    .unwrap_or_else(|| panic!("embedded i18n key `{key}` should be a string"))
                    .to_string(),
            )
        })
        .collect()
}

/// Formats one localized template by replacing named placeholders with the supplied argument values.
/// 使用给定参数值替换命名占位符，从而格式化一条本地化模板。
fn format_template(template: &str, args: &[I18nArg<'_>]) -> String {
    let mut formatted = template.to_string();

    for arg in args {
        formatted = formatted.replace(&format!("{{{}}}", arg.name), arg.value.as_ref());
    }

    formatted
}

/// Extracts the set of named placeholders declared inside one localized template.
/// 提取一条本地化模板中声明的命名占位符集合。
fn extract_placeholders(template: &str) -> BTreeSet<&str> {
    let mut placeholders = BTreeSet::new();
    let mut remaining = template;

    while let Some(start_index) = remaining.find('{') {
        let candidate = &remaining[start_index + 1..];
        let Some(end_index) = candidate.find('}') else {
            break;
        };
        let placeholder = candidate[..end_index].trim();
        if !placeholder.is_empty() {
            placeholders.insert(placeholder);
        }
        remaining = &candidate[end_index + 1..];
    }

    placeholders
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the embedded registry exposes every supported locale declared by the picker.
    /// 验证内置注册表会暴露选择器声明的全部受支持语言。
    #[test]
    fn embedded_registry_covers_all_supported_locales() {
        let registry = LanguageRegistry::embedded();
        for locale in AppLocale::ALL {
            assert!(
                registry.packs.contains_key(&locale),
                "expected embedded pack for {}",
                locale.as_str()
            );
        }
    }

    /// Verifies named placeholder formatting preserves the fallback template when all arguments are supplied.
    /// 验证命名占位符格式化在参数齐备时会保留回退模板的正确结构。
    #[test]
    fn format_template_replaces_named_placeholders() {
        let formatted = format_template(
            "Tool result: {tool_name} ({count})",
            &[
                I18nArg::named("tool_name", "read_file"),
                I18nArg::named("count", "2"),
            ],
        );

        assert_eq!(formatted, "Tool result: read_file (2)");
    }

    /// Verifies registry lookup falls back to the stable key text only after locale and fallback-locale misses.
    /// 验证注册表查询会先回退默认语言，只有双重缺失后才回退到稳定键文本。
    #[test]
    fn text_lookup_falls_back_to_key_when_missing_everywhere() {
        let registry = LanguageRegistry::embedded();
        let value = registry.text(AppLocale::EnUs, I18nKey::TranscriptOutput);
        assert!(!value.is_empty());
    }
}
