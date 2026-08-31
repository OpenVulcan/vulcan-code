//! Shared exact non-negative decimal arithmetic for account-usage protocols.
//! 账号用量协议共用的精确非负十进制运算。

/// Subtracts two non-negative JSON decimal strings without floating-point loss.
/// 在不产生浮点精度损失的情况下对两个非负 JSON 十进制字符串做减法。
pub(super) fn subtract_non_negative_decimal(
    minuend: &str,
    subtrahend: &str,
) -> Result<String, String> {
    let (mut left, left_scale) = normalized_unsigned_decimal_digits(minuend)?;
    let (mut right, right_scale) = normalized_unsigned_decimal_digits(subtrahend)?;
    let scale = left_scale.max(right_scale);
    left.extend(std::iter::repeat_n(0, scale - left_scale));
    right.extend(std::iter::repeat_n(0, scale - right_scale));
    let width = left.len().max(right.len());
    left.splice(0..0, std::iter::repeat_n(0, width - left.len()));
    right.splice(0..0, std::iter::repeat_n(0, width - right.len()));
    if left < right {
        return Err(String::from(
            "OpenRouter credits response reports total_usage greater than total_credits.",
        ));
    }
    let mut borrow = 0_i16;
    let mut result = vec![0_u8; width];
    for index in (0..width).rev() {
        let mut digit = left[index] as i16 - borrow - right[index] as i16;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result[index] = digit as u8;
    }
    format_unsigned_decimal_digits(result, scale)
}

/// Adds non-negative decimal strings without floating-point conversion.
/// 在不转换为浮点数的情况下对非负十进制字符串求和。
///
/// # Parameters
/// 参数
/// `values` contains exact JSON-compatible decimal spellings.
/// `values` 包含精确的 JSON 兼容十进制拼写。
///
/// # Returns
/// 返回值
/// The exact normalized sum, or an error when any input is invalid.
/// 返回精确规范化总和；任一输入无效时返回错误。
pub(super) fn add_non_negative_decimals(values: &[&str]) -> Result<String, String> {
    if values.is_empty() {
        return Ok(String::from("0"));
    }
    let mut normalized = values
        .iter()
        .map(|value| normalized_unsigned_decimal_digits(value))
        .collect::<Result<Vec<_>, _>>()?;
    let scale = normalized
        .iter()
        .map(|(_, scale)| *scale)
        .max()
        .unwrap_or(0);
    for (digits, value_scale) in &mut normalized {
        digits.extend(std::iter::repeat_n(0, scale - *value_scale));
    }
    let width = normalized
        .iter()
        .map(|(digits, _)| digits.len())
        .max()
        .unwrap_or(1);
    let mut sum = vec![0_u16; width + 1];
    for (digits, _) in normalized {
        let offset = sum.len() - digits.len();
        for (index, digit) in digits.into_iter().enumerate() {
            sum[offset + index] += u16::from(digit);
        }
    }
    for index in (1..sum.len()).rev() {
        let carry = sum[index] / 10;
        sum[index] %= 10;
        sum[index - 1] += carry;
    }
    let digits = sum.into_iter().map(|digit| digit as u8).collect();
    format_unsigned_decimal_digits(digits, scale)
}

/// Normalizes one non-negative JSON number into base-10 digits and a fractional scale.
/// 将一个非负 JSON 数字归一化为十进制数字及小数位数。
pub(super) fn normalized_unsigned_decimal_digits(value: &str) -> Result<(Vec<u8>, usize), String> {
    if value.starts_with('-') {
        return Err(String::from(
            "OpenRouter credits values cannot be negative.",
        ));
    }
    let (mantissa, exponent) = match value.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent
                .parse::<i32>()
                .map_err(|_| String::from("OpenRouter credits exponent is invalid."))?,
        ),
        None => (value, 0),
    };
    let fractional_digits = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = mantissa
        .chars()
        .filter(|character| *character != '.')
        .map(|character| {
            character
                .to_digit(10)
                .map(|digit| digit as u8)
                .ok_or_else(|| String::from("OpenRouter credits value is not a JSON decimal."))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if digits.is_empty() {
        return Err(String::from("OpenRouter credits value is empty."));
    }
    let resulting_scale = fractional_digits as i32 - exponent;
    if resulting_scale < 0 {
        let mut expanded = digits;
        expanded.extend(std::iter::repeat_n(0, (-resulting_scale) as usize));
        Ok((expanded, 0))
    } else {
        Ok((digits, resulting_scale as usize))
    }
}

/// Formats normalized decimal digits while retaining exact fractional significance.
/// 格式化归一化十进制数字并保留精确的小数有效位。
fn format_unsigned_decimal_digits(mut digits: Vec<u8>, scale: usize) -> Result<String, String> {
    while digits.len() > scale.saturating_add(1) && digits.first() == Some(&0) {
        digits.remove(0);
    }
    if digits.len() <= scale {
        digits.splice(0..0, std::iter::repeat_n(0, scale + 1 - digits.len()));
    }
    let mut text = digits
        .into_iter()
        .map(|digit| char::from(b'0' + digit))
        .collect::<String>();
    if scale > 0 {
        text.insert(text.len() - scale, '.');
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests;
