import type * as React from "react"
import { ExternalLink } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import {
  localizedProviderDescription,
  localizedProviderName,
  localizedServiceEntryPresentation,
} from "@/components/features/settings/provider-presentation"
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
export function billingLabel(kind: CredentialBillingKind, t: Translate): string {
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

// ProviderIntroCardProps names the provider and an optional footer action, such as connect or view accounts.
// ProviderIntroCardProps 指定供应商与可选的底部操作，例如连接或查看账号。
export interface ProviderIntroCardProps {
  provider: PublicAiProviderDescriptor
  action?: React.ReactNode
  // compact drops the heading when the card sits under a page header that already names the provider.
  // compact 在卡片位于已显示供应商名称的页头下方时省略标题。
  compact?: boolean
}

// ProviderIntroCard is the provider introduction: what it offers, how to connect, disclaimers and signup.
// ProviderIntroCard 是供应商介绍：提供什么、如何接入、免责声明与注册入口。
export function ProviderIntroCard(props: ProviderIntroCardProps) {
  const { t } = useI18n()
  const { provider } = props
  const name = localizedProviderName(provider.id, provider.display_name, t)
  const category = providerCategoryLabel(provider.category, t)
  const facts = providerIntroFacts(provider, t)
  const signupUrl = providerSignupUrl(provider)
  return (
    <div className="vk-ms-intro">
      {props.compact ? null : (
        <div className="vk-ms-intro-head">
          <ProviderIcon category={provider.category} size={32} providerId={provider.id} />
          <strong>{name}</strong>
          {category ? <span className="vk-ms-chip" data-variant="category">{category}</span> : null}
        </div>
      )}
      <ProviderTagChips provider={provider} />
      <p className="vk-ms-intro-description">{localizedProviderDescription(provider.id, t)}</p>
      {facts.length > 0 ? (
        <div className="vk-ms-intro-section">
          <span>{t("settings.modelServices.intro.features")}</span>
          <ul>{facts.map((fact) => <li key={fact}>{fact}</li>)}</ul>
        </div>
      ) : null}
      {provider.service_entries.length > 0 ? (
        <div className="vk-ms-intro-section">
          <span>{t("settings.modelServices.intro.entryList")}</span>
          <div className="vk-ms-chip-row">
            {provider.service_entries.map((entry) => (
              <span className="vk-ms-chip" key={entry.id}>{localizedServiceEntryPresentation(provider.id, entry, t).title}</span>
            ))}
          </div>
        </div>
      ) : null}
      <p className="vk-ms-intro-note">
        {t(provider.category === "relay" ? "settings.modelServices.intro.relayDisclaimer" : "settings.modelServices.intro.disclaimer")}
      </p>
      {provider.promotion?.referral_url ? (
        <p className="vk-ms-intro-affiliate">
          <strong>{t("settings.modelServices.affiliate")}</strong>
          <span>{t(provider.promotion.disclosure_key)}</span>
        </p>
      ) : null}
      {signupUrl || props.action ? (
        <div className="vk-ms-intro-foot">
          {signupUrl ? (
            <a href={signupUrl} rel="noreferrer" target="_blank">
              {t("settings.modelServices.signup")}
              <ExternalLink aria-hidden="true" className="vk-icon" />
            </a>
          ) : <span />}
          {props.action ?? null}
        </div>
      ) : null}
    </div>
  )
}

// ProviderTagChips renders the service tags in their tag colors.
// ProviderTagChips 以各自的标签颜色渲染服务标签。
export function ProviderTagChips(props: { provider: Pick<PublicAiProviderDescriptor, "service_tags">; small?: boolean }) {
  const { t } = useI18n()
  const tags = props.provider.service_tags ?? []
  if (tags.length === 0) return null
  return (
    <span className="vk-ms-chip-row" data-size={props.small ? "small" : undefined}>
      {tags.map((tag) => <span className="vk-ms-chip" data-tag={tag} key={tag}>{serviceTagLabel(tag, t)}</span>)}
    </span>
  )
}

// AffiliateChip marks a provider whose signup link is a disclosed referral link.
// AffiliateChip 标记注册链接为已披露邀请链接的供应商。
export function AffiliateChip(props: { provider: Pick<PublicAiProviderDescriptor, "promotion"> }) {
  const { t } = useI18n()
  const promotion = props.provider.promotion
  if (!promotion?.referral_url) return null
  return <span className="vk-ms-chip" data-variant="affiliate" title={t(promotion.disclosure_key)}>{t("settings.modelServices.affiliate")}</span>
}
