//! Write operations applied atomically across the scope files.
//! 跨各层文件原子应用的写入操作。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Deserialize;

use crate::document::{node_heading_levels, stored_body, title_round_trips};
use crate::scope::{BASE_SECTION_KEY, preset_sections};
use crate::snapshot::{BudgetUsage, base_usage};
use crate::store::{PreparedFile, commit_files, write_guard};
use crate::timestamp::now_utc_minute;
use crate::{
    InjectionBudget, MAX_FILE_BYTES, MAX_NODE_BODY_BYTES, MAX_NODE_LEVEL, MAX_OPERATIONS_PER_WRITE,
    MAX_TITLE_CHARS, MarkdownMemoryStore, MemoryDocument, MemoryError, MemoryNode, MemoryRegistry,
    MemoryScope, NodeMeta, NodeSource, NodeView, ProjectContext,
};

/// Where a created or moved node goes.
/// 新建或移动节点的去向。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodePlacement {
    /// Destination scope; a move without it stays in the node's scope.
    /// 目标层；移动时省略则留在节点原层。
    pub scope: Option<MemoryScope>,
    /// Parent node id; the node becomes its last child.
    /// 父节点标识；节点成为其最后一个子节点。
    pub parent_id: Option<String>,
    /// Section key or H1 title; the node becomes its last child.
    /// 分区键或 H1 标题；节点成为其最后一个子节点。
    pub section: Option<String>,
    /// Makes the node a new H1 section of the scope.
    /// 使节点成为该层新的 H1 分区。
    pub new_section: bool,
}

/// One requested change.
/// 一项请求的变更。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WriteOperation {
    /// Creates a node.
    /// 新建节点。
    Create {
        /// Title of the new node.
        /// 新节点标题。
        title: String,
        /// Markdown body.
        /// Markdown 正文。
        #[serde(default)]
        body: String,
        /// Destination scope; required for create, a move without it stays in the node's scope.
        /// 目标层；新建时必填，移动时省略则留在节点原层。
        #[serde(default)]
        scope: Option<MemoryScope>,
        /// Parent node id; the node becomes its last child.
        /// 父节点标识；节点成为其最后一个子节点。
        #[serde(default)]
        parent_id: Option<String>,
        /// Section key or H1 title; the node becomes its last child.
        /// 分区键或 H1 标题；节点成为其最后一个子节点。
        #[serde(default)]
        section: Option<String>,
        /// Makes the node a new H1 section of the scope.
        /// 使节点成为该层新的 H1 分区。
        #[serde(default)]
        new_section: bool,
    },
    /// Replaces a node's own body.
    /// 替换节点自身正文。
    Replace {
        /// Node id.
        /// 节点标识。
        id: String,
        /// Revision the caller read.
        /// 调用方读取到的修订。
        expected_revision: String,
        /// New Markdown body.
        /// 新的 Markdown 正文。
        body: String,
    },
    /// Appends text to a node's own body.
    /// 向节点自身正文追加文本。
    Append {
        /// Node id.
        /// 节点标识。
        id: String,
        /// Revision the caller read.
        /// 调用方读取到的修订。
        expected_revision: String,
        /// Markdown text appended after a blank line.
        /// 在空行后追加的 Markdown 文本。
        text: String,
    },
    /// Renames a node.
    /// 重命名节点。
    Rename {
        /// Node id.
        /// 节点标识。
        id: String,
        /// Revision the caller read.
        /// 调用方读取到的修订。
        expected_revision: String,
        /// New title.
        /// 新标题。
        title: String,
    },
    /// Moves a node with its children.
    /// 连同子节点移动节点。
    Move {
        /// Node id.
        /// 节点标识。
        id: String,
        /// Revision the caller read.
        /// 调用方读取到的修订。
        expected_revision: String,
        /// Destination scope; required for create, a move without it stays in the node's scope.
        /// 目标层；新建时必填，移动时省略则留在节点原层。
        #[serde(default)]
        scope: Option<MemoryScope>,
        /// Parent node id; the node becomes its last child.
        /// 父节点标识；节点成为其最后一个子节点。
        #[serde(default)]
        parent_id: Option<String>,
        /// Section key or H1 title; the node becomes its last child.
        /// 分区键或 H1 标题；节点成为其最后一个子节点。
        #[serde(default)]
        section: Option<String>,
        /// Makes the node a new H1 section of the scope.
        /// 使节点成为该层新的 H1 分区。
        #[serde(default)]
        new_section: bool,
    },
    /// Deletes a node.
    /// 删除节点。
    Delete {
        /// Node id.
        /// 节点标识。
        id: String,
        /// Revision the caller read.
        /// 调用方读取到的修订。
        expected_revision: String,
        /// Required when the node has children; they are deleted too.
        /// 节点有子节点时必须为真；子节点一并删除。
        #[serde(default)]
        with_children: bool,
    },
}

