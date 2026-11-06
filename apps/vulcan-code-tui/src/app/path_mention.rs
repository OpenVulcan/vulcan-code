// Path mention: typing @ at the start of a word in the composer lists the project's files and
// folders one folder level at a time, the way a shell completes a path, as the WebUI composer does
// (`use-composer-path-mention.ts`). Picking a folder steps into it; picking a file writes its
// project-relative path after the @, which the model reads like any other path in the prompt.
// 路径提及：在输入框词首输入 @ 会列出项目的文件与文件夹，像 shell 补全路径那样逐级展开，与 WebUI 输入区
// （`use-composer-path-mention.ts`）一致。选择文件夹会进入该文件夹；选择文件会在 @ 后写入其项目相对路径，
// 模型会像读取提示词中的其他路径一样读取它。

use ratatui_textarea::{CursorMove, DataCursor};
use serde_json::Value;
use vulcan_service_api::{WorkspaceFileDirectoryListing, WorkspaceFileEntrySummary};

use super::{App, Focus};
use crate::backend::Command;
use crate::service_api::{ServiceCall, Slot, workbench_snapshot};

/// The @ word around the cursor: its line, its character range in that line and what is typed
/// between the @ and the cursor.
/// 光标所在的 @ 词：所在行、在该行中的字符范围，以及 @ 与光标之间已输入的内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub row: usize,
    pub start: usize,
    pub end: usize,
    pub query: String,
}

/// What the composer remembers about the open mention list.
/// 输入框为打开的提及列表记住的状态。
#[derive(Debug, Default)]
pub struct PathMentionState {
    /// The highlighted suggestion.
    /// 高亮的建议项。
    pub highlighted: usize,
    /// The mention the highlight belongs to; another mention starts from the top.
    /// 高亮所属的提及；换成其他提及时从首项开始。
    key: Option<(usize, usize, String)>,
    /// The mention Esc closed, kept until the mention changes.
    /// 被 Esc 关闭的提及，保留到提及变化为止。
    dismissed: Option<(usize, usize, String)>,
    /// Parameters of the folder listing last asked for, and of the one the slot holds.
    /// 最近请求的文件夹列表参数，以及槽位中所存列表的参数。
    requested: Option<Value>,
    loaded: Option<Value>,
}

/// Finds the @ word the cursor is in; the @ must start the word so addresses like a@b are left
/// alone. `col` is the cursor's character column in `line`.
/// 查找光标所在的 @ 词；@ 必须位于词首，因此 a@b 这类地址不受影响。`col` 是光标在 `line` 中的字符列。
pub fn mention_at(row: usize, line: &str, col: usize) -> Option<Mention> {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    let mut start = col;
    while start > 0 && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    // A cursor in front of the @ (for example after Home) is not inside the word.
    // 位于 @ 之前的光标（例如按 Home 之后）不在该词内。
    if start == col || chars.get(start) != Some(&'@') {
        return None;
    }
    let mut end = col;
    while end < chars.len() && !chars[end].is_whitespace() {
        end += 1;
    }
    Some(Mention {
        row,
        start,
        end,
        query: chars[start + 1..col].iter().collect(),
    })
}

/// Separates the folder already typed from the name being typed in it; both slash styles count.
/// 将已输入的文件夹与其中正在输入的名称分开；两种斜杠都适用。
pub fn split_query(query: &str) -> (String, &str) {
    match query.rfind(['/', '\\']) {
        Some(slash) => (query[..slash].replace('\\', "/"), &query[slash + 1..]),
        None => (String::new(), query),
    }
}

/// The mention of one project entry: @ and its relative path with forward slashes, then a slash
/// for a folder, so its contents can be listed next, or a space for a file.
/// 单个项目条目的提及：@ 加上使用正斜杠的相对路径，文件夹之后接斜杠以便继续列出其内容，文件之后接空格。
pub fn mention_text(relative_path: &str, is_directory: bool) -> String {
    format!(
        "@{}{}",
        relative_path.replace('\\', "/"),
        if is_directory { "/" } else { " " }
    )
}

