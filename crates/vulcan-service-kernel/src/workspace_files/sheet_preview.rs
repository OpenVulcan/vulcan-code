//! Read-only spreadsheet previews for the WebUI sheet viewer.
//! 供 WebUI 表格查看器使用的只读电子表格预览。

use calamine::{Data, DataType, Reader, open_workbook_auto};

use super::*;

/// Most rows one sheet preview returns.
/// 单次工作表预览最多返回的行数。
const WORKSPACE_SHEET_PREVIEW_MAX_ROWS: usize = 2_000;

/// Most columns one sheet preview returns.
/// 单次工作表预览最多返回的列数。
const WORKSPACE_SHEET_PREVIEW_MAX_COLUMNS: usize = 200;

/// Most characters one returned cell keeps.
/// 单个返回单元格最多保留的字符数。
const WORKSPACE_SHEET_PREVIEW_MAX_CELL_CHARS: usize = 2_000;

/// Largest spreadsheet file a preview opens, since readers load whole sheets into memory.
/// 预览可打开的最大电子表格文件，因为读取器会把整张工作表载入内存。
const WORKSPACE_SHEET_PREVIEW_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Extensions the sheet reader opens.
/// 表格读取器可打开的扩展名。
const WORKSPACE_SHEET_EXTENSIONS: &[&str] = &["xlsx", "xlsm", "xlsb", "xls", "ods"];

