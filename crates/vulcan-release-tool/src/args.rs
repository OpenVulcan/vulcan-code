//! `--name value` options; every command lists the names it accepts.
//! `--name value` 形式的选项；每个命令列出其接受的名称。

use std::path::PathBuf;

use anyhow::bail;

/// Parsed options in command line order.
/// 按命令行顺序解析出的选项。
pub struct Args {
    /// Option names without `--`, with their values.
    /// 不含 `--` 的选项名及其值。
    pairs: Vec<(String, String)>,
}

impl Args {
    /// Parses `--name value` pairs, rejecting names outside `allowed`.
    /// 解析 `--name value` 对，拒绝 `allowed` 之外的名称。
    pub fn parse(
        mut arguments: impl Iterator<Item = String>,
        allowed: &[&str],
    ) -> anyhow::Result<Self> {
        let mut pairs = Vec::new();
        while let Some(flag) = arguments.next() {
            let Some(name) = flag.strip_prefix("--") else {
                bail!("unexpected argument `{flag}`");
            };
            if !allowed.contains(&name) {
                bail!("unknown option `--{name}`");
            }
            let Some(value) = arguments.next() else {
                bail!("`--{name}` needs a value");
            };
            pairs.push((name.to_owned(), value));
        }
        Ok(Self { pairs })
    }

    /// Every value of a repeatable option.
    /// 可重复选项的全部值。
    pub fn all(&self, name: &str) -> Vec<&str> {
        self.pairs
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// The value of an option given at most once.
    /// 至多出现一次的选项的值。
    pub fn optional(&self, name: &str) -> anyhow::Result<Option<&str>> {
        match self.all(name).as_slice() {
            [] => Ok(None),
            [value] => Ok(Some(value)),
            _ => bail!("`--{name}` is given more than once"),
        }
    }

    /// The value of an option given exactly once.
    /// 恰好出现一次的选项的值。
    pub fn required(&self, name: &str) -> anyhow::Result<&str> {
        match self.optional(name)? {
            Some(value) => Ok(value),
            None => bail!("`--{name}` is required"),
        }
    }

    /// A required path option.
    /// 必需的路径选项。
    pub fn path(&self, name: &str) -> anyhow::Result<PathBuf> {
        self.required(name).map(PathBuf::from)
    }

    /// An optional path option.
    /// 可选的路径选项。
    pub fn optional_path(&self, name: &str) -> anyhow::Result<Option<PathBuf>> {
        Ok(self.optional(name)?.map(PathBuf::from))
    }
}
