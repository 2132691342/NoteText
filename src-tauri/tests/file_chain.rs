//! 文件往返链路：打开 → 编辑 → 保存，编码 / 行尾 / BOM / 原子写全程不掉原信息。
//!
//! 坏了的表现：GBK 文件打开一片乱码；或打开一个 CRLF 文件按一下保存，
//! 整个文件行尾变成 LF，同事那边 diff 出几千行——这是编辑器最招人骂的一类 bug。

use std::fs;

use notetext_lib::domain::file::ReadMode;
use notetext_lib::service::encoding;

mod common;
use common::{boot, temp_paths};

#[test]
fn test_file_roundtrip_chain() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = temp_paths(&dir);
    let state = boot(&dir);
    let file = paths.data_dir.join("crlf.txt");
    fs::write(&file, b"a\r\nb\r\n").expect("写测试文件失败");
    let p = file.to_string_lossy().to_string();

    // 读：CRLF 归一成 LF 供编辑器算行列，但记下原样式。
    let read = state
        .services
        .file
        .read(&p, ReadMode::Full)
        .expect("读失败");
    assert_eq!(read.line_ending, "crlf", "应探测到 CRLF");
    assert_eq!(read.text, "a\nb\n", "读出来应归一成 LF");

    // 写：不指定行尾时按读到的 CRLF 原样写回。
    state
        .services
        .file
        .write(&p, "a\nb\n", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read(&file).expect("回读失败"),
        b"a\r\nb\r\n",
        "不指定行尾时应按原样写回 CRLF"
    );

    // 显式切换才改：用户主动转行尾是另一回事。
    state
        .services
        .file
        .write(&p, "a\nb\n", None, Some("lf"), None)
        .expect("写失败");
    assert_eq!(
        fs::read(&file).expect("回读失败"),
        b"a\nb\n",
        "显式 lf 应写成 LF"
    );

    // 原子写：内容更新后不留临时文件。
    state
        .services
        .file
        .write(&p, "new content", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read_to_string(&file).expect("回读失败"),
        "new content",
        "内容应更新"
    );
    let leftovers: Vec<String> = fs::read_dir(&paths.data_dir)
        .expect("列目录失败")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("notetext-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "不应留下临时文件：{leftovers:?}");

    // 写到不存在的目录：报错且不顺带造目录。
    let missing = paths.data_dir.join("no-such-dir").join("x.txt");
    let err = state
        .services
        .file
        .write(&missing.to_string_lossy(), "x", None, None, None);
    assert!(err.is_err(), "目标目录不存在应报错");
    assert!(
        !missing.parent().expect("有父目录").exists(),
        "不应顺带创建目录"
    );
}

#[test]
fn test_encoding_detection_chain() {
    // 探测：BOM 是硬证据，合法 UTF-8 判 UTF-8，GBK 字节判 gb18030。
    assert_eq!(
        encoding::detect(&[0xEF, 0xBB, 0xBF, 0x61]),
        ("utf-8-bom".to_string(), false),
        "带 BOM 应判 utf-8-bom"
    );
    assert_eq!(
        encoding::detect("中文".as_bytes()),
        ("utf-8".to_string(), false),
        "合法 UTF-8 应判 utf-8"
    );
    let (enc, low) = encoding::detect(&[0xD6, 0xD0, 0xCE, 0xC4]); // GBK "中文"
    assert_eq!(enc, "gb18030", "GBK 字节应判 gb18030");
    assert!(!low, "有把握时不该标低置信");
    assert_eq!(
        encoding::detect(&[]),
        ("utf-8".to_string(), false),
        "空文件按 UTF-8，不是 latin 兜底"
    );

    // BOM 剥离与补回：保存时不对上就给用户文件头多三个字节。
    let (has, body) = encoding::strip_bom(&[0xEF, 0xBB, 0xBF, 0x61, 0x62], "utf-8-bom");
    assert!(has, "应检出 BOM");
    assert_eq!(body, b"ab", "剥掉 BOM 后只剩正文");
    assert_eq!(
        encoding::bom_bytes("utf-8-bom"),
        &[0xEF, 0xBB, 0xBF],
        "BOM 字节要能补回"
    );
}
