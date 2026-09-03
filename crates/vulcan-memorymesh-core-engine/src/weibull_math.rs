//! Replays the frozen Go amd64 arithmetic used by VMM Weibull decisions without consulting local CPU capabilities.
//! 重放 VMM Weibull 决策使用的冻结 Go amd64 算术，不查询本机处理器能力。
// Adapted from Go math sources; copyright and redistribution notices are retained in ../THIRD_PARTY_NOTICES.md.
// 改编自 Go 数学源码；版权及再分发声明保留在 ../THIRD_PARTY_NOTICES.md。

// Exact comparisons, source parentheses and deliberate casts preserve the original numerical algorithm.
// 精确比较、来源括号顺序和已界定的类型转换用于保留原始数值算法。
#![allow(clippy::float_cmp, clippy::suboptimal_flops, clippy::imprecise_flops)]
#![allow(clippy::many_single_char_names, clippy::excessive_precision)]
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use vulcan_memorymesh_core_contracts::WeibullMathProfile;

/// Preserves the exact high/low logarithm split and polynomial coefficients in the source implementation.
/// 保留来源实现中对数高低拆分及多项式系数的精确位模式。
const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
const LOG_COEFFICIENTS: [f64; 7] = [
    f64::from_bits(0x3fe5_5555_5555_5593),
    f64::from_bits(0x3fd9_9999_9997_fa04),
    f64::from_bits(0x3fd2_4924_9422_9359),
    f64::from_bits(0x3fcc_71c5_1d8e_78af),
    f64::from_bits(0x3fc7_4664_96cb_03de),
    f64::from_bits(0x3fc3_9a09_d078_c69f),
    f64::from_bits(0x3fc2_f112_df3e_5244),
];

/// Preserves the source exponent reduction and small-argument Taylor constants.
/// 保留来源指数降阶和小参数泰勒常量。
const EXP_LN2_U: f64 = 0.693_147_180_559_662_956_511_601_805_686_950_683_593_75;
const EXP_LN2_L: f64 = 0.282_352_905_630_315_771_225_884_481_750_134_360_255_254_120_68e-12;
const EXP_COEFFICIENTS: [f64; 8] = [
    2.480_158_730_158_730_158_7e-5,
    1.984_126_984_126_984_127_0e-4,
    1.388_888_888_888_888_888_9e-3,
    8.333_333_333_333_333_333_3e-3,
    4.166_666_666_666_666_666_7e-2,
    1.666_666_666_666_666_666_7e-1,
    0.5,
    1.0,
];

/// Evaluates the exact selected exponent sequence; finite, infinite and NaN inputs follow the source branches.
/// 计算精确选定的指数序列；有限数、无穷及非数输入遵循来源分支。
pub fn exp(value: f64, profile: WeibullMathProfile) -> f64 {
    if value == f64::NEG_INFINITY {
        return 0.0;
    }
    if !value.is_finite() {
        return value;
    }
    if value > 7.097_827_128_933_84e2 {
        return f64::INFINITY;
    }
    // The source CVTSD2SL rounds to nearest-even; its out-of-range negative result is the minimum integer.
    // 来源 CVTSD2SL 按最近偶数舍入，负方向越界结果为最小整数；下溢分支在求多项式前即可确定。
    let exponent = (value * std::f64::consts::LOG2_E).round_ties_even() as i32;
    if exponent < -1075 {
        return 0.0;
    }
    let n = f64::from(exponent);
    let fused = profile == WeibullMathProfile::GoAmd64FmaV1;
    let reduced = if fused {
        (-EXP_LN2_L).mul_add(n, (-EXP_LN2_U).mul_add(n, value))
    } else {
        (value - n * EXP_LN2_U) - n * EXP_LN2_L
    } * 0.0625;
    let mut polynomial = EXP_COEFFICIENTS[0];
    for coefficient in &EXP_COEFFICIENTS[1..] {
        polynomial = if fused {
            polynomial.mul_add(reduced, *coefficient)
        } else {
            polynomial * reduced + coefficient
        };
    }
    let mut fraction = reduced * polynomial;
    for _ in 0..3 {
        fraction *= 2.0 + fraction;
    }
    fraction = if fused {
        fraction.mul_add(2.0 + fraction, 1.0)
    } else {
        fraction * (2.0 + fraction) + 1.0
    };
    // Reconstruct subnormal exponents in the same two multiplications as the assembly, not a replacement libm call.
    // 按汇编相同的两次乘法重建次正规指数，不替换成另一数学库调用。
    let biased = exponent + 1023;
    if biased >= 2047 {
        return f64::INFINITY;
    }
    if biased <= 0 {
        return (fraction * f64::from_bits(((biased + 1022) as u64) << 52)) * f64::MIN_POSITIVE;
    }
    fraction * f64::from_bits((biased as u64) << 52)
}

