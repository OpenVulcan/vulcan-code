//! Bounded deterministic encoding for versioned VMM checkpoint continuations.
//! 为带版本的 VMM 检查点续体提供有界确定性编码。

use postcard::ser_flavors::Flavor;
use serde::{Serialize, de::DeserializeOwned};

/// Maximum encoded size shared by continuations and effect result summaries.
/// 续体与执行结果摘要共用的最大编码字节数。
const MAX_PRIVATE_CONTINUATION_BYTES: usize = 16_777_216;

/// Storage flavor that rejects writes before the checkpoint byte limit is exceeded.
/// 在写入超过检查点字节上限之前拒绝写入的存储适配器。
#[derive(Default)]
struct BoundedBytes {
    /// Encoded bytes, never larger than the authoritative checkpoint limit.
    /// 已编码字节，长度始终不超过检查点权威上限。
    bytes: Vec<u8>,
}

impl Flavor for BoundedBytes {
    type Output = Vec<u8>;

    /// Appends the supplied bytes or returns a capacity error without extending the buffer.
    /// 追加给定字节，或返回容量错误且不扩展缓冲区。
    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if bytes.len() > MAX_PRIVATE_CONTINUATION_BYTES - self.bytes.len() {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    /// Appends one encoded byte under the same bound and returns the write result.
    /// 在相同上限内追加一个编码字节并返回写入结果。
    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.try_extend(&[byte])
    }

    /// Returns the completed bounded byte buffer without copying it.
    /// 无复制地返回完成编码的有界字节缓冲区。
    fn finalize(self) -> postcard::Result<Self::Output> {
        Ok(self.bytes)
    }
}

/// Encodes a Serde value using the stable Postcard format, rejecting oversized output during writes.
/// 使用稳定的 Postcard 格式编码 Serde 值，并在写入期间拒绝超限输出。
pub(crate) fn encode<T: Serialize + ?Sized>(value: &T) -> postcard::Result<Vec<u8>> {
    postcard::serialize_with_flavor(value, BoundedBytes::default())
}

/// Decodes one complete value from bounded input, rejecting truncation and trailing bytes.
/// 从有界输入解码一个完整值，拒绝截断和尾随字节。
pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> postcard::Result<T> {
    if bytes.len() > MAX_PRIVATE_CONTINUATION_BYTES {
        return Err(postcard::Error::DeserializeBadEncoding);
    }
    let (value, remaining) = postcard::take_from_bytes(bytes)?;
    if !remaining.is_empty() {
        return Err(postcard::Error::DeserializeBadEncoding);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies exact integer, Unicode and floating-point bit preservation across a round trip.
    /// 验证往返编解码精确保留整数、Unicode 和浮点数位模式。
    #[test]
    fn round_trip_preserves_checkpoint_values() {
        let value = (u64::MAX, i64::MIN, "记忆", -0.0_f64);
        let bytes = encode(&value).expect("encode checkpoint values");
        let restored: (u64, i64, String, f64) = decode(&bytes).expect("decode checkpoint values");
        assert_eq!(
            (restored.0, restored.1, restored.2.as_str()),
            (value.0, value.1, value.2)
        );
        assert_eq!(restored.3.to_bits(), value.3.to_bits());
        assert_eq!(bytes, encode(&value).expect("deterministic encoding"));
    }

    /// Verifies the input boundary and rejects both truncated and appended payload data.
    /// 验证输入边界，并拒绝截断载荷与追加数据。
    #[test]
    fn rejects_invalid_or_oversized_input() {
        let mut bytes = encode(&"checkpoint").expect("encode string");
        assert!(decode::<String>(&bytes[..bytes.len() - 1]).is_err());
        bytes.push(0);
        assert!(decode::<String>(&bytes).is_err());
        assert!(decode::<u8>(&vec![0; MAX_PRIVATE_CONTINUATION_BYTES + 1]).is_err());
        assert!(decode::<Vec<u8>>(&[255; 16]).is_err());
    }

    /// Verifies writes stop at the limit instead of allocating an oversized serialized result.
    /// 验证写入在上限处停止，而非分配超限的序列化结果。
    #[test]
    fn rejects_output_before_exceeding_limit() {
        let mut output = BoundedBytes::default();
        output
            .try_extend(&vec![0; MAX_PRIVATE_CONTINUATION_BYTES])
            .expect("exact bound");
        assert_eq!(
            output.try_push(0),
            Err(postcard::Error::SerializeBufferFull)
        );
        assert_eq!(output.bytes.len(), MAX_PRIVATE_CONTINUATION_BYTES);
        assert_eq!(
            encode(&vec![0_u8; MAX_PRIVATE_CONTINUATION_BYTES]),
            Err(postcard::Error::SerializeBufferFull)
        );
    }
}