/// A node that may duplicate a written one.
/// 可能与写入节点重复的节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateHint {
    /// Node id.
    /// 节点标识。
    pub id: String,
    /// Scope of the node.
    /// 节点所属层。
    pub scope: MemoryScope,
    /// Title path.
    /// 标题路径。
    pub path: Vec<String>,
}

/// Result of one applied operation.
/// 单个已应用操作的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOperationReceipt {
    /// Stable action key.
    /// 稳定动作键。
    pub action: &'static str,
    /// Node id after the operation; a node written for the first time receives a stable id.
    /// 操作后的节点标识；首次被写入的节点获得稳定标识。
    pub id: String,
    /// Scope after the operation.
    /// 操作后所属层。
    pub scope: MemoryScope,
    /// Title path after the operation.
    /// 操作后的标题路径。
    pub path: Vec<String>,
    /// Revision after the operation; empty after a delete.
    /// 操作后的修订；删除后为空。
    pub revision: String,
    /// Whether the operation created a new H1 section.
    /// 操作是否新建了 H1 分区。
    pub new_section: bool,
    /// Number of child nodes deleted with the node.
    /// 随节点一同删除的子节点数。
    pub deleted_children: usize,
    /// Similar existing nodes worth merging.
    /// 值得合并的相似既有节点。
    pub duplicates: Vec<DuplicateHint>,
}

/// Result of a whole write call.
/// 整个写入调用的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReceipt {
    /// Per-operation results in request order.
    /// 按请求顺序排列的各操作结果。
    pub operations: Vec<WriteOperationReceipt>,
    /// Usage of the injected full-text parts after the write.
    /// 写入后全文注入部分的占用。
    pub usage: Vec<BudgetUsage>,
    /// Memory project created for the host project by this write.
    /// 本次写入为宿主项目新建的记忆项目。
    pub created_project: Option<String>,
}

