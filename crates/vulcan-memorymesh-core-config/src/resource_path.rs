//! Resource-only URL path projection matching external textutil's net/url usage.
//! 与外置 textutil 的 net/url 用法一致、仅供资源处理的 URL 路径投影。
//! These helpers never open a URL or resolve a local filesystem path.
//! 这些辅助函数不会打开 URL，也不会解析本地文件系统路径。

use std::net::Ipv6Addr;

/// Returns the decoded path used for resource names/extensions, or None for malformed input.
/// 返回资源名称及扩展名使用的解码路径；输入格式错误时返回 None。
/// Rootless scheme URLs are opaque, including blob and Windows drive-like targets.
/// 无根路径的带协议 URL 属于不透明地址，包括 blob 和类似 Windows 盘符的目标。
pub(super) fn resource_url_path(target: &str) -> Option<String> {
    // Fragments are decoded independently; malformed fragment escapes invalidate Go URL.Parse.
    // 片段独立解码；错误的片段转义会导致 Go URL.Parse 失败。
    let (url, fragment) = target.split_once('#').unwrap_or((target, ""));
    decode(fragment, DecodeMode::Path)?;
    if url.bytes().any(|byte| byte < 32 || byte == 127) {
        return None;
    }
    // Scan only the leading scheme grammar, as external net/url.getScheme does.
    // 与外置 net/url.getScheme 一样，仅扫描开头的协议语法。
    let mut scheme = "";
    // Retain the exact remainder until query and authority boundaries are established.
    // 在确认查询及权威组件边界前保留准确的剩余原文。
    let mut rest = url;
    for (index, byte) in url.bytes().enumerate() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' => {}
            b'0'..=b'9' | b'+' | b'-' | b'.' if index > 0 => {}
            b':' if index == 0 => return None,
            b':' => {
                scheme = &url[..index];
                rest = &url[index + 1..];
                break;
            }
            _ => break,
        }
    }
    rest = rest.split_once('?').map_or(rest, |(path, _)| path);
    if !rest.starts_with('/') {
        if !scheme.is_empty() {
            return Some(String::new());
        }
        if rest
            .split('/')
            .next()
            .is_some_and(|segment| segment.contains(':'))
        {
            return None;
        }
    }
    if rest.starts_with("//") && (!scheme.is_empty() || !rest.starts_with("///")) {
        // Authority validation matters: external extraction returns no name on parse failure.
        // 权威组件校验不可省略：外置解析失败时不返回资源名称。
        let (authority, path) = rest[2..]
            .split_once('/')
            .map_or((&rest[2..], ""), |(authority, _)| {
                (authority, &rest[2 + authority.len()..])
            });
        validate_authority(scheme, authority)?;
        rest = path;
    }
    decode(rest, DecodeMode::Path)
}

/// Specifies external URL component escape rules; this is not a networking policy.
/// 指定外置 URL 组件的转义规则；这不是网络访问策略。
#[derive(Clone, Copy)]
enum DecodeMode {
    /// Accepts every correctly escaped byte for paths, userinfo, and fragments.
    /// 接受路径、用户信息及片段中所有正确转义的字节。
    Path,
    /// Rejects ASCII host escapes except the percent sign.
    /// 拒绝除百分号之外的 ASCII 主机转义。
    Host,
    /// Permits the external IPv6 zone escape subset.
    /// 允许外置 IPv6 区域标识的转义子集。
    Zone,
}

