//! Actual catalog pins reuse the retained legacy namespace while rejecting foreign or changed authority.
//! 实际目录固定复用保留旧命名空间，同时拒绝外来或已变权威。

use super::*;

/// A real valid managed replacement between validation and pin acquisition must be rejected while the original namespace remains owned.
/// 验证与固定取得之间的真实有效受管替换必须被拒绝，同时原命名空间保持持有。
#[test]
fn legacy_mutation_catalog_rejects_valid_selection_replaced_between_actual_reads() {
    // Existing canonical fixtures supply a genuinely verified configuration-capable package.
    // 既有规范夹具提供真实已验证且支持配置的插件包。
    let root = tempfile::tempdir().expect("catalog two-read replacement fixture");
    // Both legal managed selections below are produced by the actual full selection writer.
    // 下方两个合法受管选择均由实际完整选择写入器产生。
    let (store, package, _, legacy) = fixture(root.path());
    // This empty actual revision needs no secret values but retains the full credential inventory authority.
    // 此实际空修订不需要秘密值，但保留完整凭据清单权威。
    let revision = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(
            &package,
            revision.clone(),
            &VulcanPluginCredentialValues::new(),
        )
        .expect("actual revision writer")
        .write_and_finish(&EmptyBackend, &VulcanPluginCredentialValues::new())
        .expect("actual complete staged revision");
    // Original null document is explicitly schema-valid and selected together with complete approval and revision.
    // 原空文档明确通过模式验证，并与完整批准及修订一起选定。
    let original = store
        .activate_configured(
            &package,
            legacy.revision,
            package.required_permissions.clone(),
            revision.clone(),
            VulcanPluginConfigurationSelection::Document { value: Value::Null },
        )
        .expect("actual original configured writer");
    // Capture exact producer bytes, never reconstruct a chosen record from an identifier or hand-written phase.
    // 捕获精确生产者字节，绝不从标识或手写阶段重建所选记录。
    let path = store.root.join("active/legacy-plugin.json");
    // Full original selection bytes remain the exact fixture restoration authority.
    // 完整原选择字节保持为精确夹具恢复权威。
    let original_bytes = fs::read(&path).expect("actual original producer bytes");
    // A different schema-valid document makes the second real selected record distinct without invalidating approval or credentials.
    // 不同有效模式文档使第二个真实已选记录不同，同时不使批准或凭据无效。
    let replacement = store
        .activate_configured(
            &package,
            original.revision,
            package.required_permissions.clone(),
            revision,
            VulcanPluginConfigurationSelection::Document {
                value: json!({"label": "replacement"}),
            },
        )
        .expect("actual valid replacement writer");
    // The injected bytes must already have passed ordinary full pin validation, including configuration and credential ownership.
    // 注入字节必须已通过普通完整固定验证，包括配置及凭据所有权。
    let replacement_bytes = fs::read(&path).expect("actual replacement producer bytes");
    // An independent canonical store confirms the replacement is pin-able rather than relying on an early decoder failure.
    // 独立规范存储确认替换可固定，而非依赖提前解码失败。
    let observer = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
        .expect("independent canonical store")
        .expect("actual store exists");
    assert!(
        observer
            .pin_active("legacy-plugin")
            .expect("ordinary valid replacement pins")
            .activation
            .as_ref()
            == Some(&replacement)
    );
    fs::write(&path, &original_bytes).expect("fixture restore actual original producer bytes");
    // The same actual guard freezes the restored real full original before its first successful validation.
    // 同一实际守卫在首次成功验证之前固定已恢复真实完整原选择。
    let guard = store
        .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
        .expect("real original guard");
    // This scoped observer performs its one real replacement after validation and immediately before the shared actual pin helper.
    // 此作用域观察器在验证之后、共享实际固定辅助方法之前执行单次真实替换。
    let _replacement = guard.replace_catalog_selection_after_validation_for_test(replacement_bytes);
    assert_eq!(
        guard
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .expect_err("valid foreign pins must not escape original guard")
            .code,
        Code::Conflict
    );
    assert!(
        store
            .read_activation("legacy-plugin")
            .expect("actual replacement observed")
            == Some(replacement.clone())
    );
    assert_eq!(
        observer
            .pin_active("legacy-plugin")
            .expect_err("original namespace File stays owned")
            .code,
        Code::Busy
    );
    assert_eq!(
        observer
            .deactivate("legacy-plugin", replacement.revision)
            .expect_err("writer still excluded after Conflict")
            .code,
        Code::Busy
    );
    fs::write(&path, original_bytes).expect("restore exact original before same-owner retry");
    assert!(
        guard
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .expect("same guard exact retry")
            .activation
            .as_ref()
            == Some(&original)
    );
    // Retire the same-thread test observer before dropping its borrowed real guard; retry already proved one-shot consumption.
    // 释放借用真实守卫之前，先退役同线程测试观察器；重试已证明单次消费。
    drop(_replacement);
    drop(guard);
    observer
        .deactivate("legacy-plugin", original.revision)
        .expect("only actual guard drop permits writer");
}

