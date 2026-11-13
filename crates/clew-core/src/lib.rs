pub mod scan;
pub mod search;
pub mod snapshot;
pub mod store;
pub mod walk;

/// 给界面用的名字：模块级定义在索引里叫 `<module>`（`if __name__ == "__main__"`
/// 那一块），界面上统一显示成 `__main__`。只换显示，库里存的还是 `<module>`，
/// 匹配、查找都靠它。
pub fn display_qual(qualname: &str) -> &str {
    if qualname == "<module>" {
        "__main__"
    } else {
        qualname
    }
}
