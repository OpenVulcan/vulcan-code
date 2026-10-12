//! Real SQLite fixtures for unsupported credential rejection and explicit repair.
//! 用于不受支持凭据拒绝与显式修复的真实 SQLite 夹具。

use super::*;
use std::sync::Arc;
use vulcan_workspace_store::data_plane::{
    HostRuntimeMetadata, prepare_application_schema_for_test,
};

/// Storage format no current build reads, used to prove unsupported rows are rejected and repairable.
/// 当前构建不读取的存储格式，用于证明不受支持的记录会被拒绝且可修复。
pub(crate) const UNSUPPORTED_FORMAT_VERSION: u32 =
    vulcan_provider_adapters::CREDENTIAL_SECRET_FORMAT_VERSION + 1;

/// Owns an isolated database and store; no test resolves the user's application directory.
/// 持有隔离数据库和仓库；测试不解析用户应用目录。
pub(crate) struct CredentialRepairFixture {
    /// Real production credential store bound to the private SQLite schema.
    /// 绑定私有 SQLite 结构的真实生产凭据仓库。
    pub(crate) store: Arc<SqliteAiCredentialSecretStore>,
    /// Temporary directory retained until the database handles have been dropped.
    /// 保留至数据库句柄释放后的临时目录。
    pub(crate) directory: tempfile::TempDir,
}

impl CredentialRepairFixture {
    /// Opens a private schema with no inputs; returns the owned storage fixture.
    /// 无需参数打开私有结构；返回持有存储的夹具。
    pub(crate) async fn new() -> Self {
        // directory exclusively owns all fixture files.
        // directory 独占持有全部夹具文件。
        let directory = tempfile::tempdir().expect("private credential directory");
        // controller binds only absolute paths below the temporary directory.
        // controller 仅绑定临时目录下的绝对路径。
        let controller = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
            data_root: directory.path().to_path_buf(),
            sqlite_path: directory.path().join("app.sqlite3"),
            sqlite_binding_id: String::from("credential-repair-app"),
            system_statistics_sqlite_path: directory.path().join("usage.sqlite3"),
            system_statistics_sqlite_binding_id: String::from("credential-repair-usage"),
        });
        prepare_application_schema_for_test(&controller)
            .await
            .expect("private schema");
        Self {
            directory,
            store: Arc::new(SqliteAiCredentialSecretStore::new(controller)),
        }
    }

    /// Stores a synthetic API key at `reference` with the requested storage `version`.
    /// 按指定存储 `version` 在 `reference` 保存合成 API Key。
    /// Returns the exact relational row used to assert non-migration and compensation.
    /// 返回精确关系化记录，用于断言无迁移和补偿行为。
    pub(crate) async fn insert(&self, reference: &str, version: u32) -> AiCredentialSecretRecord {
        // secret is synthetic and never sent to a real provider.
        // secret 是合成凭据，绝不发送到真实供应商。
        let secret = CredentialSecretEnvelope::new_api_key(String::from("synthetic-repair-key"))
            .expect("fixture material");
        // record changes only the format discriminator, retaining a real historical row shape.
        // record 仅改变格式标识，保留真实历史记录形状。
        let mut record = credential_record_from_envelope(reference, &secret).expect("fixture row");
        record.format_version = i64::from(version);
        create_ai_credential_secret_record(&self.store.controller, &record)
            .await
            .expect("insert row");
        record
    }

    /// Reads the raw record at `reference`; returns the row without interpreting its credential format.
    /// 读取 `reference` 的原始记录；返回记录但不解释其凭据格式。
    pub(crate) async fn record(&self, reference: &str) -> Option<AiCredentialSecretRecord> {
        load_ai_credential_secret_record(&self.store.controller, reference)
            .await
            .expect("raw row")
    }

    /// Takes no arguments and returns the number of current-version credential rows in this private database.
    /// 无需参数，返回当前私有数据库中当前版本凭据的记录数。
    pub(crate) fn record_count(&self) -> i64 {
        // connection inspects the existing fixture runtime without acquiring another database owner.
        // connection 检查既有夹具运行时，不创建另一个数据库所有者。
        let connection = self
            .store
            .controller
            .local
            .connect(vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState)
            .expect("private credential database");
        connection
            .query_row(
                &format!(
                    "SELECT count(*) FROM {}",
                    vulcan_workspace_store::data_plane::AI_CREDENTIAL_SECRETS_TABLE
                ),
                [],
                |row| row.get(0),
            )
            .expect("credential row count")
    }
}

/// Ensures old and unknown formats remain rejected while deletion and exact compensation stay available.
/// 确保旧版和未知格式始终被拒绝，同时允许删除与精确补偿。
/// No inputs; assertions cover identity, absence, and every original relational field.
/// 无输入；断言覆盖身份、缺失和每个原始关系化字段。
#[tokio::test]
async fn credential_repair_deletes_and_restores_unsupported_records() {
    // fixture owns an independent production-shaped database.
    // fixture 持有独立的生产形态数据库。
    let fixture = CredentialRepairFixture::new().await;
    for version in [UNSUPPORTED_FORMAT_VERSION, UNSUPPORTED_FORMAT_VERSION + 1] {
        // reference isolates each version's compensation history.
        // reference 隔离每个版本的补偿历史。
        let reference = format!("unsupported-{version}");
        // original is intentionally never decoded as executable credential material.
        // original 刻意不被解码为可执行凭据材料。
        let original = fixture.insert(&reference, version).await;
        assert!(fixture.store.read_secret(&reference).await.is_err());
        // snapshot is a raw before-image, not a compatibility envelope.
        // snapshot 是原始前镜像，不是兼容封装。
        let snapshot = fixture
            .store
            .capture_secret(&reference)
            .await
            .expect("capture")
            .expect("present");
        fixture
            .store
            .delete_secret(&reference)
            .await
            .expect("delete unsupported row");
        assert!(fixture.record(&reference).await.is_none());
        assert!(
            fixture
                .store
                .restore_snapshot("wrong-reference", &snapshot)
                .await
                .is_err()
        );
        fixture
            .store
            .restore_snapshot(&reference, &snapshot)
            .await
            .expect("restore original row");
        assert!(fixture.record(&reference).await.as_ref() == Some(&original));
        assert!(fixture.store.read_secret(&reference).await.is_err());
        assert!(
            fixture
                .store
                .restore_snapshot(&reference, &snapshot)
                .await
                .is_err()
        );
    }
}
