//! 表格服务：大文件 CSV / TSV / JSONL 的分页读取、查找、视图与写回。
//! 四块各管一段：索引（记录对齐的稀疏块表）、解析（记录边界扫描）、
//! 视图（排序 / 筛选）、重写（覆盖层流式落盘）。

pub mod index;
pub mod records;
pub mod rewrite;
pub mod view;

pub use index::{TableIndex, TableService};
