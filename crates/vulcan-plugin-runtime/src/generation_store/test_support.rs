//! Test-only selection shortcuts that publish the sole activation format with staged empty credentials.
//! 仅测试使用的选择捷径，以已暂存空凭据发布唯一激活格式。

use super::{
    PinnedVulcanPluginGeneration, VulcanPluginActivation, VulcanPluginConfigurationSelection,
    VulcanPluginCredentialBackend, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues, VulcanPluginGenerationStore, VulcanPluginStoreResult,
};

/// Backend for inventories whose every declared slot is explicitly unconfigured.
/// 用于每个声明槽位均明确未配置之清单的后端。
struct UnconfiguredCredentialBackend;

impl VulcanPluginCredentialBackend for UnconfiguredCredentialBackend {
    /// Report every slot as absent, matching the unconfigured inventory.
    /// 报告每个槽位缺失，与未配置清单一致。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }
    /// Reject writes because no value belongs to this inventory.
    /// 拒绝写入，因为此清单不拥有任何值。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err("Unexpected unconfigured-fixture write.".into())
    }
    /// Absence is already the confirmed state of every slot.
    /// 缺失已是每个槽位的确认状态。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Ok(())
    }
}

impl PinnedVulcanPluginGeneration {
    /// Return the complete capability set verified for this exact package.
    /// 返回为此精确插件包校验的完整能力集合。
    pub(crate) fn required_permissions_for_test(&self) -> Vec<String> {
        self.required_permissions.clone()
    }

    /// Return the package's own default configuration, or NotDeclared when it declares none.
    /// 返回插件包自身默认配置；未声明配置时返回 NotDeclared。
    pub(crate) fn default_configuration_for_test(&self) -> VulcanPluginConfigurationSelection {
        match &self.configuration {
            None => VulcanPluginConfigurationSelection::NotDeclared {},
            Some(config) => {
                let bytes = std::fs::read(self.package_root.join(&config.defaults_file))
                    .expect("read fixture configuration defaults");
                VulcanPluginConfigurationSelection::Document {
                    value: serde_json::from_slice(&bytes)
                        .expect("decode fixture configuration defaults"),
                }
            }
        }
    }
}

impl VulcanPluginGenerationStore {
    /// Stage a fresh revision whose declared slots are all unconfigured; return its identity.
    /// 暂存全部声明槽位均未配置的新修订；返回其身份。
    pub(crate) fn stage_unconfigured_credentials_for_test(
        &self,
        generation: &PinnedVulcanPluginGeneration,
    ) -> VulcanPluginCredentialRevision {
        let revision = VulcanPluginCredentialRevision::allocate();
        let values: VulcanPluginCredentialValues = generation
            .credential_keys
            .iter()
            .map(|key| (key.clone(), None))
            .collect();
        self.begin_credential_staging(generation, revision.clone(), &values)
            .expect("prepare unconfigured credential inventory")
            .write_and_finish(&UnconfiguredCredentialBackend, &values)
            .expect("verify unconfigured credential staging");
        revision
    }

    /// Select `generation` with `approved_permissions`, fresh unconfigured credentials and package defaults.
    /// 以 `approved_permissions`、新的未配置凭据及插件包默认值选择 `generation`。
    pub(crate) fn activate_for_test(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        approved_permissions: Vec<String>,
    ) -> VulcanPluginStoreResult<VulcanPluginActivation> {
        let revision = self.stage_unconfigured_credentials_for_test(generation);
        self.activate_with_credentials_for_test(
            generation,
            expected_revision,
            approved_permissions,
            revision,
        )
    }

    /// Select `generation` with an already staged `credential_revision` and package default configuration.
    /// 以已暂存 `credential_revision` 及插件包默认配置选择 `generation`。
    pub(crate) fn activate_with_credentials_for_test(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        approved_permissions: Vec<String>,
        credential_revision: VulcanPluginCredentialRevision,
    ) -> VulcanPluginStoreResult<VulcanPluginActivation> {
        self.activate_configured(
            generation,
            expected_revision,
            approved_permissions,
            credential_revision,
            generation.default_configuration_for_test(),
        )
    }
}