/// Reads the leading cells of one sheet of a spreadsheet at an absolute path.
/// 读取绝对路径处电子表格中某个工作表的前部单元格。
///
/// # Parameters
/// 参数
/// `file_path` is the absolute spreadsheet path; it may lie outside any project, like the local media route.
/// `file_path` 是电子表格绝对路径；与本地媒体路由一样，它可以位于任何项目之外。
/// `sheet_name` picks a sheet; `None` reads the first one.
/// `sheet_name` 选择工作表；`None` 读取第一个。
///
/// # Returns
/// 返回值
/// The sheet names and the cells within the preview limits, or an English error.
/// 返回工作表名称及预览上限内的单元格，或英文错误。
pub(crate) fn workspace_sheet_preview(
    file_path: &str,
    sheet_name: Option<&str>,
) -> Result<WorkspaceSheetPreview, String> {
    let path = Path::new(file_path);
    if !path.is_absolute() {
        return Err(format!("The spreadsheet path is not absolute: {file_path}"));
    }
    let extension = workspace_file_extension(path).unwrap_or_default();
    if !WORKSPACE_SHEET_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!(
            "The file is not a supported spreadsheet: {file_path}"
        ));
    }
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Failed to read spreadsheet metadata `{file_path}`: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("The spreadsheet path is not a file: {file_path}"));
    }
    if metadata.len() > WORKSPACE_SHEET_PREVIEW_MAX_FILE_BYTES {
        return Err(format!(
            "The spreadsheet is larger than the {} MiB preview limit: {file_path}",
            WORKSPACE_SHEET_PREVIEW_MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    let mut workbook = open_workbook_auto(path)
        .map_err(|error| format!("Failed to open spreadsheet `{file_path}`: {error}"))?;
    let sheet_names = workbook.sheet_names();
    let selected = match sheet_name {
        Some(name) if sheet_names.iter().any(|candidate| candidate == name) => name.to_owned(),
        Some(name) => return Err(format!("The spreadsheet has no sheet named `{name}`.")),
        None => sheet_names
            .first()
            .cloned()
            .ok_or_else(|| format!("The spreadsheet has no sheets: {file_path}"))?,
    };
    let range = workbook
        .worksheet_range(&selected)
        .map_err(|error| format!("Failed to read sheet `{selected}`: {error}"))?;
    let (first_row, first_column) = range.start().unwrap_or((0, 0));
    let (total_rows, total_columns) = range.get_size();
    let rows = range
        .rows()
        .take(WORKSPACE_SHEET_PREVIEW_MAX_ROWS)
        .map(|row| {
            row.iter()
                .take(WORKSPACE_SHEET_PREVIEW_MAX_COLUMNS)
                .map(workspace_sheet_cell_text)
                .collect()
        })
        .collect();
    Ok(WorkspaceSheetPreview {
        path: normalized_workspace_path_text_from_path(path),
        sheet_names,
        sheet_name: selected,
        first_row,
        first_column,
        rows,
        total_rows,
        total_columns,
        truncated: total_rows > WORKSPACE_SHEET_PREVIEW_MAX_ROWS
            || total_columns > WORKSPACE_SHEET_PREVIEW_MAX_COLUMNS,
    })
}

/// Formats one cell the way a spreadsheet shows a general-format value, keeping at most the cell character limit.
/// 按电子表格显示常规格式值的方式格式化单个单元格，最多保留单元格字符上限。
///
/// # Parameters
/// 参数
/// `cell` is the cell value.
/// `cell` 是单元格值。
///
/// # Returns
/// 返回值
/// The display text; empty for empty cells.
/// 返回显示文本；空单元格为空字符串。
fn workspace_sheet_cell_text(cell: &Data) -> String {
    let text = match cell {
        Data::Empty => String::new(),
        Data::Bool(value) => String::from(if *value { "TRUE" } else { "FALSE" }),
        Data::Float(value) if value.fract() == 0.0 && value.abs() < 1e15 => {
            format!("{}", *value as i64)
        }
        Data::DateTime(value) if value.is_duration() => cell
            .as_duration()
            .map(|duration| {
                let seconds = duration.num_seconds();
                format!(
                    "{}:{:02}:{:02}",
                    seconds / 3600,
                    (seconds % 3600).abs() / 60,
                    seconds.abs() % 60
                )
            })
            .unwrap_or_else(|| cell.to_string()),
        Data::DateTime(_) => cell
            .as_datetime()
            .map(|date_time| {
                let text = date_time.format("%Y-%m-%d %H:%M:%S").to_string();
                text.strip_suffix(" 00:00:00")
                    .map(String::from)
                    .unwrap_or(text)
            })
            .unwrap_or_else(|| cell.to_string()),
        _ => cell.to_string(),
    };
    match text
        .char_indices()
        .nth(WORKSPACE_SHEET_PREVIEW_MAX_CELL_CHARS)
    {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// Writes a two-sheet xlsx whose first sheet "Data" holds a header row and `data_rows` numbered rows.
    /// 写入一个含两个工作表的 xlsx，其首个工作表 "Data" 含表头行及 `data_rows` 个编号行。
    fn write_test_workbook(path: &Path, data_rows: usize) {
        let mut rows = String::from(
            r#"<row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Done</t></is></c><c r="C1" t="inlineStr"><is><t>Score</t></is></c></row>"#,
        );
        for index in 0..data_rows {
            let row = index + 2;
            rows.push_str(&format!(
                r#"<row r="{row}"><c r="A{row}" t="inlineStr"><is><t>item {index}</t></is></c><c r="B{row}" t="b"><v>1</v></c><c r="C{row}"><v>{index}.5</v></c></row>"#
            ));
        }
        let sheet = |data: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{data}</sheetData></worksheet>"#
            )
        };
        let parts = [
            (
                "[Content_Types].xml",
                String::from(
                    r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
                ),
            ),
            (
                "_rels/.rels",
                String::from(
                    r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
                ),
            ),
            (
                "xl/workbook.xml",
                String::from(
                    r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>"#,
                ),
            ),
            (
                "xl/_rels/workbook.xml.rels",
                String::from(
                    r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#,
                ),
            ),
            ("xl/worksheets/sheet1.xml", sheet(&rows)),
            (
                "xl/worksheets/sheet2.xml",
                sheet(r#"<row r="3"><c r="C3"><v>7</v></c></row>"#),
            ),
        ];
        let mut archive = zip::ZipWriter::new(fs::File::create(path).expect("create workbook"));
        for (name, content) in parts {
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .expect("start workbook part");
            archive
                .write_all(content.as_bytes())
                .expect("write workbook part");
        }
        archive.finish().expect("finish workbook");
    }

    /// Returns a fresh temporary directory for one test.
    /// 为单个测试返回新的临时目录。
    fn test_directory() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("vulcan-sheet-preview-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("create sheet preview temp dir");
        directory
    }

    /// Verifies the preview lists sheets, reads the first one by default and formats cells as displayed.
    /// 验证预览列出工作表、默认读取第一个，并按显示形式格式化单元格。
    #[test]
    fn sheet_preview_reads_sheets_and_formats_cells() {
        let directory = test_directory();
        let path = directory.join("book.xlsx");
        write_test_workbook(&path, 2);

        let preview = workspace_sheet_preview(path.to_str().expect("utf-8 path"), None)
            .expect("preview the first sheet");
        assert_eq!(preview.sheet_names, ["Data", "Other"]);
        assert_eq!(preview.sheet_name, "Data");
        assert_eq!((preview.total_rows, preview.total_columns), (3, 3));
        assert_eq!(preview.rows[1], ["item 0", "TRUE", "0.5"]);
        assert!(!preview.truncated);

        let other = workspace_sheet_preview(path.to_str().expect("utf-8 path"), Some("Other"))
            .expect("preview a named sheet");
        assert_eq!((other.first_row, other.first_column), (2, 2));
        assert_eq!(other.rows, [["7"]]);
        let _ = fs::remove_dir_all(directory);
    }

    /// Verifies rows beyond the preview limit are left out and reported.
    /// 验证超出预览上限的行会被省略并报告。
    #[test]
    fn sheet_preview_caps_rows() {
        let directory = test_directory();
        let path = directory.join("long.xlsx");
        write_test_workbook(&path, WORKSPACE_SHEET_PREVIEW_MAX_ROWS);

        let preview = workspace_sheet_preview(path.to_str().expect("utf-8 path"), None)
            .expect("preview the long sheet");
        assert_eq!(preview.total_rows, WORKSPACE_SHEET_PREVIEW_MAX_ROWS + 1);
        assert_eq!(preview.rows.len(), WORKSPACE_SHEET_PREVIEW_MAX_ROWS);
        assert!(preview.truncated);
        let _ = fs::remove_dir_all(directory);
    }

    /// Verifies relative paths, other file types and unknown sheet names are rejected.
    /// 验证相对路径、其他文件类型与未知工作表名称会被拒绝。
    #[test]
    fn sheet_preview_rejects_invalid_requests() {
        let directory = test_directory();
        let path = directory.join("book.xlsx");
        write_test_workbook(&path, 1);
        let text = directory.join("notes.txt");
        fs::write(&text, "plain").expect("write text file");

        assert!(workspace_sheet_preview("book.xlsx", None).is_err());
        assert!(workspace_sheet_preview(text.to_str().expect("utf-8 path"), None).is_err());
        assert!(
            workspace_sheet_preview(path.to_str().expect("utf-8 path"), Some("Missing")).is_err()
        );
        let _ = fs::remove_dir_all(directory);
    }
}
