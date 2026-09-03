//! Freezes every public durable-memory source discriminant required by the first VMM integration.
//! 冻结首次VMM接入所需的每个公开持久记忆来源判别值。

use vulcan_memorymesh_core_contracts::MemorySourceKind;

/// Preserves existing source values and appends system-seed and legacy provenance without aliases.
/// 保留既有来源值，并以无别名方式追加系统种子及旧版来源。
#[test]
fn memory_source_kind_discriminants_are_closed_and_stable() {
    let expected = [
        (MemorySourceKind::Turn, 1),
        (MemorySourceKind::Direct, 2),
        (MemorySourceKind::Import, 3),
        (MemorySourceKind::SystemSeed, 4),
        (MemorySourceKind::Legacy, 5),
    ];
    for (source, discriminant) in expected {
        assert_eq!(source.as_u32(), discriminant);
        assert_eq!(MemorySourceKind::try_from_u32(discriminant), Some(source));
    }
    assert_eq!(MemorySourceKind::try_from_u32(0), None);
    assert_eq!(MemorySourceKind::try_from_u32(6), None);
}
