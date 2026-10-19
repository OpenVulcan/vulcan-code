import * as React from "react";
import { PageNotice } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Combobox, type ComboboxOption } from "@/ui-kit/primitives/combobox";
import {
  inferenceRouteKey,
  inferenceRoutePickerOptions,
} from "@/components/features/inference-route-picker";
import { loadPublicAiSettings, savePublicAiInferenceRoute } from "@/service/settings-actions-service";
import { formatMessage, useI18n, type Translate } from "@/i18n";
import type {
  PublicAiInferenceProfile,
  PublicAiInferenceRoute,
  PublicAiInternalInferenceCandidate,
  PublicAiSettingsSnapshot,
} from "@/types";

// LUASKILLS_CONSUMER_PROFILE_ID is the optional consumer this section owns.
// LUASKILLS_CONSUMER_PROFILE_ID 是本区块持有的可选消费者。
const LUASKILLS_CONSUMER_PROFILE_ID = "luaskills";
// LUASKILLS_TEMPORARY_LLM_PURPOSE_ID is the single LLM purpose LuaSkills may use.
// LUASKILLS_TEMPORARY_LLM_PURPOSE_ID 是 LuaSkills 可使用的唯一 LLM 用途。
const LUASKILLS_TEMPORARY_LLM_PURPOSE_ID = "temporary_llm";

// luaskillsLlmCandidates returns only the validated candidates that can answer text.
// luaskillsLlmCandidates 只返回能够回答文本的已验证候选。
//
// The operation is the capability boundary: offering a model that cannot perform the
// configured operation, is disabled, or is scoped to another caller would let the user
// save a route that only fails at call time.
// 操作、启用状态和调用范围共同构成能力边界：提供不满足任一条件的模型，会让用户保存一条
// 只在调用时才失败的路由。
export function luaskillsLlmCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate[] {
  return candidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "conversation_respond"
  ));
}

// luaskillsEmbeddingCandidates returns only the validated candidates that can embed.
// luaskillsEmbeddingCandidates 只返回能够生成向量的已验证候选。
export function luaskillsEmbeddingCandidates(
  candidates: PublicAiInternalInferenceCandidate[],
): PublicAiInternalInferenceCandidate[] {
  return candidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
    && candidate.route.operation === "embedding_create"
  ));
}

// configuredRouteUnavailable reports whether a persisted route has no executable candidate.
// configuredRouteUnavailable 判断已持久化路由是否没有可执行候选。
//
// Parameters contain the optional persisted route and already eligibility-filtered candidates.
// 参数包含可选的已持久化路由和已经过资格过滤的候选。
// Returns true only for a retained route that cannot execute now.
// 返回值仅在保留的路由当前无法执行时为 true。
export function configuredRouteUnavailable(
  route: PublicAiInferenceRoute | undefined,
  candidates: PublicAiInternalInferenceCandidate[],
): boolean {
  return Boolean(route && !candidates.some((candidate) => (
    inferenceRouteKey(candidate.route) === inferenceRouteKey(route)
  )));
}

// appendUnavailableOption keeps an invalid saved identity visible but impossible to reselect.
// appendUnavailableOption 保持失效的已保存身份可见，但禁止再次选择。
//
// Parameters provide valid options, the retained route, its availability verdict, and locale.
// 参数提供有效选项、保留路由、可用性判定与语言环境。
// Returns options with at most one disabled retained-route entry appended.
// 返回值最多追加一个禁用的保留路由条目。
function appendUnavailableOption(
  options: ComboboxOption[],
  route: PublicAiInferenceRoute | undefined,
  unavailable: boolean,
  t: Translate): ComboboxOption[] {
  return unavailable && route ? [...options, {
    value: inferenceRouteKey(route),
    label: `${route.model_id} · ${route.connection_id}`,
    description: formatMessage(t("skills.routes.currentlyUnavailable"), { provider_id: route.provider_id, service_entry_id: route.service_entry_id }),
    keywords: [route.model_id, route.connection_id, route.provider_id, route.service_entry_id],
    disabled: true,
  }] : options;
}

// luaskillsSelectedRouteKeys derives both exact selected keys from one committed profile.
// luaskillsSelectedRouteKeys 从一份已提交 Profile 推导两个精确选中键。
//
// An absent profile yields empty keys, because LuaSkills is an optional consumer that
// may legitimately have no configured route at all.
// 缺失的 Profile 产出空键，因为 LuaSkills 是可选消费者，完全可以没有任何已配置路由。
export function luaskillsSelectedRouteKeys(
  profile: PublicAiInferenceProfile | undefined,
): { llm: string; embedding: string } {
  const llmRoute = profile?.llm_routes?.[LUASKILLS_TEMPORARY_LLM_PURPOSE_ID];
  return {
    llm: llmRoute ? inferenceRouteKey(llmRoute) : "",
    embedding: profile?.embedding ? inferenceRouteKey(profile.embedding) : "",
  };
}