/// Evaluates the source amd64 logarithm used by the fractional-power path, retaining its exact bit reduction.
/// 计算分数次幂路径使用的来源 amd64 对数，保留其精确位降阶方式。
fn log(value: f64) -> f64 {
    if value == 0.0 {
        return f64::NEG_INFINITY;
    }
    if value < 0.0 {
        return f64::NAN;
    }
    if !value.is_finite() {
        return value;
    }
    let bits = value.to_bits();
    let mut fraction = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000);
    let mut exponent = ((bits >> 52) & 0x7ff) as i32 - 1022;
    // CMPNLT in the source assembly includes equality; do not substitute the generic Go path's strict comparison.
    // 来源汇编的 CMPNLT 包含相等情形，不替换为通用 Go 路径的严格比较。
    if fraction <= std::f64::consts::FRAC_1_SQRT_2 {
        fraction *= 2.0;
        exponent -= 1;
    }
    let f = fraction - 1.0;
    let k = f64::from(exponent);
    let s = f / (2.0 + f);
    let s2 = s * s;
    let s4 = s2 * s2;
    let [l1, l2, l3, l4, l5, l6, l7] = LOG_COEFFICIENTS;
    let t1 = s2 * (l1 + s4 * (l3 + s4 * (l5 + s4 * l7)));
    let t2 = s4 * (l2 + s4 * (l4 + s4 * l6));
    let remainder = t1 + t2;
    let half_square = 0.5 * f * f;
    k * LN2_HI - ((half_square - (s * (half_square + remainder) + k * LN2_LO)) - f)
}

/// Reproduces the source frexp normalization and returns a fraction with its binary exponent.
/// 复现来源 frexp 归一化，返回尾数及其二进制指数。
fn frexp(mut value: f64) -> (f64, i32) {
    if value == 0.0 || !value.is_finite() {
        return (value, 0);
    }
    let adjustment = if value.abs() < f64::MIN_POSITIVE {
        value *= 4_503_599_627_370_496.0;
        -52
    } else {
        0
    };
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1022 + adjustment;
    (
        f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | 0x3fe0_0000_0000_0000),
        exponent,
    )
}

/// Reproduces the source ldexp's final rounding, including subnormals and signed overflow/underflow.
/// 复现来源 ldexp 的最终舍入，包括次正规数及带符号上溢/下溢。
fn ldexp(mut fraction: f64, mut exponent: i32) -> f64 {
    if fraction == 0.0 || !fraction.is_finite() {
        return fraction;
    }
    if fraction.abs() < f64::MIN_POSITIVE {
        fraction *= 4_503_599_627_370_496.0;
        exponent -= 52;
    }
    let bits = fraction.to_bits();
    exponent += ((bits >> 52) & 0x7ff) as i32 - 1023;
    if exponent < -1075 {
        return 0.0_f64.copysign(fraction);
    }
    if exponent > 1023 {
        return f64::INFINITY.copysign(fraction);
    }
    let multiplier = if exponent < -1022 {
        exponent += 53;
        1.0 / 9_007_199_254_740_992.0
    } else {
        1.0
    };
    multiplier * f64::from_bits((bits & 0x800f_ffff_ffff_ffff) | (((exponent + 1023) as u64) << 52))
}