impl WriteReceipt {
    /// Renders the receipt as model-facing English text.
    /// 将回执渲染为面向模型的英文文本。
    pub fn render(&self) -> String {
        let mut lines = vec![format!(
            "Applied {} memory operation(s).",
            self.operations.len()
        )];
        if let Some(project) = &self.created_project {
            lines.push(format!(
                "Created project memory `{project}` for this project."
            ));
        }
        for operation in &self.operations {
            let mut line = format!(
                "- {} {} [{}] {}",
                operation.action,
                operation.id,
                operation.scope.key(),
                operation.path.join(" › ")
            );
            if !operation.revision.is_empty() {
                line.push_str(&format!(" (revision {})", operation.revision));
            }
            if operation.new_section {
                line.push_str(" · new section");
            }
            if operation.deleted_children > 0 {
                line.push_str(&format!(
                    " · {} child node(s) deleted",
                    operation.deleted_children
                ));
            }
            lines.push(line);
            if !operation.duplicates.is_empty() {
                let hints = operation
                    .duplicates
                    .iter()
                    .map(|hint| {
                        format!(
                            "{} [{}] {}",
                            hint.id,
                            hint.scope.key(),
                            hint.path.join(" › ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                lines.push(format!(
                    "  possible duplicates (merge if they cover the same fact): {hints}"
                ));
            }
        }
        for usage in &self.usage {
            lines.push(usage.render_line());
            if usage.over_budget() {
                lines.push(String::from(
                    "  Base is over budget: move less essential nodes from the base section to a catalog section.",
                ));
            } else if usage.near_budget() {
                lines.push(String::from(
                    "  Base is nearly full: keep only rules that would cause mistakes if unknown.",
                ));
            }
        }
        lines.join("\n")
    }
}

/// One scope file loaded for a write.
/// 为写入加载的单个层文件。
struct LoadedScope {
    /// File path.
    /// 文件路径。
    path: PathBuf,
    /// Exact text read, or `None` when missing.
    /// 读取到的精确文本；缺失时为 `None`。
    original: Option<String>,
    /// Working document.
    /// 工作文档。
    document: MemoryDocument,
    /// Whether an operation changed the document.
    /// 是否有操作修改了文档。
    changed: bool,
}

/// Mutable state of one write call.
/// 单次写入调用的可变状态。
struct WriteSession<'store> {
    /// Store handle.
    /// 存储句柄。
    store: &'store MarkdownMemoryStore,
    /// Request project.
    /// 请求所属项目。
    context: &'store ProjectContext,
    /// Registry as loaded under the write lock.
    /// 在写锁下加载的注册表。
    registry: MemoryRegistry,
    /// Bound or newly created memory project id.
    /// 已绑定或新建的记忆项目标识。
    project_id: Option<String>,
    /// Memory project created by this call.
    /// 本次调用新建的记忆项目。
    created_project: Option<String>,
    /// Loaded scopes.
    /// 已加载的层。
    scopes: BTreeMap<MemoryScope, LoadedScope>,
    /// Conversation recorded in metadata.
    /// 记录在元数据中的会话。
    session: Option<String>,
    /// Writer recorded in metadata.
    /// 记录在元数据中的写入方。
    source: NodeSource,
    /// Timestamp shared by every change of this call.
    /// 本次调用全部变更共用的时间戳。
    now: String,
}

impl MarkdownMemoryStore {
    /// Applies operations in order; failed commits restore originals or report an incomplete rollback.
    /// 按顺序应用操作；提交失败时还原原文或报告未完成的回滚。
    pub fn write(
        &self,
        context: &ProjectContext,
        session: Option<&str>,
        source: NodeSource,
        budget: &InjectionBudget,
        operations: Vec<WriteOperation>,
    ) -> Result<WriteReceipt, MemoryError> {
        if operations.is_empty() || operations.len() > MAX_OPERATIONS_PER_WRITE {
            return Err(MemoryError::invalid(format!(
                "Provide 1 to {MAX_OPERATIONS_PER_WRITE} operations."
            )));
        }
        let guard = write_guard();
        let registry = self.registry()?;
        let project_id = context
            .bound_project(&registry)?
            .map(|project| project.id.clone());
        let mut session = WriteSession {
            store: self,
            context,
            registry,
            project_id,
            created_project: None,
            scopes: BTreeMap::new(),
            session: session.map(str::to_string),
            source,
            now: now_utc_minute(),
        };
        let mut receipts = Vec::with_capacity(operations.len());
        for (index, operation) in operations.into_iter().enumerate() {
            let receipt = session
                .apply(operation)
                .map_err(|error| MemoryError::Operation {
                    index: index + 1,
                    source: Box::new(error),
                })?;
            receipts.push(receipt);
        }
        session.finish(receipts, budget, &guard)
    }
}

impl WriteSession<'_> {
    /// Loads a scope on first use, seeding preset sections into a missing or blank file.
    /// 首次使用时加载某层；缺失或空白文件会写入预置分区。
    fn scope(&mut self, scope: MemoryScope) -> Result<&mut LoadedScope, MemoryError> {
        if !self.scopes.contains_key(&scope) {
            let project_id = if scope == MemoryScope::Project {
                Some(self.ensure_project()?)
            } else {
                None
            };
            let path = self.store.scope_path(scope, project_id.as_deref())?;
            let original = MarkdownMemoryStore::read_optional(&path)?;
            let blank = original
                .as_deref()
                .is_none_or(|text| text.trim().is_empty());
            let (document, changed) = if blank {
                (preset_document(scope, &self.now), true)
            } else {
                (
                    MemoryDocument::parse(original.as_deref().unwrap_or_default()),
                    false,
                )
            };
            self.scopes.insert(
                scope,
                LoadedScope {
                    path,
                    original,
                    document,
                    changed,
                },
            );
        }
        Ok(self.scopes.get_mut(&scope).expect("scope inserted above"))
    }

