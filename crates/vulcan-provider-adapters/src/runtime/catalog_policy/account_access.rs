//! Exact account-plan and positive-quota admission for declared model catalogs.
//! 已声明模型目录的精确账号套餐与正额度准入。

use vulcan_ai_contract::AccountUsageSnapshot;

/// Reports whether one normalized non-negative decimal string is greater than zero.
/// 判断一个已归一化非负十进制字符串是否大于零。
///
/// # Parameters
/// 参数
/// `value` is provider-normalized account-usage evidence.
/// `value` 是供应商归一化后的账号用量证据。
///
/// # Returns
/// 返回值
/// `true` when the decimal mantissa contains a non-zero digit and is not negative.
/// 当十进制尾数包含非零数字且不是负数时返回 `true`。
fn normalized_usage_decimal_is_positive(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') {
        return false;
    }
    let (mantissa, exponent) = value
        .split_once(['e', 'E'])
        .map_or((value, None), |(mantissa, exponent)| {
            (mantissa, Some(exponent))
        });
    if exponent.is_some_and(|exponent| {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return false;
    }
    let mantissa = mantissa.strip_prefix('+').unwrap_or(mantissa);
    let mut decimal_points = 0_u8;
    let mut digit_count = 0_usize;
    let mut positive = false;
    for byte in mantissa.bytes() {
        match byte {
            b'.' => decimal_points = decimal_points.saturating_add(1),
            b'0' => digit_count = digit_count.saturating_add(1),
            b'1'..=b'9' => {
                digit_count = digit_count.saturating_add(1);
                positive = true;
            }
            _ => return false,
        }
    }
    decimal_points <= 1 && digit_count > 0 && positive
}

/// Reports whether configured positive quota evidence unlocks the complete declared catalog.
/// 判断已配置的正数额度证据是否解锁完整已声明目录。
///
/// # Parameters
/// 参数
/// `policy` declares quota-window identities and `usage` is one credential-scoped observation.
/// `policy` 声明额度窗口身份，`usage` 是一次凭据作用域观测。
///
/// # Returns
/// 返回值
/// `true` when at least one configured, available window has positive remaining quota.
/// 当至少一个已配置且可用的窗口具有正数剩余额度时返回 `true`。
pub(super) fn account_usage_unlocks_catalog(
    policy: &crate::AccountModelAccessPolicy,
    usage: &AccountUsageSnapshot,
) -> bool {
    policy
        .unrestricted_when_quota_remaining
        .iter()
        .any(|window_id| {
            usage.quota_windows.iter().any(|window| {
                window.available
                    && window.id == *window_id
                    && window
                        .remaining
                        .as_deref()
                        .is_some_and(normalized_usage_decimal_is_positive)
            })
        })
}

/// Resolves the exact configured plan identity from one account-usage observation.
/// 从一次账号用量观测中解析精确配置的套餐身份。
///
/// # Parameters
/// 参数
/// `policy` selects the identity field and `usage` owns provider evidence.
/// `policy` 选择身份字段，`usage` 持有供应商证据。
///
/// # Returns
/// 返回值
/// The selected non-empty plan identity, or `None` when the provider supplied no evidence.
/// 返回选中的非空套餐身份；供应商未提供证据时返回 `None`。
fn account_plan_identity<'a>(
    policy: &crate::AccountModelAccessPolicy,
    usage: &'a AccountUsageSnapshot,
) -> Option<&'a str> {
    let identity = match policy.plan_identity_field {
        crate::AccountPlanIdentityField::PlanName => usage.plan_name.as_deref(),
        crate::AccountPlanIdentityField::PlanId => usage.plan_id.as_deref(),
    };
    identity.map(str::trim).filter(|value| !value.is_empty())
}

/// Applies one Manifest rule's account-plan allowlist and account-access groups.
/// 应用一条 Manifest 规则的账号套餐白名单与账号访问分组。
///
/// # Parameters
/// 参数
/// `rule` owns model access metadata, `policy` owns credential plan rules, `usage` supplies
/// account evidence, and `model_id` is the exact effective-catalog identity.
/// `rule` 持有模型访问元数据，`policy` 持有凭据套餐规则，`usage` 提供账号证据，`model_id`
/// 是精确生效目录身份。
///
/// # Returns
/// 返回值
/// `true` when every configured access dimension admits the model.
/// 当所有已配置访问维度均允许该模型时返回 `true`。
pub(super) fn model_rule_allows_account_access(
    rule: &crate::provider_manifest::ResolvedRuleInstance,
    policy: &crate::AccountModelAccessPolicy,
    usage: &AccountUsageSnapshot,
    model_id: &str,
) -> bool {
    let plan_name_allowed = usage.plan_name.as_deref().is_none_or(|plan_name| {
        rule.allowed_account_plan_names.is_empty()
            || rule
                .allowed_account_plan_names
                .iter()
                .any(|allowed| allowed == plan_name)
    });
    if !plan_name_allowed {
        return false;
    }
    let Some(plan_identity) = account_plan_identity(policy, usage) else {
        return true;
    };
    let Some(plan_rule) = policy.plan_rules.get(plan_identity) else {
        return true;
    };
    if plan_rule.blocked_model_ids.contains(model_id) {
        return false;
    }
    rule.account_access_groups.is_empty()
        || rule
            .account_access_groups
            .iter()
            .any(|group| plan_rule.allowed_model_groups.contains(group))
}

#[cfg(test)]
mod tests;