/// Catalog pins preserve full managed selection and real ownership while only actual guard retirement permits another writer.
/// 目录固定保留完整受管选择及真实所有权，仅实际守卫退役允许另一写入者。
#[test]
fn legacy_mutation_catalog_pins_exact_selection_without_reopening_namespace() {
    // The existing canonical fixture supplies verified configuration, permissions and immutable package bytes.
    // 既有规范夹具提供已验证配置、权限及不可变插件包字节。
    let root = tempfile::tempdir().expect("catalog owner fixture");
    // The retained package initially supplies the actual generation lease used to publish managed state.
    // 保留插件包最初提供发布受管状态所用实际代际租约。
    let (store, package, _, original) = fixture(root.path());
    // Empty inventory needs no backend values but still owns a real revision File.
    // 空清单不需要后端值，但仍持有真实修订 File。
    let revision = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(
            &package,
            revision.clone(),
            &VulcanPluginCredentialValues::new(),
        )
        .expect("real credential writer")
        .write_and_finish(&EmptyBackend, &VulcanPluginCredentialValues::new())
        .expect("actual staged revision");
    // Managed document and revision belong to this complete production-written selection.
    // 受管文档及修订属于此完整生产写入选择。
    let selected = store
        .activate_configured(
            &package,
            original.revision,
            package.required_permissions.clone(),
            revision.clone(),
            VulcanPluginConfigurationSelection::Document { value: Value::Null },
        )
        .expect("actual full configured selection");
    // A separate store opens the same canonical OS files without sharing a Rust lock object.
    // 独立存储打开相同规范操作系统文件，不共享 Rust 锁对象。
    let observer = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
        .expect("actual independent store")
        .expect("existing canonical store");
    // Freeze full selection and namespace before catalog observations begin.
    // 目录观察开始前固定完整选择及命名空间。
    let guard = store
        .acquire_legacy_mutation_guard("legacy-plugin", Some(&selected))
        .expect("actual original namespace guard");
    assert_eq!(
        observer
            .pin_active("legacy-plugin")
            .expect_err("ordinary self-busy namespace")
            .code,
        Code::Busy
    );
    // The actual guard authorizes this independent store's locked inspection without acquiring another namespace File.
    // 实际守卫授权此独立存储持锁检查，不取得另一命名空间 File。
    let active = guard
        .pin_active_for_catalog(&observer, "legacy-plugin")
        .expect("same actual original catalog pins");
    assert!(
        active.activation.as_ref() == Some(&selected),
        "complete selection must stay exact"
    );
    assert!(
        active
            .generation
            .as_ref()
            .expect("actual generation pin")
            .identity()
            == package.identity()
    );
    assert!(
        active
            .credentials
            .as_ref()
            .expect("actual credential pin")
            .record()
            .revision()
            == &revision
    );
    assert!(
        guard
            .pin_active_for_catalog(&store, "legacy-plugin")
            .is_ok()
    );
    assert_eq!(
        observer
            .deactivate("legacy-plugin", selected.revision)
            .expect_err("original guard excludes writer")
            .code,
        Code::Busy
    );
    // Peer publication and ordinary reads use a different real namespace and remain independent.
    // 同伴发布及普通读取使用另一真实命名空间并保持独立。
    let peer = prepare(
        &observer,
        &write_package(&root.path().join("peer"), "peer-plugin"),
    );
    observer
        .activate_for_test(&peer, 0, peer.required_permissions.clone())
        .expect("peer publication");
    assert!(
        observer
            .pin_active("peer-plugin")
            .expect("peer ordinary pin")
            .generation
            .is_some()
    );
    // Preserve only the catalog's physical ownership before retiring the original publishing pin.
    // 退役原发布固定前，仅保留目录物理所有权。
    let identity = package.identity().clone();
    drop(package);
    drop(guard);
    observer
        .deactivate("legacy-plugin", selected.revision)
        .expect("actual guard retirement permits writer");
    assert_eq!(
        observer
            .collect(&identity)
            .expect_err("catalog's real generation File remains live")
            .code,
        Code::Busy
    );
    drop(active);
    assert!(
        observer
            .collect(&identity)
            .expect("last actual catalog generation owner retired")
    );
}

