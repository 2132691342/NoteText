//! 文件域长链路：读改写往返 + 文件树操作的安全护栏。
//!
//! 这里守护「用户的文件没了」且没有撤销键的事故：CRLF 文件一保存行尾全变 LF、
//! 正文前多三个不可见字节、选了编码打开仍是乱码、重命名悄悄覆盖同名文件、
//! 目录拖进自己子目录让文件树成环。

use std::fs;
use std::path::PathBuf;

use notetext_lib::app::bootstrap::{self, AppState};
use notetext_lib::domain::file::{FileContentVO, ReadMode};
use notetext_lib::domain::file_ops::{DeleteReq, MkdirReq, RenameReq};
use notetext_lib::service::encoding;

mod common;
use common::temp_paths;

/// 一套隔离的文件服务：每个测试自己一份临时目录。
fn setup() -> (tempfile::TempDir, AppState, PathBuf) {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = temp_paths(&dir);
    let state = bootstrap::build(&paths).expect("装配失败");
    (dir, state, paths.data_dir)
}

/// 写一个文件并读回来：拿到服务返回的读结果与磁盘路径。
fn roundtrip(state: &AppState, file: &PathBuf, bytes: &[u8]) -> FileContentVO {
    fs::write(file, bytes).expect("写测试文件失败");
    state
        .services
        .file
        .read(&file.to_string_lossy(), ReadMode::Full)
        .expect("读失败")
}

#[test]
fn read_write_roundtrip_preserves_line_ending_and_bom() {
    let (dir, state, data) = setup();

    // ---- CRLF：读时归一成 LF 供编辑器算行列，写回时还原 ----
    let crlf = data.join("crlf.txt");
    let p = crlf.to_string_lossy().to_string();
    let read = roundtrip(&state, &crlf, b"a\r\nb\r\n");
    assert_eq!(read.line_ending, "crlf", "应探测到 CRLF");
    assert_eq!(read.text, "a\nb\n", "读出来应归一成 LF");

    // 不指定行尾：按文件原本的 CRLF 写回。没有这一步，一次保存就把整个
    // 文件的行尾全变成 LF，diff 里满屏都是改动。
    state
        .services
        .file
        .write(&p, "a\nb\n", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read(&crlf).expect("回读失败"),
        b"a\r\nb\r\n",
        "不指定行尾应原样写回 CRLF"
    );

    // 显式切换才改。
    state
        .services
        .file
        .write(&p, "a\nb\n", None, Some("lf"), None)
        .expect("写失败");
    assert_eq!(
        fs::read(&crlf).expect("回读失败"),
        b"a\nb\n",
        "显式 lf 应写成 LF"
    );

    // ---- BOM：剥不干净会让光标列号全偏，补不回去会让下游编码判定翻车 ----
    let bom = data.join("bom.txt");
    let (has, body) = encoding::strip_bom(&[0xEF, 0xBB, 0xBF, 0x61, 0x62], "utf-8-bom");
    assert!(has, "应检出 BOM");
    assert_eq!(body, b"ab", "剥掉 BOM 后只剩正文");
    assert_eq!(
        encoding::bom_bytes("utf-8-bom"),
        &[0xEF, 0xBB, 0xBF],
        "BOM 字节要能补回"
    );

    // 带 BOM 的文件走完整读写链：读进来没有 BOM，写回去 BOM 还在。
    let read_bom = roundtrip(&state, &bom, b"\xEF\xBB\xBFhello");
    assert_eq!(read_bom.text, "hello", "读进来应剥掉 BOM");
    let bp = bom.to_string_lossy().to_string();
    state
        .services
        .file
        .write(&bp, "hello", Some("utf-8-bom"), None, Some(true))
        .expect("写失败");
    assert_eq!(
        fs::read(&bom).expect("回读失败"),
        b"\xEF\xBB\xBFhello",
        "写回去应补回 BOM"
    );
    drop(dir);
}

#[test]
fn forced_encoding_read_decodes_and_falls_back_when_unsupported() {
    let (dir, state, data) = setup();
    let gbk = data.join("gbk.txt");
    // 用码表里有的全角符号（0xA1A1 起）而不是汉字：内置表是常用子集，汉字不在内。
    let p = gbk.to_string_lossy().to_string();
    roundtrip(&state, &gbk, &[0xA1u8, 0xA1, 0xA1, 0xA2]);

    // 用户按「以指定编码重新打开」选了编码，打开的必须不再是乱码。
    let forced = state
        .services
        .file
        .read_as(&p, ReadMode::Full, Some("gb18030"))
        .expect("按 GBK 读失败");
    assert_eq!(forced.text, "、。", "指定 gb18030 应解出表内符号");
    assert_eq!(forced.encoding, "gb18030", "返回的编码应是强制值");

    // 选了个不认识的编码：退回探测编码，而不是整个文件打不开。
    let fallback = state
        .services
        .file
        .read_as(&p, ReadMode::Full, Some("klingon"))
        .expect("选错编码也不该打不开");
    assert_eq!(fallback.text, "、。", "不支持应退回探测编码");
    drop(dir);
}

#[test]
fn atomic_write_leaves_no_temp_file_and_never_creates_missing_dirs() {
    let (dir, state, data) = setup();
    let file = data.join("x.txt");
    let p = file.to_string_lossy().to_string();

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

    // 临时文件残留会在用户目录里越积越多。
    let leftovers: Vec<String> = fs::read_dir(&data)
        .expect("列目录失败")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("notetext-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "不应留下临时文件：{leftovers:?}");

    // 目标目录不存在：报错，不替用户造目录（打错路径不该在磁盘上留痕）。
    let missing = data.join("no-such-dir").join("x.txt");
    assert!(
        state
            .services
            .file
            .write(&missing.to_string_lossy(), "x", None, None, None)
            .is_err(),
        "目标目录不存在应报错"
    );
    assert!(
        !missing.parent().expect("有父目录").exists(),
        "不应顺带创建目录"
    );
    drop(dir);
}

#[test]
fn rename_rejects_overwrite_cycles_and_escapes() {
    let (dir, state, _data) = setup();
    let svc = &state.services.file_ops;
    let root = dir.path().join("ops");
    let sub = root.join("sub");
    fs::create_dir_all(&sub).expect("建目录失败");
    let a = root.join("a.txt");
    fs::write(&a, "content\n").expect("写文件失败");

    // ---- 覆盖：整体拒绝，覆盖等于无声销毁别人的文件 ----
    let b = root.join("b.txt");
    svc.rename(&RenameReq {
        from: a.to_string_lossy().to_string(),
        to: b.to_string_lossy().to_string(),
    })
    .expect("重命名失败");
    assert!(!a.exists(), "原路径该消失");

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

    // ---- 成环：移进自己的子目录会让文件树递归遍历挂死 ----
    let err = svc
        .rename(&RenameReq {
            from: root.to_string_lossy().to_string(),
            to: sub.join("root").to_string_lossy().to_string(),
        })
        .expect_err("目录移进自己子目录必须拒绝");
    assert_eq!(err.code, "1003", "成环必须拒绝");

    // ---- 逃逸：相对路径依赖隐式工作目录，`..` 会逃出操作范围 ----
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

#[test]
fn delete_accounts_for_each_path_and_create_refuses_existing() {
    let (dir, state, _data) = setup();
    let svc = &state.services.file_ops;
    let root = dir.path().join("ops");
    fs::create_dir_all(&root).expect("建目录失败");

    // 一个不存在的项不该让整批删除失败：用户勾了一堆，其中一个是陈旧路径。
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

    // 新建同名文件：拒绝而不是清空已有内容。
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
}
