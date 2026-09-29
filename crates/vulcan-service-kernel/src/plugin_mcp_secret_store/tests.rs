//! Credential namespace framing and actual Windows secure-store revision isolation.
//! 凭据命名空间分帧及真实 Windows 安全存储修订隔离。

use super::*;

#[cfg(windows)]
mod staging;

/// Verify stable account bytes and tuple separation without opening the user's credential backend.
/// 验证稳定账户字节及元组隔离，不打开用户凭据后端。
#[test]
fn credential_binding_account_has_stable_framing_and_rejects_invalid_components() {
    // This fixed identity makes the secure-store account portable across pointer widths.
    // 此固定身份使安全存储账户在不同指针位宽下保持可移植。
    let revision = VulcanPluginCredentialRevision::parse("12345678-1234-4234-8234-123456789abc")
        .expect("canonical fixture revision");
    assert_eq!(
        versioned_plugin_secret_account("plugin-a", "server-a", "token", &revision).unwrap(),
        "sha256:cc775a89e39e013d09f43c71c32c31815528fc219a1bba789ed30dc3410b1788"
    );
    assert_ne!(
        VERSIONED_PLUGIN_MCP_SECRET_SERVICE,
        VULCAN_PLUGIN_MCP_SECRET_SERVICE
    );
    assert_ne!(
        versioned_plugin_secret_account("plugin-a", "ab", "c", &revision).unwrap(),
        versioned_plugin_secret_account("plugin-a", "a", "bc", &revision).unwrap()
    );
    assert_ne!(
        versioned_plugin_secret_account("plugin-a", "server-a", "token", &revision).unwrap(),
        versioned_plugin_secret_account("plugin-b", "server-a", "token", &revision).unwrap()
    );
    for (plugin, server, key) in [
        ("", "server", "token"),
        ("plugin", "", "token"),
        ("plugin", "server", ""),
        ("plugin\0suffix", "server", "token"),
        ("plugin", "server\0suffix", "token"),
        ("plugin", "server", "token\0suffix"),
    ] {
        assert!(versioned_plugin_secret_account(plugin, server, key, &revision).is_err());
    }
}

/// Retain only uniquely named test entries so assertion failures also attempt fixture cleanup.
/// 仅保留唯一命名测试条目，使断言失败时也尝试清理夹具。
#[cfg(windows)]
struct SecureFixtureEntries {
    /// These entries contain disposable fixture strings, never existing user credentials.
    /// 这些条目包含一次性夹具字符串，绝不包含既有用户凭据。
    entries: Vec<Entry>,
}

#[cfg(windows)]
impl Drop for SecureFixtureEntries {
    /// Attempt cleanup during unwinding; normal completion separately checks every deletion result.
    /// 展开期间尝试清理；正常完成时独立检查每次删除结果。
    fn drop(&mut self) {
        for entry in &self.entries {
            let _ = entry.delete_credential();
        }
    }
}

/// Prove the actual Windows adapter never overwrites or probes a different revision on a missing selected value.
/// 证明真实 Windows 适配器在选定值缺失时绝不覆盖或探测其他修订。
/// The fixture uses a random plugin namespace and removes every created secure entry before returning.
/// 夹具使用随机插件命名空间，并在返回前移除每个已创建安全条目。
#[cfg(windows)]
#[test]
fn credential_binding_windows_secure_store_keeps_legacy_and_revisions_separate() {
    // A unique namespace prevents test setup and cleanup from addressing any installed plugin.
    // 唯一命名空间防止测试准备及清理寻址任何已安装插件。
    let plugin = format!("credential-binding-test-{}", uuid::Uuid::new_v4());
    // Revision A remains readable after revision B is populated.
    // 修订 B 写入后，修订 A 保持可读。
    let first = VulcanPluginCredentialRevision::allocate();
    // A distinct revision occupies a different secure entry for the exact same declared key.
    // 不同修订为精确同一声明键占用不同安全条目。
    let second = VulcanPluginCredentialRevision::allocate();
    // The legacy and versioned bindings all remain live to expose any unintended fallback.
    // 旧绑定及版本绑定均保持有效，以暴露任何非预期回退。
    let bindings = [
        VulcanPluginCredentialBinding::Legacy,
        VulcanPluginCredentialBinding::Revision(first.clone()),
        VulcanPluginCredentialBinding::Revision(second.clone()),
    ];
    // Cleanup owns every entry before any value is written.
    // 任何值写入前，清理所有者持有每个条目。
    let mut cleanup = SecureFixtureEntries {
        entries: Vec::new(),
    };
    for binding in &bindings {
        // Open exactly the production namespace selected by this declared binding.
        // 打开此声明绑定选定的精确生产命名空间。
        let entry = match binding {
            VulcanPluginCredentialBinding::Legacy => {
                plugin_secret_entry(&plugin, "server", "token")
            }
            VulcanPluginCredentialBinding::Revision(revision) => {
                versioned_plugin_secret_entry(&plugin, "server", "token", revision)
            }
        }
        .expect("open isolated secure fixture");
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
        cleanup.entries.push(entry);
    }
    // Each entry receives a distinct, deliberately non-sensitive test value.
    // 每个条目接收不同且明确不敏感的测试值。
    let values = ["fixture-legacy", "fixture-first", "fixture-second"];
    for (entry, value) in cleanup.entries.iter().zip(values) {
        entry
            .set_password(value)
            .expect("write isolated secure fixture");
    }
    // Reads use the actual production trait implementation, not direct entry access.
    // 读取使用实际生产特征实现，不直接访问条目。
    let backend = OsPluginMcpSecretStore;
    for (binding, value) in bindings.iter().zip(values) {
        assert_eq!(
            backend
                .read_secret(&plugin, "server", "token", binding)
                .unwrap(),
            value
        );
    }
    versioned_plugin_secret_entry(&plugin, "server", "token", &first)
        .unwrap()
        .delete_credential()
        .expect("remove only revision A");
    assert!(
        backend
            .read_secret(
                &plugin,
                "server",
                "token",
                &VulcanPluginCredentialBinding::Revision(first)
            )
            .expect_err("revision A must not fall back")
            .contains("is not configured")
    );
    assert_eq!(
        backend
            .read_secret(
                &plugin,
                "server",
                "token",
                &VulcanPluginCredentialBinding::Legacy
            )
            .unwrap(),
        "fixture-legacy"
    );
    assert_eq!(
        backend
            .read_secret(
                &plugin,
                "server",
                "token",
                &VulcanPluginCredentialBinding::Revision(second)
            )
            .unwrap(),
        "fixture-second"
    );
    for entry in &cleanup.entries {
        assert!(matches!(
            entry.delete_credential(),
            Ok(()) | Err(KeyringError::NoEntry)
        ));
        assert!(matches!(entry.get_password(), Err(KeyringError::NoEntry)));
    }
}
