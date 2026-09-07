import type { PublicAiEmbeddingCapabilities } from "@/types";

// Legal explicit UI controls; provider-default values never define this domain.
// 合法明确界面控件；供应商默认值绝不定义此取值域。
export type VceDimensionDomain = { kind: "discrete"; values: number[] }
  | { kind: "range"; minimum: number; maximum: number };

// Check the shared positive-u32 dimension range without accepting fractions or NaN.
// 检查共享正 u32 维度范围，不接受小数或非数值。
// value is untrusted capability/user input; returns whether it is representable in the host contract.
// value 为不可信能力或用户输入；返回它能否由宿主契约表示。
function positiveDimension(value: number): boolean {
  return Number.isInteger(value) && value > 0 && value <= 4_294_967_295;
}

// Derive explicit discrete/range controls using the same declared constraints as host preflight.
// 使用与宿主预检相同的声明约束，推导明确离散或范围控件。
// capabilities is a model contract; returns null for unknown or contradictory declarations.
// capabilities 为模型契约；声明未知或矛盾时返回空值。
export function vceDimensionDomain(capabilities: PublicAiEmbeddingCapabilities): VceDimensionDomain | null {
  // Missing one bound means the positive-u32 boundary in the shared capability contract.
  // 共享能力契约中缺失单侧边界表示正 u32 边界。
  const minimum = capabilities.minimum_dimensions ?? 1;
  const maximum = capabilities.maximum_dimensions ?? 4_294_967_295;
  if (!positiveDimension(minimum) || !positiveDimension(maximum) || maximum < minimum
    || capabilities.dimensions.some((value) => !positiveDimension(value))) return null;
  if (capabilities.dimensions.length > 0) {
    // All simultaneous bounds apply, so excluded discrete choices cannot appear in the picker.
    // 同时应用全部边界，因此选择器不能出现已排除的离散选项。
    const values = [...new Set(capabilities.dimensions)]
      .filter((value) => value >= minimum && value <= maximum).sort((left, right) => left - right);
    return values.length > 0 ? { kind: "discrete", values } : null;
  }
  return capabilities.minimum_dimensions !== undefined || capabilities.maximum_dimensions !== undefined
    ? { kind: "range", minimum, maximum } : null;
}

// Validate one explicit UI value; the backend independently repeats authoritative validation.
// 校验一个明确界面取值；后端独立重复权威校验。
// domain is declared capability metadata and value is user input; returns no fallback dimension.
// domain 为声明能力元数据，value 为用户输入；不返回兜底维度。
export function vceDimensionSupported(domain: VceDimensionDomain, value: number): boolean {
  return positiveDimension(value) && (domain.kind === "discrete" ? domain.values.includes(value)
    : value >= domain.minimum && value <= domain.maximum);
}

// Suggest an explicit value while preserving a valid deliberate selection; never writes configuration.
// 保留有效明确选择并建议明确取值；绝不写配置。
// domain contains legal values, preserved is optional user intent; returns preserved/1024/up/down.
// domain 包含合法值，preserved 为可选用户意图；依次返回已有值、1024、向上或向下档位。
export function preferredVceDimension(domain: VceDimensionDomain, preserved?: number): number {
  if (preserved !== undefined && vceDimensionSupported(domain, preserved)) return preserved;
  if (domain.kind === "range") return Math.max(domain.minimum, Math.min(1024, domain.maximum));
  return domain.values.find((value) => value >= 1024) ?? domain.values[domain.values.length - 1];
}