// LuaSkillsInferenceRoutes selects the two exact internal routes LuaSkills may use.
// LuaSkillsInferenceRoutes 选择 LuaSkills 可使用的两条精确内部路由。
//
// Both pickers are fed by backend-validated candidates filtered on the operation each
// one configures, so a model that cannot perform the operation is never offered. The
// selection is an exact identity key rather than a model name, and saving goes through
// the shared public-AI transaction, which owns generation verification.
// 两个选择器都由后端已验证的候选供给，并按各自配置的操作过滤，因此无法执行该操作的模型绝不
// 会出现。选择结果是精确身份键而非模型名，保存走共享公共 AI 事务，代次验证由该事务持有。
export function LuaSkillsInferenceRoutes() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [llmKey, setLlmKey] = React.useState("");
  const [embeddingKey, setEmbeddingKey] = React.useState("");
  const [busy, setBusy] = React.useState<"llm" | "embedding" | null>(null);
  const [error, setError] = React.useState("");
  const [notice, setNotice] = React.useState("");

  // apply derives both selected keys from one committed snapshot.
  // apply 从一份已提交快照推导两个选中的键。
  const apply = React.useCallback((next: PublicAiSettingsSnapshot) => {
    setSnapshot(next);
    const keys = luaskillsSelectedRouteKeys(
      next.inferenceProfiles[LUASKILLS_CONSUMER_PROFILE_ID],
    );
    setLlmKey(keys.llm);
    setEmbeddingKey(keys.embedding);
  }, []);

  React.useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const next = await loadPublicAiSettings();
        if (!cancelled) {
          apply(next);
          setError("");
        }
      } catch (cause) {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [apply]);

  // llmCandidates and embeddingCandidates filter the validated candidates on the exact
  // operation, which is the capability boundary each picker configures.
  // llmCandidates 与 embeddingCandidates 按精确操作过滤已验证候选，即每个选择器所配置的
  // 能力边界。
  const llmCandidates = React.useMemo(
    () => luaskillsLlmCandidates(snapshot?.internalInferenceCandidates ?? []),
    [snapshot],
  );
  const embeddingCandidates = React.useMemo(
    () => luaskillsEmbeddingCandidates(snapshot?.internalInferenceCandidates ?? []),
    [snapshot],
  );
  const llmOptions: ComboboxOption[] = React.useMemo(
    () => inferenceRoutePickerOptions(llmCandidates), [llmCandidates]);
  const embeddingOptions: ComboboxOption[] = React.useMemo(
    () => inferenceRoutePickerOptions(embeddingCandidates), [embeddingCandidates]);
  // persistedProfile is the authoritative LuaSkills route state retained across catalog changes.
  // persistedProfile 是在目录变化后仍保留的权威 LuaSkills 路由状态。
  const persistedProfile = snapshot?.inferenceProfiles[LUASKILLS_CONSUMER_PROFILE_ID];
  const persistedLlmRoute = persistedProfile?.llm_routes?.[LUASKILLS_TEMPORARY_LLM_PURPOSE_ID];
  const persistedEmbeddingRoute = persistedProfile?.embedding;
  const llmUnavailable = configuredRouteUnavailable(persistedLlmRoute, llmCandidates);
  const embeddingUnavailable = configuredRouteUnavailable(persistedEmbeddingRoute, embeddingCandidates);
  // diagnostics contains only the backend verdicts owned by the LuaSkills consumer.
  // diagnostics 仅包含归属于 LuaSkills 消费者的后端判定。
  const diagnostics = snapshot?.inferenceRouteDiagnostics ?? [];
  const llmDiagnostic = diagnostics.find((diagnostic) => (
    diagnostic.consumerProfileId === LUASKILLS_CONSUMER_PROFILE_ID
    && diagnostic.operation === "conversation_respond"
    && diagnostic.purposeId === LUASKILLS_TEMPORARY_LLM_PURPOSE_ID
    && persistedLlmRoute !== undefined
    && inferenceRouteKey(diagnostic.route) === inferenceRouteKey(persistedLlmRoute)
  ));
  const embeddingDiagnostic = diagnostics.find((diagnostic) => (
    diagnostic.consumerProfileId === LUASKILLS_CONSUMER_PROFILE_ID
    && diagnostic.operation === "embedding_create"
    && diagnostic.purposeId === undefined
    && persistedEmbeddingRoute !== undefined
    && inferenceRouteKey(diagnostic.route) === inferenceRouteKey(persistedEmbeddingRoute)
  ));
  const displayedLlmOptions = React.useMemo(
    () => appendUnavailableOption(llmOptions, persistedLlmRoute, llmUnavailable, t),
    [llmOptions, persistedLlmRoute, llmUnavailable, t],
  );
  const displayedEmbeddingOptions = React.useMemo(
    () => appendUnavailableOption(embeddingOptions, persistedEmbeddingRoute, embeddingUnavailable, t),
    [embeddingOptions, persistedEmbeddingRoute, embeddingUnavailable, t],
  );

  // save commits one picker through the shared public-AI route transaction.
  // save 通过共享公共 AI 路由事务提交一个选择器。
  //
  // Parameters: which selects the picker, and route is the exact route or null to clear.
  // 参数：which 选择选择器，route 是精确路由，null 表示清除。
  // Returns: a promise that settles after the committed snapshot is applied.
  // 返回值：在已提交快照被应用后结束的 Promise。
  const save = React.useCallback(async (
    which: "llm" | "embedding",
    route: PublicAiInferenceRoute | null,
  ) => {
    setBusy(which);
    setError("");
    setNotice("");
    try {
      const next = await savePublicAiInferenceRoute(which === "llm"
        ? {
          consumerProfileId: LUASKILLS_CONSUMER_PROFILE_ID,
          purposeId: LUASKILLS_TEMPORARY_LLM_PURPOSE_ID,
          operation: "conversation_respond",
          route,
        }
        : {
          consumerProfileId: LUASKILLS_CONSUMER_PROFILE_ID,
          operation: "embedding_create",
          route,
        });
      apply(next);
      setNotice(route
        ? t("skills.routes.savedAndActiveImmediately")
        : t("skills.routes.clearedTheCapabilityIs"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }, [apply, t]);

  // selectedRoute resolves one exact key back to its validated candidate route.
  // selectedRoute 将一个精确键解析回其已验证候选路由。
  const selectedRoute = (
    candidates: typeof llmCandidates,
    key: string,
  ): PublicAiInferenceRoute | null => candidates
    .map((candidate) => candidate.route)
    .find((route) => inferenceRouteKey(route) === key) ?? null;

  return (
    <section className="lua-settings-section lua-inference-routes">
      <h4>{t("skills.routes.internalModelRoutes")}</h4>
      <p>{t("skills.routes.luaskillsBindsOneExact")}</p>
      {error ? <PageNotice tone="error"><pre>{error}</pre></PageNotice> : null}
      {notice ? <p role="status">{notice}</p> : null}
      {llmUnavailable ? <PageNotice details={llmDiagnostic?.reason} detailsLabel={t("skills.lua.errorDetails")} role="alert" tone="warning">
        {t("skills.routes.theSavedTemporaryQuestion")}
      </PageNotice> : null}
      <label className="lua-inference-route-row">
        <span>{t("skills.routes.temporaryQuestionAnsweringTemporary")}</span>
        <Combobox
          ariaLabel={t("skills.routes.temporaryLlmRoute")}
          disabled={busy !== null || llmCandidates.length === 0}
          emptyText={t("skills.routes.noAvailableCandidate")}
          onValueChange={(value) => {
            const route = selectedRoute(llmCandidates, value);
            void save("llm", route);
          }}
          options={displayedLlmOptions}
          placeholder={t("skills.routes.selectAnExactRoute")}
          searchPlaceholder={t("skills.routes.searchModelOrConnection")}
          value={llmKey}
        />
      </label>
      <Button
        disabled={busy !== null || !llmKey}
        onClick={() => void save("llm", null)}
        type="button"
        variant="outline"
      >{t("skills.routes.clearQuestionAnsweringRoute")}</Button>
      {embeddingUnavailable ? <PageNotice details={embeddingDiagnostic?.reason} detailsLabel={t("skills.lua.errorDetails")} role="alert" tone="warning">
        {t("skills.routes.theSavedEmbeddingRoute")}
      </PageNotice> : null}
      <label className="lua-inference-route-row">
        <span>{t("skills.routes.embeddingsEmbeddingCreate")}</span>
        <Combobox
          ariaLabel={t("skills.routes.embeddingRoute")}
          disabled={busy !== null || embeddingCandidates.length === 0}
          emptyText={t("skills.routes.noAvailableCandidate")}
          onValueChange={(value) => {
            const route = selectedRoute(embeddingCandidates, value);
            void save("embedding", route);
          }}
          options={displayedEmbeddingOptions}
          placeholder={t("skills.routes.selectAnExactRoute")}
          searchPlaceholder={t("skills.routes.searchModelOrConnection")}
          value={embeddingKey}
        />
      </label>
      <Button
        disabled={busy !== null || !embeddingKey}
        onClick={() => void save("embedding", null)}
        type="button"
        variant="outline"
      >{t("skills.routes.clearEmbeddingRoute")}</Button>
    </section>
  );
}
