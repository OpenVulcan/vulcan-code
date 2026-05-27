//! Convert one Hugging Face `tokenizer.json` file into tokie's `.tkz` binary format.
//! 将单个 Hugging Face `tokenizer.json` 文件转换为 tokie 的 `.tkz` 二进制格式。

use std::{env, path::PathBuf};

use tokie::Tokenizer;

/// Converts one tokenizer JSON file into one `.tkz` output file.
/// 将单个 tokenizer JSON 文件转换为一个 `.tkz` 输出文件。
///
/// # Returns
/// 返回值
/// `Ok(())` when the conversion succeeds.
/// 转换成功时返回 `Ok(())`。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let input = args
        .next()
        .map(PathBuf::from)
        .ok_or("missing input tokenizer.json path")?;
    let output = args
        .next()
        .map(PathBuf::from)
        .ok_or("missing output tokenizer.tkz path")?;

    if args.next().is_some() {
        return Err(
            "usage: convert_tokenizer_json_to_tkz <input-tokenizer.json> <output-tokenizer.tkz>"
                .into(),
        );
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tokenizer = Tokenizer::from_json(&input)?;
    tokenizer.to_file(&output)?;

    println!("converted {} -> {}", input.display(), output.display());
    Ok(())
}
