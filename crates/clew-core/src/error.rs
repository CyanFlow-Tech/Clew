//! 可能跨模块边界的那几种失败。
//!
//! 查询层直接返回 `rusqlite::Result`——SQL 失败就是 SQL 失败，中间层包装一遍
//! 只是把同一个东西搬运两次。会跨到别的模块、需要调用方分情况处理的失败
//! （打不开库、库版本比程序新、目标目录不空）才收在这里，让上层能 match 而不是
//! 只剩一串字符串可以打印。

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ClewError {
    /// 文件系统层面的失败。带上正在做什么和在哪条路径上，否则报错只剩
    /// 一句「拒绝访问」，看不出是哪一步。
    Io {
        doing: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    /// 索引库自己报的错，原样留着——SQL 细节在排查时才是唯一线索。
    Sql {
        /// 出事的库文件；查询中途的失败没有这一项。
        at: Option<PathBuf>,
        source: rusqlite::Error,
    },
    /// 前提不成立：目录不空、版本比程序新、缺迁移脚本。这不是 bug，是状态，
    /// 调用方可以据此换一条路走。
    Refused(String),
}

impl ClewError {
    /// `map_err(ClewError::io("打开索引库", &path))`。
    pub fn io<P: AsRef<Path>>(doing: &'static str, path: P) -> impl FnOnce(io::Error) -> Self {
        let path = path.as_ref().to_path_buf();
        move |source| ClewError::Io {
            doing,
            path,
            source,
        }
    }

    pub fn refused(message: impl Into<String>) -> Self {
        ClewError::Refused(message.into())
    }

    /// 是不是「前提不成立」这一类：可以给用户看的一句话，不需要开发介入。
    pub fn is_refusal(&self) -> bool {
        matches!(self, ClewError::Refused(_))
    }
}

impl fmt::Display for ClewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClewError::Io { doing, path, source } => {
                write!(f, "{doing}时失败（{}）：{source}", path.display())
            }
            ClewError::Sql { at, source } => match at {
                Some(path) => write!(f, "索引库 {} 查询失败：{source}", path.display()),
                None => write!(f, "索引库查询失败：{source}"),
            },
            ClewError::Refused(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ClewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClewError::Io { source, .. } => Some(source),
            ClewError::Sql { source, .. } => Some(source),
            ClewError::Refused(_) => None,
        }
    }
}

impl From<rusqlite::Error> for ClewError {
    fn from(source: rusqlite::Error) -> Self {
        ClewError::Sql { at: None, source }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_io_failure_names_the_step_and_the_path() {
        let err = ClewError::io("打开索引库", "D:/x/index.db")(
            io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
        );
        let text = err.to_string();
        assert!(text.contains("打开索引库"), "{text}");
        assert!(text.contains("index.db"), "{text}");
        assert!(text.contains("access denied"), "{text}");
    }

    #[test]
    fn only_refusals_count_as_user_level_answers() {
        assert!(ClewError::refused("目录不是空的").is_refusal());
        assert!(
            !ClewError::Sql { at: None, source: rusqlite::Error::InvalidQuery }.is_refusal()
        );
    }

    #[test]
    fn sql_errors_keep_their_text() {
        let err = ClewError::from(rusqlite::Error::InvalidQuery);
        assert!(err.to_string().contains("索引库查询失败"));
        assert!(std::error::Error::source(&err).is_some());
    }
}