/// Actual absent selection remains explicit and cannot authorize a foreign root or plugin or adopt changed full selected bytes.
/// 实际缺失选择保持显式，不能授权外来根或插件，也不能接管已变完整选择字节。
#[test]
fn legacy_mutation_catalog_rejects_foreign_and_changed_complete_selection() {
    // Both guards below use a genuine canonical store; no fabricated owner is reconstructed from identifiers.
    // 下方两个守卫使用真实规范存储；不从标识重建伪造所有者。
    let root = tempfile::tempdir().expect("catalog exact original fixture");
    // Complete original selection remains production-written throughout the external-corruption fixture.
    // 外部损坏夹具全程保留生产写入完整原选择。
    let (store, _package, _, original) = fixture(root.path());
    // Independent store binding must be canonical-root equality rather than Rust object identity.
    // 独立存储绑定必须为规范根相等，而非 Rust 对象身份。
    let observer = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
        .expect("independent canonical store")
        .expect("actual store exists");
    // Actual None has no chosen package, credential revision or alternate selected namespace.
    // 实际 None 没有所选插件包、凭据修订或替代已选命名空间。
    let absent = store
        .acquire_legacy_mutation_guard("unpublished-plugin", None)
        .expect("actual absent original");
    // The original guard validates absence before calling the same strict selected pin helper.
    // 原守卫调用相同严格选择固定辅助方法之前校验缺失。
    let empty = absent
        .pin_active_for_catalog(&observer, "unpublished-plugin")
        .expect("actual absent catalog choice");
    assert!(
        empty.activation.is_none() && empty.generation.is_none() && empty.credentials.is_none()
    );
    assert_eq!(
        absent
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .expect_err("foreign plugin")
            .code,
        Code::Conflict
    );
    // A different real store has its own canonical namespace authority, even for the same literal plugin name.
    // 即使字面插件名相同，不同真实存储也具有自身规范命名空间权威。
    let foreign_root = tempfile::tempdir().expect("foreign root");
    // Opening a foreign root grants no ownership of the guard's original File.
    // 打开外来根不授予守卫原 File 所有权。
    let foreign = VulcanPluginGenerationStore::open(foreign_root.path()).expect("foreign store");
    assert_eq!(
        absent
            .pin_active_for_catalog(&foreign, "unpublished-plugin")
            .expect_err("foreign root")
            .code,
        Code::Conflict
    );
    drop(absent);
    // This guard freezes every selected field, including the complete permission approval at the same revision.
    // 此守卫固定每项选择字段，包括相同修订的完整权限批准。
    let guard = store
        .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
        .expect("actual selected original");
    // Explicit non-cooperating byte replacement is corruption evidence, never a competing writer bypass.
    // 明确非协作字节替换是损坏证据，绝不是竞争写入者绕过。
    let path = store.root.join("active/legacy-plugin.json");
    // Keep the exact actual bytes so fixture restoration cannot project a synthetic original.
    // 保留精确实际字节，使夹具恢复不能投影合成原选择。
    let bytes = fs::read(&path).expect("actual original selection bytes");
    // Changed full approval remains valid wire metadata and preserves the original revision.
    // 已变完整批准保持有效线格式元数据，并保留原修订。
    let mut changed = original.clone();
    changed.approved_permissions = vec!["config".into()];
    fs::write(
        &path,
        serde_json::to_vec(&changed).expect("changed selection encoding"),
    )
    .expect("explicit external replacement");
    assert_eq!(
        guard
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .expect_err("same revision with changed full selection")
            .code,
        Code::Conflict
    );
    assert_eq!(
        observer
            .acquire_legacy_mutation_guard("legacy-plugin", Some(&changed))
            .expect_err("original guard remains retained after rejection")
            .code,
        Code::Busy
    );
    fs::write(&path, bytes).expect("restore original real bytes");
    assert!(
        guard
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .is_ok()
    );
    drop(guard);
    observer
        .deactivate("legacy-plugin", original.revision)
        .expect("real guard drop permits next writer");
}