/// Decodes percent escapes without interpreting plus signs; invalid component bytes return None.
/// 解码百分号转义，不解释加号；组件字节无效时返回 None。
fn decode(text: &str, mode: DecodeMode) -> Option<String> {
    // Use bytes because percent sequences encode bytes, not Unicode scalar values.
    // 使用字节，因为百分号序列编码的是字节，而非 Unicode 字符。
    let bytes = text.as_bytes();
    // Retain decoded bytes until the whole component is validated.
    // 在整个组件完成校验前保存解码字节。
    let mut decoded = Vec::with_capacity(bytes.len());
    // Advance by one literal byte or three percent-escape bytes.
    // 按一个字面字节或三个百分号转义字节推进。
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            // A missing or non-hex nibble is a parse error, never a guessed URL repair.
            // 缺失或非十六进制半字节属于解析错误，绝不猜测修复 URL。
            let high = char::from(*bytes.get(index + 1)?).to_digit(16)? as u8;
            // Read the second nibble under the same validated grammar.
            // 按同一已校验语法读取第二个半字节。
            let low = char::from(*bytes.get(index + 2)?).to_digit(16)? as u8;
            // Compose the decoded byte before checking component restrictions.
            // 在检查组件限制前组合解码字节。
            let value = high * 16 + low;
            match mode {
                DecodeMode::Host if value < 128 && value != b'%' => return None,
                DecodeMode::Zone if value != b'%' && value != b' ' && !allowed_host_byte(value) => {
                    return None;
                }
                _ => {}
            }
            decoded.push(value);
            index += 3;
        } else {
            if !matches!(mode, DecodeMode::Path)
                && bytes[index] < 128
                && !allowed_host_byte(bytes[index])
            {
                return None;
            }
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    // External strings can carry invalid UTF-8; its JSON boundary replaces each invalid rune.
    // 外置字符串可携带无效 UTF-8；其 JSON 边界会替换每个无效字符。
    let mut output = String::new();
    // Decode valid prefixes and replace invalid bytes individually, as Go's rune traversal does.
    // 解码合法前缀并逐字节替换无效内容，与 Go 字符遍历一致。
    let mut remaining = decoded.as_slice();
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(valid) => {
                output.push_str(valid);
                break;
            }
            Err(error) => {
                output.push_str(
                    std::str::from_utf8(&remaining[..error.valid_up_to()])
                        .expect("validated UTF-8 prefix"),
                );
                output.push('\u{fffd}');
                remaining = &remaining[error.valid_up_to() + 1..];
            }
        }
    }
    Some(output)
}

/// Reports Go's literal host/zone byte allowlist, including its historical delimiters.
/// 返回 Go 字面主机及区域字节允许列表，包括其历史分隔符。
fn allowed_host_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-_.~!$&'()*+,;=:[]<>\"".contains(&byte)
}

/// Validates userinfo, host and optional port; returns unit only for an accepted authority.
/// 校验用户信息、主机及可选端口；仅在权威组件合法时返回空值。
fn validate_authority(scheme: &str, authority: &str) -> Option<()> {
    // The final @ is authoritative; external net/url explicitly allows earlier @ in userinfo.
    // 最后一个 @ 是权威分界；外置 net/url 明确允许用户信息内出现更早的 @。
    let host = if let Some((userinfo, host)) = authority.rsplit_once('@') {
        if !userinfo
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._:~!$&'()*+,;=%@".contains(&byte))
        {
            return None;
        }
        decode(userinfo, DecodeMode::Path)?;
        host
    } else {
        authority
    };
    if let Some(open) = host.rfind('[') {
        if open != 0 {
            return None;
        }
        // IPv6 literals have a mandatory closing bracket and a separately validated port.
        // IPv6 字面值必须有闭括号，端口独立校验。
        let close = host.rfind(']')?;
        validate_port(&host[close + 1..])?;
        // Split the zone using its encoded percent introducer before decoding.
        // 在解码之前按编码的百分号起始符拆分区域标识。
        let hostname = &host[1..close];
        // Validate the zone separately but parse only the address portion as IPv6.
        // 独立校验区域标识，仅将地址部分按 IPv6 解析。
        let address = if let Some((address, zone)) = hostname.split_once("%25") {
            if zone.is_empty() {
                return None;
            }
            decode(zone, DecodeMode::Zone)?;
            decode(address, DecodeMode::Host)?
        } else {
            decode(hostname, DecodeMode::Host)?
        };
        address.parse::<Ipv6Addr>().ok()?;
    } else {
        // Current external Go rejects multiple HTTP host colons but permits historical non-HTTP forms.
        // 当前外置 Go 拒绝 HTTP 主机多冒号形式，但保留历史非 HTTP 形式。
        let colon = if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
            host.find(':')
        } else {
            host.rfind(':')
        };
        if let Some(colon) = colon {
            validate_port(&host[colon..])?;
        }
        decode(host, DecodeMode::Host)?;
    }
    Some(())
}

/// Accepts no port or a colon followed only by ASCII digits, including an empty port.
/// 接受无端口，或冒号后仅包含 ASCII 数字的端口，包括空端口。
fn validate_port(port: &str) -> Option<()> {
    (port.is_empty()
        || port
            .strip_prefix(':')
            .is_some_and(|digits| digits.bytes().all(|byte| byte.is_ascii_digit())))
    .then_some(())
}
