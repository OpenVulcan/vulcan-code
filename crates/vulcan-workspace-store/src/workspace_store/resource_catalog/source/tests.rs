//! Exact-size and digest-bounded source read regressions.
//! 精确大小及摘要有界来源读取回归。

use super::*;

/// Accepts only the canonical positive, bounded project id and leaves other locator families to their owners.
/// 仅接受规范正数有界项目ID，并将其他定位符族交回各自所有者。
#[test]
fn project_locator_has_one_canonical_spelling() -> Result<(), Box<dyn std::error::Error>> {
    // These ids exercise the smallest identity and the JavaScript-safe upper boundary.
    // 这些ID覆盖最小身份和JavaScript安全上界。
    for value in [1, 9_007_199_254_740_991] {
        // The same typed id must round-trip without aliases or project path disclosure.
        // 同一强类型ID必须无别名、无项目路径泄露地往返。
        let id = ResourceId::try_new(value)?;
        assert_eq!(
            parse_project_resource_locator(&project_resource_locator(id))?,
            Some(id)
        );
    }
    // Reserved-family malformed input must fail without echoing attacker-controlled text.
    // 保留族中的损坏输入必须失败，且不能回显攻击者控制的文本。
    for reference in [
        "project-resource:",
        "project-resource:/1",
        "project-resource://0",
        "project-resource://01",
        "project-resource://-1",
        "project-resource://+1",
        "project-resource://1/",
        "project-resource://1?q=secret",
        "project-resource://1#secret",
        "project-resource://%31",
        "project-resource://host/1",
        "project-resource://9007199254740992",
        "project-resource://18446744073709551616",
        "project-resource://１",
    ] {
        assert_eq!(
            parse_project_resource_locator(reference),
            Err(String::from("Invalid project resource locator."))
        );
    }
    assert_eq!(parse_project_resource_locator("attachment://old-id")?, None);
    assert_eq!(parse_project_resource_locator("images/source.png")?, None);
    Ok(())
}

/// Registers `bytes` and resolves metadata only, returning the private source for this isolated `project`.
/// 注册`bytes`并仅解析元数据，返回隔离`project`的私有来源。
fn source(
    project: &Path,
    bytes: &[u8],
) -> Result<ProjectResourceSource, Box<dyn std::error::Error>> {
    // The existing project owner serializes resource catalog mutations.
    // 既有项目所有者串行化资源目录修改。
    let registry = ProjectResourceCatalogRegistry::default();
    // Registration returns exact ordered logical resource identities.
    // 注册返回精确有序逻辑资源身份。
    let ids = registry.resolve_or_register_project_resources(
        project,
        &[ProjectResourceRegistration {
            original_name: "source.bin".into(),
            mime: None,
            content: bytes.to_vec(),
            provider_bindings: Vec::new(),
        }],
    )?;
    resolve_project_resource_source(project, &load_project_resource_catalog(project)?, ids[0])?
        .ok_or_else(|| "fixture source missing".into())
}

/// Verifies inclusive exact limits and pre-read oversize rejection.
/// 验证包含边界的精确限制与读取前超限拒绝。
#[test]
fn source_bounds_are_inclusive_and_checked_before_reads() -> Result<(), Box<dyn std::error::Error>>
{
    // An isolated temporary project owns every file touched by this test.
    // 隔离临时项目持有本测试涉及的全部文件。
    let project = tempfile::tempdir()?;
    // The exact source identity is retained independently of later receipt or file changes.
    // 精确来源身份独立于后续回执或文件变化保留。
    let source = source(project.path(), b"1234")?;
    assert_eq!(source.read_bounded(4)?, b"1234");
    source.verify_bounded(4)?;
    assert!(source.read_bounded(3).is_err());
    assert!(source.verify_bounded(3).is_err());
    // Deleting a fixture proves rejected bounds do not depend on opening the entity first.
    // 删除夹具证明超限拒绝不依赖先打开实体。
    fs::remove_file(&source.path)?;
    assert!(
        source
            .read_bounded(3)
            .expect_err("bound rejection")
            .to_string()
            .contains("exceeds admitted")
    );
    Ok(())
}