/// Raises a nonnegative base to a nonnegative exponent using the source fraction/integer decomposition.
/// 使用来源的分数/整数分解对非负底数求非负次幂；该定义域覆盖已校验 Weibull 参数。
pub fn pow_nonnegative(base: f64, power: f64, profile: WeibullMathProfile) -> f64 {
    if power == 0.0 || base == 1.0 {
        return 1.0;
    }
    if power == 1.0 {
        return base;
    }
    if base.is_nan() || power.is_nan() || base < 0.0 || power < 0.0 {
        return f64::NAN;
    }
    if base == 0.0 {
        return 0.0;
    }
    if base == f64::INFINITY {
        return f64::INFINITY;
    }
    if power == f64::INFINITY {
        return if base < 1.0 { 0.0 } else { f64::INFINITY };
    }
    if power == 0.5 {
        return base.sqrt();
    }
    let mut integer = power.trunc();
    let mut fraction = power - integer;
    if integer >= 9_223_372_036_854_775_808.0 {
        return if base < 1.0 { 0.0 } else { f64::INFINITY };
    }
    let mut answer = 1.0;
    let mut answer_exponent = 0;
    if fraction != 0.0 {
        if fraction > 0.5 {
            fraction -= 1.0;
            integer += 1.0;
        }
        answer = exp(fraction * log(base), profile);
    }
    // Bounded exponent accumulation mirrors the source overflow guard before each squaring.
    // 有界指数累积在每次平方之前复现来源上溢防护；整数转换已由上面的二进制范围检查约束。
    let (mut base_fraction, mut base_exponent) = frexp(base);
    let mut bits = integer as i64;
    while bits != 0 {
        if !(-4096..=4096).contains(&base_exponent) {
            answer_exponent += base_exponent;
            break;
        }
        if bits & 1 == 1 {
            answer *= base_fraction;
            answer_exponent += base_exponent;
        }
        base_fraction *= base_fraction;
        base_exponent <<= 1;
        if base_fraction < 0.5 {
            base_fraction += base_fraction;
            base_exponent -= 1;
        }
        bits >>= 1;
    }
    ldexp(answer, answer_exponent)
}

/// Evaluates the source log1p polynomial with its compensated argument reduction and exact parentheses.
/// 使用带补偿的参数降阶和精确括号顺序计算来源 log1p 多项式。
#[allow(clippy::too_many_lines)]
pub fn log1p(value: f64) -> f64 {
    if value < -1.0 || value.is_nan() {
        return f64::NAN;
    }
    if value == -1.0 {
        return f64::NEG_INFINITY;
    }
    if value == f64::INFINITY {
        return value;
    }
    let absolute = value.abs();
    let mut f = 0.0;
    let mut mantissa = 0_u64;
    let mut exponent = 1;
    if absolute < f64::from_bits(0x3fda_8279_99fc_ef34) {
        if absolute < f64::from_bits(0x3e20_0000_0000_0000) {
            if absolute < 1.0 / 18_014_398_509_481_984.0 {
                return value;
            }
            return value - value * value * 0.5;
        }
        if value > f64::from_bits(0xbfd2_bec3_3301_8866) {
            exponent = 0;
            f = value;
            mantissa = 1;
        }
    }
    let mut correction = 0.0;
    if exponent != 0 {
        let mut u;
        if absolute < 9_007_199_254_740_992.0 {
            u = 1.0 + value;
            mantissa = u.to_bits();
            exponent = (mantissa >> 52) as i32 - 1023;
            correction = if exponent > 0 {
                1.0 - (u - value)
            } else {
                value - (u - 1.0)
            };
            correction /= u;
        } else {
            u = value;
            mantissa = u.to_bits();
            exponent = (mantissa >> 52) as i32 - 1023;
        }
        mantissa &= 0x000f_ffff_ffff_ffff;
        if mantissa < 0x0006_a09e_667f_3bcd {
            u = f64::from_bits(mantissa | 0x3ff0_0000_0000_0000);
        } else {
            exponent += 1;
            u = f64::from_bits(mantissa | 0x3fe0_0000_0000_0000);
            mantissa = (0x0010_0000_0000_0000 - mantissa) >> 2;
        }
        f = u - 1.0;
    }
    let half_square = 0.5 * f * f;
    let k = f64::from(exponent);
    if mantissa == 0 {
        if f == 0.0 {
            return if exponent == 0 {
                0.0
            } else {
                correction += k * LN2_LO;
                k * LN2_HI + correction
            };
        }
        let remainder = half_square * (1.0 - 0.666_666_666_666_666_66 * f);
        if exponent == 0 {
            return f - remainder;
        }
        return k * LN2_HI - ((remainder - (k * LN2_LO + correction)) - f);
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let [l1, l2, l3, l4, l5, l6, l7] = LOG_COEFFICIENTS;
    let remainder = z * (l1 + z * (l2 + z * (l3 + z * (l4 + z * (l5 + z * (l6 + z * l7))))));
    if exponent == 0 {
        return f - (half_square - s * (half_square + remainder));
    }
    k * LN2_HI - ((half_square - (s * (half_square + remainder) + (k * LN2_LO + correction))) - f)
}