    /// Returns the bound memory project, creating and binding one for the host project if needed.
    /// 返回已绑定的记忆项目；需要时为宿主项目新建并绑定。
    fn ensure_project(&mut self) -> Result<String, MemoryError> {
        if let Some(id) = &self.project_id {
            // Users may still edit a project whose memory is off; only AI writes are refused.
            // 用户仍可编辑已关闭记忆的项目；只拒绝 AI 写入。
            if self.source == NodeSource::Ai
                && self
                    .registry
                    .project(id)
                    .is_some_and(|project| !project.memory_enabled)
            {
                return Err(MemoryError::ProjectUnavailable(String::from(
                    "Memory is turned off for this project.",
                )));
            }
            return Ok(id.clone());
        }
        let (Some(host), Some(root)) = (
            self.context.host_project_id.as_deref(),
            self.context.root_path.as_deref(),
        ) else {
            return Err(MemoryError::ProjectUnavailable(String::from(
                "This conversation has no project, so project memory is unavailable.",
            )));
        };
        let id = self.registry.create_for_host(host, root, None, &self.now);
        self.project_id = Some(id.clone());
        self.created_project = Some(id.clone());
        Ok(id)
    }

    /// Returns views of every scope that is loaded or can be loaded without creating a project.
    /// 返回已加载或无需新建项目即可加载的各层视图。
    fn all_views(&mut self) -> Result<Vec<NodeView>, MemoryError> {
        let mut scopes = vec![MemoryScope::User, MemoryScope::Space];
        if self.project_id.is_some() {
            scopes.push(MemoryScope::Project);
        }
        let mut views = Vec::new();
        for scope in scopes {
            views.extend(self.scope(scope)?.document.views(scope));
        }
        Ok(views)
    }

    /// Finds a node by id across scopes and checks its revision.
    /// 跨层按标识查找节点并校验修订。
    fn locate(&mut self, id: &str, expected_revision: &str) -> Result<NodeView, MemoryError> {
        let view = self
            .all_views()?
            .into_iter()
            .find(|view| view.id == id)
            .ok_or_else(|| MemoryError::NodeNotFound(id.to_string()))?;
        if view.revision != expected_revision.trim() {
            return Err(MemoryError::RevisionMismatch {
                id: id.to_string(),
                expected: expected_revision.to_string(),
                current: view.revision,
            });
        }
        Ok(view)
    }

    /// Applies one operation.
    /// 应用单个操作。
    fn apply(&mut self, operation: WriteOperation) -> Result<WriteOperationReceipt, MemoryError> {
        match operation {
            WriteOperation::Create {
                title,
                body,
                scope,
                parent_id,
                section,
                new_section,
            } => self.create(
                title,
                body,
                NodePlacement {
                    scope,
                    parent_id,
                    section,
                    new_section,
                },
            ),
            WriteOperation::Replace {
                id,
                expected_revision,
                body,
            } => {
                let view = self.locate(&id, &expected_revision)?;
                validate_body(&body)?;
                self.update_node(&view, "replace", |node, eol| {
                    node.set_body(stored_body(&body, eol));
                    Ok(())
                })
            }
            WriteOperation::Append {
                id,
                expected_revision,
                text,
            } => {
                let view = self.locate(&id, &expected_revision)?;
                if text.trim().is_empty() {
                    return Err(MemoryError::invalid("Append text must not be empty."));
                }
                let combined = if view.body.is_empty() {
                    text.trim().to_string()
                } else {
                    format!("{}\n\n{}", view.body, text.trim())
                };
                validate_body(&combined)?;
                self.update_node(&view, "append", |node, eol| {
                    node.set_body(stored_body(&combined, eol));
                    Ok(())
                })
            }
            WriteOperation::Rename {
                id,
                expected_revision,
                title,
            } => {
                let view = self.locate(&id, &expected_revision)?;
                let title = validate_title(&title, view.level)?;
                self.update_node(&view, "rename", |node, _| {
                    node.set_title(title.clone());
                    Ok(())
                })
            }
            WriteOperation::Move {
                id,
                expected_revision,
                scope,
                parent_id,
                section,
                new_section,
            } => self.move_node(
                &id,
                &expected_revision,
                NodePlacement {
                    scope,
                    parent_id,
                    section,
                    new_section,
                },
            ),
            WriteOperation::Delete {
                id,
                expected_revision,
                with_children,
            } => self.delete(&id, &expected_revision, with_children),
        }
    }

