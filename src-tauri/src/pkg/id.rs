//! id 生成：UUID v4。
//!
//! 只给「需要导出的实体」用（宏、草稿）。标签 / 书签用数据库自增整数：
//! 它们不出本机，自增更省事也更好读。
//!
//! 不引 `uuid` crate：v4 只需要 16 字节随机数 + 版本位，标准库的
//! `rand` 也不引——用 `std::collections::hash_map::RandomState` 的
//! 每次进程不同的种子做熵源，够用于本机标识（不用于安全场景）。

use std::hash::{BuildHasher, Hasher};

/// 生成一个 UUID v4 形状的字符串（小写连字符形式）。
pub fn uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);

    // 版本 4 与变体位（RFC 4122 §4.4）。
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// 熵源：RandomState 的种子每个进程都不同（由系统随机数播种），
/// 混入进程 id 与一个自增计数器，连续调用也拿得到不同值。
fn fill_random(out: &mut [u8]) {
    for (counter, chunk) in out.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(counter as u64);
        h.write_u32(std::process::id());
        // 混入一个纳秒时间戳：同一毫秒内的连续调用也要有差异。
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        h.write_u64(n);
        let v = h.finish();
        for (i, b) in chunk.iter_mut().enumerate() {
            *b = (v >> (i * 8)) as u8;
        }
    }
}