/// Real previously written Preparing journal bytes invalidate a retained catalog guard if the request authority is replaced externally.
/// 若请求权威被外部替换，真实先前写入 Preparing 日志字节使保留目录守卫无效。
#[test]
fn legacy_mutation_catalog_rejects_replaced_pending_request_without_releasing_guard() {
    // A real preparation writer supplies both the full request body and its canonical current index before guard acquisition.
    // 守卫取得前，真实准备写入器提供完整请求正文及规范当前索引。
    let root = tempfile::tempdir().expect("catalog pending replacement fixture");
    // Source and original selection come from the existing production-backed fixture.
    // 来源及原选择来自既有生产支持夹具。
    let (store, _package, source, original) = fixture(root.path());
    // The independent observer must remain excluded by the original actual namespace File through all failures.
    // 独立观察者须在所有失败期间持续受原实际命名空间 File 排除。
    let observer = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
        .expect("independent store")
        .expect("actual canonical root");
    // Freeze actual source digest and request before observing the true production-written journal.
    // 观察真实生产写入日志前，固定实际来源摘要及请求。
    let inspection = crate::inspect_user_plugin_package(&source).expect("actual inspected digest");
    // Request identity originates at the real preparation entry, never a synthetic lease owner.
    // 请求身份来自真实准备入口，绝不是合成租约所有者。
    let request = Uuid::new_v4();
    // This actual first writer creates canonical journal/index authority with no physical candidate allocation.
    // 此实际首写入器创建规范日志及索引权威，不分配物理候选。
    let preparation = store
        .begin_activation_preparation(
            request,
            &source,
            &inspection.content_fingerprint,
            Some(&original),
            vec!["config".into(), "network_https".into()],
        )
        .expect("actual original Preparing writer");
    // The unique journal layout is derived from the actual plugin and request writer arguments.
    // 唯一日志布局派生自实际插件及请求写入器参数。
    let path = store
        .root
        .join("transactions/legacy-plugin")
        .join(format!("{request}.json"));
    // Capture true Preparing bytes before actual Aborted transition, avoiding hand-written phase evidence.
    // 实际 Aborted 转换前捕获真实 Preparing 字节，避免手写阶段证据。
    let preparing_bytes = fs::read(&path).expect("actual durable Preparing body");
    drop(preparation);
    store
        .abort_activation_preparation_request("legacy-plugin", request)
        .expect("actual original abort writer");
    // Terminal request bytes remain exact restoration evidence throughout this corruption test.
    // 此损坏测试期间，终态请求字节保持为精确恢复证据。
    let aborted_bytes = fs::read(&path).expect("actual durable Aborted body");
    // Actual original terminal owner retired before this new legacy namespace authority is acquired.
    // 取得此新旧格式命名空间权威之前，实际原终态所有者已退役。
    let guard = store
        .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
        .expect("actual no-pending guard");
    assert!(
        guard
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .is_ok()
    );
    fs::write(&path, preparing_bytes)
        .expect("explicit external replacement with actual prior bytes");
    assert_eq!(
        guard
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .expect_err("actual indexed unfinished request")
            .code,
        Code::Busy
    );
    assert_eq!(
        observer
            .pin_active("legacy-plugin")
            .expect_err("original guard still owns namespace")
            .code,
        Code::Busy
    );
    fs::write(&path, aborted_bytes).expect("restore exact actual terminal bytes");
    assert!(
        guard
            .pin_active_for_catalog(&observer, "legacy-plugin")
            .is_ok()
    );
    drop(guard);
    observer
        .deactivate("legacy-plugin", original.revision)
        .expect("writer after actual guard retirement");
}