    /// Creates a node at a placement.
    /// 在指定去向新建节点。
    fn create(
        &mut self,
        title: String,
        body: String,
        placement: NodePlacement,
    ) -> Result<WriteOperationReceipt, MemoryError> {
        let scope = placement
            .scope
            .ok_or_else(|| MemoryError::invalid("create requires `scope`."))?;
        validate_body(&body)?;
        let (parent, level, section_key) = self.resolve_placement(scope, &placement, None)?;
        let title = validate_title(&title, level)?;
        let duplicates = self.duplicates(scope, &title, &body, None)?;
        let id = self.fresh_id()?;
        let meta = NodeMeta {
            id: id.clone(),
            section: section_key.clone(),
            created: self.now.clone(),
            updated: self.now.clone(),
            session: self.session.clone(),
            source: self.source,
        };
        let loaded = self.scope(scope)?;
        let node = MemoryNode::new(
            level,
            title,
            meta,
            stored_body(&body, loaded.document.eol()),
        );
        loaded.document.append(parent.as_deref(), node);
        loaded.changed = true;
        let view = self.view_by_id(scope, &id)?;
        Ok(WriteOperationReceipt {
            action: "create",
            id,
            scope,
            path: view.path,
            revision: view.revision,
            new_section: section_key.is_some(),
            deleted_children: 0,
            duplicates,
        })
    }

    /// Moves a subtree to a placement.
    /// 将子树移动到指定去向。
    fn move_node(
        &mut self,
        id: &str,
        expected_revision: &str,
        placement: NodePlacement,
    ) -> Result<WriteOperationReceipt, MemoryError> {
        let view = self.locate(id, expected_revision)?;
        guard_base_section(&view, "moved")?;
        let target_scope = placement.scope.unwrap_or(view.scope);
        let source = self.scope(view.scope)?;
        let mut node = source
            .document
            .remove(&view.locator)
            .ok_or_else(|| MemoryError::NodeNotFound(id.to_string()))?;
        source.changed = true;
        let (parent, level, section_key) =
            self.resolve_placement(target_scope, &placement, Some(id))?;
        node.relevel(level).map_err(|deepest| {
            MemoryError::invalid(format!(
                "Moving there would put nodes at heading level {deepest}; memory nodes stop at H{MAX_NODE_LEVEL}."
            ))
        })?;
        let mut meta = self.touched_meta(node.meta.as_ref())?;
        meta.section = section_key.clone();
        let new_id = meta.id.clone();
        node.set_meta(meta);
        let target = self.scope(target_scope)?;
        target.document.append(parent.as_deref(), node);
        target.changed = true;
        let moved = self.view_by_id(target_scope, &new_id)?;
        Ok(WriteOperationReceipt {
            action: "move",
            id: new_id,
            scope: target_scope,
            path: moved.path,
            revision: moved.revision,
            new_section: section_key.is_some(),
            deleted_children: 0,
            duplicates: Vec::new(),
        })
    }

    /// Deletes a node, refusing the base section and unconfirmed children.
    /// 删除节点；拒绝基础分区与未确认的子节点。
    fn delete(
        &mut self,
        id: &str,
        expected_revision: &str,
        with_children: bool,
    ) -> Result<WriteOperationReceipt, MemoryError> {
        let view = self.locate(id, expected_revision)?;
        guard_base_section(&view, "deleted")?;
        if view.descendants > 0 && !with_children {
            return Err(MemoryError::invalid(format!(
                "Node `{id}` has {} child node(s); set withChildren to delete them too, or move them first.",
                view.descendants
            )));
        }
        let loaded = self.scope(view.scope)?;
        loaded
            .document
            .remove(&view.locator)
            .ok_or_else(|| MemoryError::NodeNotFound(id.to_string()))?;
        loaded.changed = true;
        Ok(WriteOperationReceipt {
            action: "delete",
            id: view.id,
            scope: view.scope,
            path: view.path,
            revision: String::new(),
            new_section: false,
            deleted_children: view.descendants,
            duplicates: Vec::new(),
        })
    }