/// The folder's entries whose name starts with the typed text, then those that only contain it,
/// folders first in each group.
/// 名称以已输入文本开头的条目，其后是仅包含该文本的条目，每组中文件夹在前。
pub fn suggestions<'a>(
    entries: &'a [WorkspaceFileEntrySummary],
    prefix: &str,
) -> Vec<&'a WorkspaceFileEntrySummary> {
    let needle = prefix.to_lowercase();
    let by_kind = |a: &&WorkspaceFileEntrySummary, b: &&WorkspaceFileEntrySummary| {
        b.is_directory
            .cmp(&a.is_directory)
            .then_with(|| a.name.cmp(&b.name))
    };
    let (mut starts, mut contains): (Vec<_>, Vec<_>) = entries
        .iter()
        .filter(|entry| entry.name.to_lowercase().contains(&needle))
        .partition(|entry| entry.name.to_lowercase().starts_with(&needle));
    starts.sort_by(by_kind);
    contains.sort_by(by_kind);
    starts.extend(contains);
    starts
}

impl App {
    /// The @ word at the composer cursor, when the open conversation belongs to a project.
    /// 输入框光标处的 @ 词，仅当打开的会话属于某个项目时存在。
    pub fn path_mention(&self) -> Option<Mention> {
        self.mention_project()?;
        let DataCursor(row, col) = self.composer.cursor();
        mention_at(row, self.composer.lines().get(row)?, col)
    }

    /// The listing request for the mention's folder.
    /// 提及所在文件夹的列表请求。
    fn mention_call(&self, mention: &Mention) -> Option<ServiceCall> {
        let (path, project_id) = self.mention_project()?;
        let (directory, _) = split_query(&mention.query);
        Some(workbench_snapshot::load_workspace_file_directory(
            &path,
            Some(&project_id),
            self.selected.as_deref(),
            (!directory.is_empty()).then_some(directory.as_str()),
        ))
    }

    fn mention_project(&self) -> Option<(String, String)> {
        let project = self.project_of(self.conversation()?)?;
        Some((project.path.clone(), project.id.clone()))
    }

    /// Suggestions for the mention at the cursor, empty while its folder is loading, after Esc
    /// closed it or outside the composer.
    /// 光标处提及的建议；其文件夹加载中、被 Esc 关闭或焦点不在输入框时为空。
    pub fn path_mention_suggestions(&self) -> Vec<WorkspaceFileEntrySummary> {
        let Some(mention) = self
            .path_mention()
            .filter(|_| self.focus == Focus::Composer)
        else {
            return Vec::new();
        };
        let state = &self.path_mention_state;
        if state.dismissed.as_ref() == Some(&mention_key(&mention))
            || state.loaded.is_none()
            || state.loaded != self.mention_call(&mention).map(|call| call.params)
        {
            return Vec::new();
        }
        let Ok(listing) = serde_json::from_value::<WorkspaceFileDirectoryListing>(
            self.slots.value(Slot::PathMentionDirectory).clone(),
        ) else {
            return Vec::new();
        };
        let (_, prefix) = split_query(&mention.query);
        suggestions(&listing.entries, prefix)
            .into_iter()
            .cloned()
            .collect()
    }

    /// After the composer changed: asks for the mention's folder when it is not the one held or
    /// asked for, and starts the highlight from the top for a new mention.
    /// 输入框变化后：提及所在文件夹既非已持有也未请求时发起请求；新的提及从首项开始高亮。
    pub fn sync_path_mention(&mut self) {
        let Some(mention) = self.path_mention() else {
            self.path_mention_state.key = None;
            return;
        };
        let key = mention_key(&mention);
        if self.path_mention_state.key.as_ref() != Some(&key) {
            self.path_mention_state.key = Some(key);
            self.path_mention_state.highlighted = 0;
        }
        let Some(call) = self.mention_call(&mention) else {
            return;
        };
        let state = &self.path_mention_state;
        if state.loaded.as_ref() == Some(&call.params)
            || state.requested.as_ref() == Some(&call.params)
        {
            return;
        }
        self.path_mention_state.requested = Some(call.params.clone());
        self.backend.send(Command::Call(call));
    }

    /// Whether a folder listing answers the mention's latest request; the slot keeps only that one.
    /// 文件夹列表是否回应提及的最近请求；槽位只保留该列表。
    pub(super) fn path_mention_listing_is_current(&self, call: &ServiceCall) -> bool {
        self.path_mention_state.requested.as_ref() == Some(&call.params)
    }

