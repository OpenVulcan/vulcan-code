import { ExternalLink } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { localizedProviderDescription, localizedProviderName } from "@/components/features/settings/provider-presentation"
import { useI18n } from "@/i18n"
import type {
  CredentialBillingKind,
  PublicAiProviderCategory,
  PublicAiProviderDescriptor,
  PublicAiProviderServiceTag,
} from "@/types"

type Translate = ReturnType<typeof useI18n>["t"]

// serviceTagLabel names one service family.
// serviceTagLabel 命名一个服务类别。
export function serviceTagLabel(tag: PublicAiProviderServiceTag, t: Translate): string {
  switch (tag) {
    case "model": return t("settings.modelServices.tag.model")
    case "search": return t("settings.modelServices.tag.search")
    case "analysis": return t("settings.modelServices.tag.analysis")
  }
}

// providerCategoryLabel names the business relationship, or null for an unclassified provider.
// providerCategoryLabel 命名供应商业务关系；未分类时返回 null。
export function providerCategoryLabel(category: PublicAiProviderCategory, t: Translate): string | null {
  switch (category) {
    case "official": return t("settings.modelServices.category.official")
    case "cloud_maas": return t("settings.modelServices.category.cloudMaas")
    case "relay": return t("settings.modelServices.category.relay")
    case "unclassified": return null
  }
}

// billingLabel names one credential billing model with the account-card wording.
// billingLabel 使用账号卡片的措辞命名一种凭据计费方式。
function billingLabel(kind: CredentialBillingKind, t: Translate): string {
  switch (kind) {
    case "pay_as_you_go": return t("composer.accountCard.billingPayAsYouGo")
    case "token_plan": return t("composer.accountCard.billingTokenPlan")
    case "provider_managed": return t("composer.accountCard.billingProviderManaged")
  }
}

// providerSignupUrl prefers the disclosed referral link and falls back to the plain signup page.
// providerSignupUrl 优先使用已披露的邀请链接，否则使用普通注册页。
export function providerSignupUrl(provider: Pick<PublicAiProviderDescriptor, "promotion">): string | undefined {
  return provider.promotion?.referral_url ?? provider.promotion?.signup_url
}

// providerIntroFacts derives the facts the intro card lists from manifest-projected metadata only.
// providerIntroFacts 仅从清单投影的元数据推导介绍卡列出的事实。
// Parameters: provider is the descriptor and t resolves labels. Returns ordered display lines.
// 参数：provider 为描述，t 解析文案。返回有序的展示行。
export function providerIntroFacts(provider: PublicAiProviderDescriptor, t: Translate): string[] {
  const profiles = provider.service_entries.flatMap((entry) => entry.credential_profiles)
  const facts: string[] = []
  const billing = Array.from(new Set(profiles.map((profile) => profile.billing_kind)))
  if (billing.length > 0) {
    facts.push(t("settings.modelServices.intro.billing").replace("{kinds}", billing.map((kind) => billingLabel(kind, t)).join(t("global.listDelimiter"))))
  }
  if (profiles.some((profile) => profile.account_usage_query !== "unsupported")) {
    facts.push(t("settings.modelServices.intro.usageQuery"))
  }
  if (profiles.some((profile) => profile.credential_mode === "auth")) {
    facts.push(t("settings.modelServices.intro.signIn"))
  }
  if (provider.service_entries.length > 1) {
    facts.push(t("settings.modelServices.intro.entries").replace("{count}", String(provider.service_entries.length)))
  }
  return facts
}

// ProviderIntroCard is the hover introduction: what the provider offers, disclaimers and signup.
// ProviderIntroCard 是悬停介绍：供应商提供什么、免责声明与注册入口。
export function ProviderIntroCard(props: { provider: PublicAiProviderDescriptor }) {
  const { t } = useI18n()
  const { provider } = props
  const name = localizedProviderName(provider.id, provider.display_name, t)
  const category = providerCategoryLabel(provider.category, t)
  const facts = providerIntroFacts(provider, t)
  const signupUrl = providerSignupUrl(provider)
  return (
    <div className="model-services-intro">
      <div className="model-services-intro-head">
        <ProviderIcon category={provider.category} className="size-8 shrink-0" providerId={provider.id} />
        <div className="min-w-0">
          <strong>{name}</strong>
          <span className="model-services-intro-tags">
            {category ? <span className="model-services-chip" data-variant="category">{category}</span> : null}
            {(provider.service_tags ?? []).map((tag) => (
              <span className="model-services-chip" data-tag={tag} key={tag}>{serviceTagLabel(tag, t)}</span>
            ))}
          </span>
        </div>
      </div>
      <p className="model-services-intro-description">{localizedProviderDescription(provider.id, t)}</p>
      {facts.length > 0 ? (
        <ul className="model-services-intro-facts">
          {facts.map((fact) => <li key={fact}>{fact}</li>)}
        </ul>
      ) : null}
      <p className="model-services-intro-note">
        {t(provider.category === "relay" ? "settings.modelServices.intro.relayDisclaimer" : "settings.modelServices.intro.disclaimer")}
      </p>
      {signupUrl ? (
        <div className="model-services-intro-signup">
          <a href={signupUrl} rel="noreferrer" target="_blank">
            <ExternalLink aria-hidden="true" className="size-3.5" />
            {t("settings.modelServices.signup")}
          </a>
          {provider.promotion ? <small>{t(provider.promotion.disclosure_key)}</small> : null}
        </div>
      ) : null}
    </div>
  )
}