    /// Applies an in-place change and refreshes the node's metadata.
    /// 应用原位变更并刷新节点元数据。
    fn update_node(
        &mut self,
        view: &NodeView,
        action: &'static str,
        change: impl FnOnce(&mut MemoryNode, &'static str) -> Result<(), MemoryError>,
    ) -> Result<WriteOperationReceipt, MemoryError> {
        let meta = self.touched_meta(view.meta.as_ref())?;
        let id = meta.id.clone();
        let loaded = self.scope(view.scope)?;
        let eol = loaded.document.eol();
        let node = loaded
            .document
            .node_mut(&view.locator)
            .ok_or_else(|| MemoryError::NodeNotFound(view.id.clone()))?;
        change(node, eol)?;
        node.set_meta(meta);
        loaded.changed = true;
        let updated = self.view_by_id(view.scope, &id)?;
        let duplicates = if action == "rename" {
            let title = updated.path.last().cloned().unwrap_or_default();
            self.duplicates(view.scope, &title, &updated.body, Some(&id))?
        } else {
            Vec::new()
        };
        Ok(WriteOperationReceipt {
            action,
            id,
            scope: view.scope,
            path: updated.path,
            revision: updated.revision,
            new_section: false,
            deleted_children: 0,
            duplicates,
        })
    }

    /// Returns metadata for a written node: existing identity with a new update time, or a new identity.
    /// 返回被写入节点的元数据：沿用既有身份并更新时间，或分配新身份。
    fn touched_meta(&mut self, existing: Option<&NodeMeta>) -> Result<NodeMeta, MemoryError> {
        Ok(match existing {
            Some(meta) => NodeMeta {
                updated: self.now.clone(),
                session: self.session.clone().or_else(|| meta.session.clone()),
                source: self.source,
                ..meta.clone()
            },
            None => NodeMeta {
                id: self.fresh_id()?,
                section: None,
                created: self.now.clone(),
                updated: self.now.clone(),
                session: self.session.clone(),
                source: self.source,
            },
        })
    }

    /// Resolves a placement to `(parent locator, level, new section key)`.
    /// 将去向解析为 `(父定位链, 级别, 新分区键)`。
    fn resolve_placement(
        &mut self,
        scope: MemoryScope,
        placement: &NodePlacement,
        moving_id: Option<&str>,
    ) -> Result<ResolvedPlacement, MemoryError> {
        let choices = [
            placement.parent_id.is_some(),
            placement.section.is_some(),
            placement.new_section,
        ]
        .into_iter()
        .filter(|chosen| *chosen)
        .count();
        if choices != 1 {
            return Err(MemoryError::invalid(
                "Give exactly one of `parentId`, `section` or `newSection`.",
            ));
        }
        let views = self.scope(scope)?.document.views(scope);
        if let Some(parent_id) = &placement.parent_id {
            let parent = views.iter().find(|view| &view.id == parent_id).ok_or_else(|| {
                if moving_id.is_some() {
                    MemoryError::invalid(format!(
                        "Parent `{parent_id}` is not in the {} scope, or it is inside the node being moved.",
                        scope.key()
                    ))
                } else {
                    MemoryError::invalid(format!(
                        "Parent `{parent_id}` is not in the {} scope.",
                        scope.key()
                    ))
                }
            })?;
            if parent.level >= MAX_NODE_LEVEL {
                return Err(MemoryError::invalid(format!(
                    "Parent `{parent_id}` is already H{MAX_NODE_LEVEL}; add the content to its body or choose a shallower parent."
                )));
            }
            return Ok((Some(parent.locator.clone()), parent.level + 1, None));
        }
        if let Some(section) = &placement.section {
            let wanted = section.trim();
            let parent = views
                .iter()
                .find(|view| view.level == 1 && view.section.as_deref() == Some(wanted))
                .or_else(|| {
                    views.iter().find(|view| {
                        view.locator.len() == 1
                            && view.level == 1
                            && view
                                .path
                                .first()
                                .is_some_and(|title| title.eq_ignore_ascii_case(wanted))
                    })
                })
                .ok_or_else(|| {
                    let existing = views
                        .iter()
                        .filter(|view| view.locator.len() == 1 && view.level == 1)
                        .map(|view| match &view.section {
                            Some(key) => key.clone(),
                            None => view.path[0].clone(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    MemoryError::invalid(format!(
                        "Section `{wanted}` does not exist in the {} scope (existing: {existing}). Use one of them, or set newSection to declare a new one.",
                        scope.key()
                    ))
                })?;
            return Ok((Some(parent.locator.clone()), 2, None));
        }
        let taken = views
            .iter()
            .filter_map(|view| view.section.clone())
            .collect::<BTreeSet<_>>();
        let key = loop {
            let candidate = format!("s-{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
            if !taken.contains(&candidate) {
                break candidate;
            }
        };
        Ok((None, 1, Some(key)))
    }

    /// Generates a node id unused in every loaded scope.
    /// 生成在所有已加载层中均未使用的节点标识。
    fn fresh_id(&mut self) -> Result<String, MemoryError> {
        let used = self
            .all_views()?
            .into_iter()
            .map(|view| view.id)
            .collect::<BTreeSet<_>>();
        Ok(loop {
            let candidate = format!("m-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
            if !used.contains(&candidate) {
                break candidate;
            }
        })
    }

    /// Returns the current view of a node in a scope.
    /// 返回某层中节点的当前视图。
    fn view_by_id(&mut self, scope: MemoryScope, id: &str) -> Result<NodeView, MemoryError> {
        self.scope(scope)?
            .document
            .views(scope)
            .into_iter()
            .find(|view| view.id == id)
            .ok_or_else(|| MemoryError::NodeNotFound(id.to_string()))
    }

    /// Finds similar nodes in the same scope, and in the space scope for project writes.
    /// 在同层查找相似节点；项目层写入同时检查空间层。
    fn duplicates(
        &mut self,
        scope: MemoryScope,
        title: &str,
        body: &str,
        exclude: Option<&str>,
    ) -> Result<Vec<DuplicateHint>, MemoryError> {
        let mut scopes = vec![scope];
        if scope == MemoryScope::Project {
            scopes.push(MemoryScope::Space);
        }
        let mut scored = Vec::new();
        for candidate_scope in scopes {
            for view in self.scope(candidate_scope)?.document.views(candidate_scope) {
                if Some(view.id.as_str()) == exclude || view.level == 1 {
                    continue;
                }
                let score = similarity(
                    title,
                    body,
                    view.path.last().map_or("", String::as_str),
                    &view.body,
                );
                if score >= DUPLICATE_THRESHOLD {
                    scored.push((score, view));
                }
            }
        }
        scored.sort_by(|left, right| right.0.total_cmp(&left.0));
        Ok(scored
            .into_iter()
            .take(3)
            .map(|(_, view)| DuplicateHint {
                id: view.id,
                scope: view.scope,
                path: view.path,
            })
            .collect())
    }

    /// Validates file sizes and commits every changed file together with the registry.
    /// 校验文件大小，并将全部变更文件与注册表一起提交。
    fn finish(
        mut self,
        operations: Vec<WriteOperationReceipt>,
        budget: &InjectionBudget,
        guard: &crate::store::MemoryWriteGuard,
    ) -> Result<WriteReceipt, MemoryError> {
        let mut files = Vec::new();
        for loaded in self.scopes.values().filter(|loaded| loaded.changed) {
            let text = loaded.document.render();
            if text.len() > MAX_FILE_BYTES {
                return Err(MemoryError::invalid(format!(
                    "`{}` would grow to {} bytes, above the {MAX_FILE_BYTES}-byte limit; merge or delete outdated nodes first.",
                    loaded.path.display(),
                    text.len()
                )));
            }
            files.push(PreparedFile {
                path: loaded.path.clone(),
                original: loaded.original.clone(),
                text,
            });
        }
        if self.created_project.is_some() {
            let directory = self.store.markdown_directory();
            let path = directory.join(MemoryRegistry::FILE_NAME);
            let original = MarkdownMemoryStore::read_optional(&path)?;
            let mut text = self.registry.to_json()?;
            text.push('\n');
            files.push(PreparedFile {
                path,
                original,
                text,
            });
        }
        commit_files(guard, files)?;
        let touched = operations
            .iter()
            .map(|operation| operation.scope)
            .collect::<BTreeSet<_>>();
        let mut usage = Vec::new();
        for scope in touched {
            let document = &self.scope(scope)?.document;
            usage.push(base_usage(scope, document, budget));
        }
        Ok(WriteReceipt {
            operations,
            usage,
            created_project: self.created_project,
        })
    }
}

/// A resolved placement: `(parent locator, heading level, new section key)`.
/// 已解析的去向：`(父定位链, 标题级别, 新分区键)`。
type ResolvedPlacement = (Option<Vec<usize>>, u8, Option<String>);

/// Similarity at or above which a node is reported as a possible duplicate.
/// 达到或超过该值的节点被报告为可能重复。
const DUPLICATE_THRESHOLD: f64 = 0.6;

/// Scores two nodes by title character bigrams and body word overlap.
/// 按标题字符二元组与正文词重合度为两个节点打分。
fn similarity(title: &str, body: &str, other_title: &str, other_body: &str) -> f64 {
    let titles = jaccard(&title_bigrams(title), &title_bigrams(other_title));
    let left = body_terms(body);
    let right = body_terms(other_body);
    let bodies = if left.len() >= 8 && right.len() >= 8 {
        jaccard(&left, &right) * 0.8
    } else {
        0.0
    };
    titles.max(bodies)
}

/// Returns lowercase character bigrams of a title, ignoring spaces and punctuation.
/// 返回标题的小写字符二元组，忽略空白与标点。
fn title_bigrams(title: &str) -> BTreeSet<String> {
    let characters = title
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<Vec<_>>();
    if characters.len() == 1 {
        return characters.iter().map(char::to_string).collect();
    }
    characters
        .windows(2)
        .map(|pair| pair.iter().collect())
        .collect()
}

/// Returns body terms: lowercase ASCII words of three or more letters and CJK bigrams.
/// 返回正文词项：三个字母以上的小写 ASCII 单词与中日韩二元组。
fn body_terms(body: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    for word in body
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| word.len() >= 3)
    {
        terms.insert(word.to_ascii_lowercase());
    }
    let cjk = body
        .chars()
        .filter(|character| vulcan_context::is_cjk_character(*character))
        .collect::<Vec<_>>();
    for pair in cjk.windows(2) {
        terms.insert(pair.iter().collect());
    }
    terms
}

/// Returns the Jaccard index of two sets.
/// 返回两个集合的 Jaccard 指数。
fn jaccard(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    let union = left.union(right).count();
    if union == 0 {
        return 0.0;
    }
    left.intersection(right).count() as f64 / union as f64
}

/// Refuses to move or delete the base section.
/// 拒绝移动或删除基础分区。
fn guard_base_section(view: &NodeView, verb: &str) -> Result<(), MemoryError> {
    if view.level == 1
        && view.locator.len() == 1
        && view.meta.as_ref().and_then(|meta| meta.section.as_deref()) == Some(BASE_SECTION_KEY)
    {
        return Err(MemoryError::invalid(format!(
            "The base section cannot be {verb}; change the nodes inside it instead."
        )));
    }
    Ok(())
}

/// Validates a title for a heading level and returns it trimmed.
/// 校验某标题级别下的标题并返回去除空白后的值。
fn validate_title(title: &str, level: u8) -> Result<String, MemoryError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(MemoryError::invalid(format!(
            "A title must have 1 to {MAX_TITLE_CHARS} characters."
        )));
    }
    if title.starts_with('#') || !title_round_trips(level, title) {
        return Err(MemoryError::invalid(format!(
            "Title `{title}` must be a single line of plain heading text (no leading or trailing `#`)."
        )));
    }
    Ok(title.to_string())
}

/// Validates a body: size, no node-forming headings and no metadata comments.
/// 校验正文：大小、不含会形成节点的标题、不含元数据注释。
fn validate_body(body: &str) -> Result<(), MemoryError> {
    if body.len() > MAX_NODE_BODY_BYTES {
        return Err(MemoryError::invalid(format!(
            "A node body can hold at most {MAX_NODE_BODY_BYTES} bytes; split it into child nodes."
        )));
    }
    if let Some(level) = node_heading_levels(body).first() {
        return Err(MemoryError::invalid(format!(
            "The body contains an H{level} heading, which would create a separate node; create a child node instead, or use H5/H6 inside the body."
        )));
    }
    if body
        .lines()
        .any(|line| line.trim_start().starts_with("<!-- mem"))
    {
        return Err(MemoryError::invalid(
            "The body must not contain `<!-- mem` metadata comments; the host maintains them.",
        ));
    }
    Ok(())
}

/// Builds a document holding the scope's preset sections.
/// 构建包含该层预置分区的文档。
fn preset_document(scope: MemoryScope, now: &str) -> MemoryDocument {
    let mut document = MemoryDocument::empty();
    for (key, title) in preset_sections(scope) {
        let meta = NodeMeta {
            id: format!("m-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            section: Some((*key).to_string()),
            created: now.to_string(),
            updated: now.to_string(),
            session: None,
            source: NodeSource::User,
        };
        document.append(
            None,
            MemoryNode::new(1, (*title).to_string(), meta, String::from("\n")),
        );
    }
    document
}

#[cfg(test)]
mod tests;