    /// Records which request the slot now answers; a failed listing leaves it answering none.
    /// 记录槽位当前回应的请求；列表读取失败时不再回应任何请求。
    pub(super) fn path_mention_loaded(&mut self, call: Option<&ServiceCall>) {
        self.path_mention_state.loaded = call.map(|call| call.params.clone());
    }

    /// Moves the highlight by `step`, wrapping at either end.
    /// 按 `step` 移动高亮，在两端循环。
    pub fn step_path_mention(&mut self, step: isize) {
        let count = self.path_mention_suggestions().len();
        if count > 0 {
            let current = self.path_mention_state.highlighted.min(count - 1) as isize;
            self.path_mention_state.highlighted =
                (current + step).rem_euclid(count as isize) as usize;
        }
    }

    /// Closes the list for the mention at the cursor until the mention changes.
    /// 关闭光标处提及的列表，直到提及变化。
    pub fn dismiss_path_mention(&mut self) {
        self.path_mention_state.dismissed = self.path_mention().map(|m| mention_key(&m));
    }

    /// Writes suggestion `index` in place of the @ word: a folder stays open for its contents, a
    /// file ends the mention with a space (taking the place of one that followed it).
    /// 以第 `index` 项建议替换 @ 词：文件夹保持打开以列出其内容；文件以空格结束提及（替代其后原有的一个空格）。
    pub fn accept_path_mention(&mut self, index: usize) -> bool {
        let (Some(mention), Some(entry)) = (
            self.path_mention(),
            self.path_mention_suggestions().get(index).cloned(),
        ) else {
            return false;
        };
        let line = &self.composer.lines()[mention.row];
        let follows_space = line
            .chars()
            .nth(mention.end)
            .is_some_and(char::is_whitespace);
        let remove =
            mention.end - mention.start + usize::from(!entry.is_directory && follows_space);
        self.composer.cancel_selection();
        self.composer.move_cursor(CursorMove::Jump(
            u16::try_from(mention.row).unwrap_or(u16::MAX),
            u16::try_from(mention.start).unwrap_or(u16::MAX),
        ));
        self.composer.delete_str(remove);
        self.composer
            .insert_str(mention_text(&entry.relative_path, entry.is_directory));
        self.sync_path_mention();
        true
    }
}

fn mention_key(mention: &Mention) -> (usize, usize, String) {
    (mention.row, mention.start, mention.query.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_directory: bool) -> WorkspaceFileEntrySummary {
        serde_json::from_value(serde_json::json!({
            "name": name, "path": format!("/p/{name}"), "relativePath": name,
            "kind": if is_directory { "directory" } else { "code" }, "isDirectory": is_directory,
            "extension": null, "mimeType": null, "sizeBytes": null, "modifiedAt": null,
            "hidden": false, "hasChildren": is_directory,
        }))
        .expect("entry fixture")
    }

    #[test]
    fn mentions_start_a_word_and_split_at_the_last_slash() {
        let found = mention_at(0, "see @src/app now", 11).expect("mention");
        assert_eq!(
            (found.start, found.end, found.query.as_str()),
            (4, 12, "src/ap")
        );
        assert_eq!(
            mention_at(0, "mail a@b", 8),
            None,
            "@ inside a word is no mention"
        );
        assert_eq!(mention_at(0, "é @ü", 4).map(|m| m.query), Some("ü".into()));
        assert_eq!(mention_at(0, "@x", 0), None, "cursor before the @");
        assert_eq!(mention_at(0, "see @x", 4), None, "cursor before a later @");
        assert_eq!(mention_at(0, "@", 1).map(|m| m.query), Some(String::new()));
        assert_eq!(split_query(r"src\app\ma"), ("src/app".into(), "ma"));
        assert_eq!(split_query("ma"), (String::new(), "ma"));
        assert_eq!(mention_text(r"src\main.rs", false), "@src/main.rs ");
        assert_eq!(mention_text("src", true), "@src/");
    }

    #[test]
    fn suggestions_rank_prefix_matches_then_folders() {
        let entries = [
            entry("main.rs", false),
            entry("domain", true),
            entry("map.rs", false),
            entry("macros", true),
        ];
        let names: Vec<_> = suggestions(&entries, "ma")
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["macros", "main.rs", "map.rs", "domain"]);
    }
}
