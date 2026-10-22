//! 文件域链路：读写的编码 / 行尾 / BOM / 原子写 + 文件树操作的安全护栏。
//!
//! 坏了的表现：GBK 文件打开一片乱码；CRLF 文件按一下保存行尾全变 LF；
//! 重命名悄悄覆盖同名文件、文件夹拖进自己的子目录让树成环、
//! 一个节点删不掉就让整批删除全失败——都是「用户的文件没了」，且没有撤销键。

use std::fs;

use notetext_lib::domain::file::ReadMode;
use notetext_lib::domain::file_ops::{DeleteReq, MkdirReq, RenameReq};
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

#[test]
fn test_file_ops_chain() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let state = boot(&dir);
    let svc = &state.services.file_ops;

    // 建一棵树：root/{a.txt, sub/}。
    let root = dir.path().join("ops");
    let sub = root.join("sub");
    fs::create_dir_all(&sub).expect("建目录失败");
    let a = root.join("a.txt");
    fs::write(&a, "content\n").expect("写文件失败");

    // 重命名落到新路径，原路径消失。
    let b = root.join("b.txt");
    let res = svc
        .rename(&RenameReq {
            from: a.to_string_lossy().to_string(),
            to: b.to_string_lossy().to_string(),
        })
        .expect("重命名失败");
    assert_eq!(res.succeeded, 1, "重命名该成功一次");
    assert!(!a.exists(), "原路径该消失");

    // 目标已存在：整体拒绝，且已有文件不被覆盖。
    let c = root.join("c.txt");
    fs::write(&c, "keep me\n").expect("写 c 失败");
    let err = svc
        .rename(&RenameReq {
            from: b.to_string_lossy().to_string(),
            to: c.to_string_lossy().to_string(),
        })
        .expect_err("目标已存在必须拒绝");
    assert_eq!(err.code, "1003", "拒绝码该是路径越界档");
    assert_eq!(
        fs::read_to_string(&c).expect("读回 c").trim(),
        "keep me",
        "拒绝后已有文件不能被覆盖"
    );

    // 移进自己的子目录：拒绝（树会成环）。
    let err = svc
        .rename(&RenameReq {
            from: root.to_string_lossy().to_string(),
            to: root.join("sub").join("root").to_string_lossy().to_string(),
        })
        .expect_err("目录移进自己子目录必须拒绝");
    assert_eq!(err.code, "1003", "成环必须拒绝");

    // 删除逐个记账：一个占用/不存在的项不影响其他项。
    let d = root.join("d.txt");
    fs::write(&d, "bye\n").expect("写 d 失败");
    let res = svc
        .remove(&DeleteReq {
            paths: vec![
                d.to_string_lossy().to_string(),
                root.join("nope.txt").to_string_lossy().to_string(),
            ],
        })
        .expect("删除失败");
    assert_eq!(res.succeeded, 1, "存在的那个该删掉");
    assert_eq!(res.failed, 1, "不存在的那个该记账失败");
    assert!(!d.exists(), "存在的那个该真的消失");

    // 新建文件与目录：已存在即拒（不覆盖）。
    let nf = root.join("fresh.txt");
    svc.create_file(&MkdirReq {
        path: nf.to_string_lossy().to_string(),
    })
    .expect("建文件失败");
    assert!(
        svc.create_file(&MkdirReq {
            path: nf.to_string_lossy().to_string()
        })
        .is_err(),
        "重复建同一文件必须拒绝"
    );

    // 相对路径与 `..`：判据都依赖隐式工作目录 / 会逃出操作范围，必须拒。
    assert!(
        svc.rename(&RenameReq {
            from: "a.txt".into(),
            to: "b.txt".into()
        })
        .is_err(),
        "相对路径必须拒绝"
    );
    assert!(
        svc.rename(&RenameReq {
            from: dir.path().join("x").to_string_lossy().to_string(),
            to: dir
                .path()
                .join("..")
                .join("y")
                .to_string_lossy()
                .to_string(),
        })
        .is_err(),
        "含 .. 的路径必须拒绝"
    );
}
