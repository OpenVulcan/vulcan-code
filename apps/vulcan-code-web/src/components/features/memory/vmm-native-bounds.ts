// VMM_PROFILE_LIFETIME_MAX_MS is the native host's upper bound for every profile lifetime field (3650 days).
// VMM_PROFILE_LIFETIME_MAX_MS 是原生宿主对每个画像生命周期字段的上限（3650 天）。
export const VMM_PROFILE_LIFETIME_MAX_MS = 3650 * 86_400_000

// VMM_RETENTION_BOUNDS declares the closed native bounds shared by the global and project retention forms.
// VMM_RETENTION_BOUNDS 声明全局与项目数据保留表单共享的原生封闭边界。
export const VMM_RETENTION_BOUNDS = {
  turnHotWindowSize: { min: 1, max: 4_294_967_295 },
  sessionIdleMs: { min: 1, max: 9_223_372_036_854 },
  trashRetentionMs: { min: 1, max: 9_223_372_036_854 },
  protectedPriorityFloor: { min: 0, max: 100 },
  protectedMemoryLevelFloor: { min: 1, max: 4 },
} as const

// VMM_FRAGMENT_TOKEN_BOUNDS is the accepted range of the text policy's maximum fragment tokens.
// VMM_FRAGMENT_TOKEN_BOUNDS 是文本策略最大片段 Token 数的可接受范围。
export const VMM_FRAGMENT_TOKEN_BOUNDS = { min: 256, max: 131_072 } as const