/// Verifies same-size corruption and post-resolution growth cannot escape the catalog identity.
/// 验证同大小损坏与解析后增长不能绕过目录身份。
#[test]
fn source_rechecks_digest_and_size_after_metadata_capture() -> Result<(), Box<dyn std::error::Error>>
{
    // An isolated temporary project owns every file touched by this test.
    // 隔离临时项目持有本测试涉及的全部文件。
    let project = tempfile::tempdir()?;
    // The exact source identity is retained independently of later receipt or file changes.
    // 精确来源身份独立于后续回执或文件变化保留。
    let source = source(project.path(), b"1234")?;
    fs::write(&source.path, b"4321")?;
    assert!(source.read_bounded(4).is_err());
    assert!(source.verify_bounded(4).is_err());
    fs::write(&source.path, b"12345")?;
    assert!(source.read_bounded(100).is_err());
    assert!(source.verify_bounded(100).is_err());
    // Read the authoritative persisted catalog for invariant assertions.
    // 读取权威持久化目录进行不变量断言。
    let catalog = load_project_resource_catalog(project.path())?;
    assert!(
        resolve_project_resource_source(project.path(), &catalog, source.entry.resource_id)
            .is_err()
    );
    Ok(())
}

/// Keeps missing catalog ids and missing entities non-blocking without changing their catalog owner.
/// 目录ID与实体缺失保持不阻断，不修改其目录所有者。
#[test]
fn missing_source_keeps_catalog_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    // An isolated temporary project owns every file touched by this test.
    // 隔离临时项目持有本测试涉及的全部文件。
    let project = tempfile::tempdir()?;
    // The exact source identity is retained independently of later receipt or file changes.
    // 精确来源身份独立于后续回执或文件变化保留。
    let source = source(project.path(), b"1234")?;
    // Read the authoritative persisted catalog for invariant assertions.
    // 读取权威持久化目录进行不变量断言。
    let catalog = load_project_resource_catalog(project.path())?;
    assert!(
        resolve_project_resource_source(project.path(), &catalog, ResourceId::try_new(2)?)?
            .is_none()
    );
    fs::remove_file(&source.path)?;
    assert!(
        resolve_project_resource_source(project.path(), &catalog, source.entry.resource_id)?
            .is_none()
    );
    assert_eq!(load_project_resource_catalog(project.path())?, catalog);
    Ok(())
}

/// Pre-cancellation rejects before opening an absent entity, and never changes its catalog.
/// 预先取消在打开缺失实体前拒绝，绝不修改其目录。
#[test]
fn cancelled_source_read_stops_before_opening_entity() -> Result<(), Box<dyn std::error::Error>> {
    // Every entity is created within this isolated test-owned project.
    // 每个实体均在本隔离测试所有项目内创建。
    let project = tempfile::tempdir()?;
    // Keep the canonical source descriptor after deleting only the temporary fixture entity.
    // 仅删除临时夹具实体后保留规范来源描述符。
    let source = source(project.path(), b"1234")?;
    // The original catalog remains the mutation boundary under test.
    // 原目录仍是本测试的修改边界。
    let catalog = load_project_resource_catalog(project.path())?;
    fs::remove_file(&source.path)?;
    assert!(
        matches!(source.read_bounded_cancellable(4, || true), Err(ProjectResourceCatalogError::CatalogRead { detail, .. }) if detail == "project resource read cancelled")
    );
    assert_eq!(load_project_resource_catalog(project.path())?, catalog);
    Ok(())
}

/// Cancellation between chunks discards partial bytes; a later independent read still verifies the untouched source.
/// 分块间取消丢弃部分字节；之后独立读取仍校验未改变来源。
#[test]
fn cancelled_source_read_discards_partial_buffer_between_chunks()
-> Result<(), Box<dyn std::error::Error>> {
    // A multi-chunk entity distinguishes cooperative cancellation from a full-read-then-check implementation.
    // 多块实体区分协作式取消与整文件读取后才检查的实现。
    let project = tempfile::tempdir()?;
    // The source has a trailing byte so length and digest checks must include more than two full chunks.
    // 来源带尾字节，使长度及摘要检查必须覆盖两个完整块之外的内容。
    let bytes = vec![13_u8; 128 * 1024 + 1];
    // Only this source belongs to the callback's cancellation decision.
    // 仅此来源属于回调取消决定。
    let source = source(project.path(), &bytes)?;
    // Cancel at the next chunk boundary after the first completed read, without thread timing races.
    // 在首次完成读取后的下一分块边界取消，不依赖线程时序竞争。
    let probes = std::cell::Cell::new(0);
    // The callback is host-owned control, not a field supplied by the model.
    // 回调是宿主持有控制，而非模型提供的字段。
    let result = source.read_bounded_cancellable(bytes.len() as u64, || {
        probes.set(probes.get() + 1);
        probes.get() >= 4
    });
    assert!(
        matches!(result, Err(ProjectResourceCatalogError::CatalogRead { detail, .. }) if detail == "project resource read cancelled")
    );
    assert_eq!(probes.get(), 4);
    assert_eq!(source.read_bounded(bytes.len() as u64)?, bytes);
    Ok(())
}
