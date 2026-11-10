// Parse numeric GPT IDs, including short hyphenated revisions but excluding date suffixes from the version.
// 解析数字 GPT 标识，支持短连字符小版本号，同时避免将日期后缀当作版本号。
const gptVersion = /^gpt-(\d+)(?:\.(\d+)|-(\d{1,2})(?=-|$))?/;

// minimumGptPricingVersion is the first numeric GPT version retained in the fee table.
// minimumGptPricingVersion 是费用表保留的首个数字 GPT 版本。
export const minimumGptPricingVersion = { major: 5, minor: 4 };

// namedGptPricing begins where the family and variant are both required for an independent price.
// 从 namedGptPricing 指定的代际开始，只有版本和具体型号都明确才提供独立价格。
export const namedGptPricing = { major: 5, minor: 6, variants: ["-sol", "-terra", "-luna"], astraMajor: 6 };

// Only the numeric GPT namespace uses this set to identify reasoning effort.
// 仅在数字版本的 GPT 命名中使用这组后缀识别思考档位。
export const gptReasoningEfforts = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

// These names identify actual models even though their last segment is an effort name.
// 这些名称是实际型号，即使最后一段也可用作思考档位名。
const gptModelExceptions = new Set(["gpt-5.1-codex-max"]);

// Return whether a numeric GPT ID is older than the retained version, including historical spellings.
// 判断数字 GPT 标识是否早于费用表保留版本，同时识别历史写法。
export function isRetiredGptModel(id) {
  // version reads the full major number and either a dotted minor or a short hyphenated minor.
  // version 读取完整主版本号，以及点号小版本号或较短的连字符小版本号。
  const version = gptVersion.exec(id);
  if (!version) return false;
  // A bare major or a date suffix does not advance the minor version.
  // 裸主版本号或日期后缀都不会改变小版本号。
  const major = Number(version[1]);
  const minor = Number(version[2] ?? version[3] ?? 0);
  return major < minimumGptPricingVersion.major
    || (major === minimumGptPricingVersion.major && minor < minimumGptPricingVersion.minor);
}

// Return the normalized model name for a GPT effort alias, or null for a model name.
// 如果 id 是 GPT 思考档位别名，返回去掉档位的型号；如果本身是型号，返回 null。
export function gptReasoningBase(id) {
  if (!gptVersion.test(id) || gptModelExceptions.has(id)) return null;
  // Match a terminal segment only; for example, Codex Max can itself carry High.
  // 只匹配末尾的完整分段；例如 Codex Max 型号本身还可以选择 High 档位。
  const split = id.lastIndexOf("-");
  return gptReasoningEfforts.includes(id.slice(split + 1)) ? id.slice(0, split) : null;
}

// Return whether id is eligible to own an independent reference price.
// 判断 id 是否可以拥有一份独立参考价。
export function isPricingModel(id) {
  if (isRetiredGptModel(id) || gptReasoningBase(id) !== null) return false;
  // Modern GPT generations price named models individually; a bare version cannot select a price.
  // 新版 GPT 对具体型号分别定价；裸版本号无法确定应使用哪个价格。
  const version = gptVersion.exec(id);
  if (!version) return true;
  // major and minor decide when a named model is required.
  // major 和 minor 决定何时必须使用具名型号。
  const major = Number(version[1]);
  const minor = Number(version[2] ?? version[3] ?? 0);
  if (major < namedGptPricing.major || (major === namedGptPricing.major && minor < namedGptPricing.minor)) return true;
  // variant is the complete segment following the version, including its separator.
  // variant 是版本号之后含分隔符的完整型号后缀。
  const variant = id.slice(version[0].length);
  return namedGptPricing.variants.includes(variant) || (major === namedGptPricing.astraMajor && variant === "-astra");
}

// Return the supported hyphenated version spelling for a canonical ID, or null when unchanged.
// 返回主标识支持的版本号连字符写法；无需转换时返回 null。
export function hyphenatedGptAlias(id) {
  // version must include an actual dotted minor number, not a digit in a model suffix.
  // version 必须含有点号分隔的小版本号，不能匹配型号后缀里的数字。
  const version = gptVersion.exec(id);
  if (!version?.[2] || Number(version[1]) < 5 || (Number(version[1]) === 5 && Number(version[2]) < 5)) return null;
  return id.replace(".", "-");
}
