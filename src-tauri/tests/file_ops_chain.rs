//! 文件操作链路：文件树右键与拖拽的安全护栏。
//!
//! 坏了的表现：重命名把同名文件悄悄覆盖、把文件夹拖进自己的子目录
//! 让树成环、或一个节点删不掉就让整批删除全失败。三种都是
//! 「用户的文件没了或不见了」，且没有撤销键。

use std::fs;

use notetext_lib::domain::file_ops::{DeleteReq, MkdirReq, MoveCopyReq, RenameReq};

mod common;
use common::boot;

/// 建一棵树：root/{a.txt, sub/}。
fn tree(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let root = dir.join("ops");
    let sub = root.join("sub");
    fs::create_dir_all(&sub).expect("建目录失败");
    let a = root.join("a.txt");
    fs::write(&a, "content\n").expect("写文件失败");
    (root, sub, a)
}

#[test]
fn test_file_ops_chain() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let state = boot(&dir);
    let (root, sub, a) = tree(dir.path());
    let svc = &state.services.file_ops;

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
    assert_eq!(
        fs::read_to_string(&b).expect("读回 b").trim(),
        "content",
        "内容该跟着走"
    );

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
    assert!(b.exists(), "拒绝后源文件该还在");

    // 移进自己的子目录：拒绝（树会成环）。
    let err = svc
        .rename(&RenameReq {
            from: root.to_string_lossy().to_string(),
            to: root.join("sub").join("root").to_string_lossy().to_string(),
        })
        .expect_err("目录移进自己子目录必须拒绝");
    assert_eq!(err.code, "1003", "成环必须拒绝");

    // 移动到另一个目录：内容与内容一起过去。
    let moved = sub.join("b.txt");
    let res = svc
        .move_to(&MoveCopyReq {
            paths: vec![b.to_string_lossy().to_string()],
            dest_dir: sub.to_string_lossy().to_string(),
            auto_suffix: Some(false),
        })
        .expect("移动失败");
    assert_eq!(res.succeeded, 1, "移动该成功一次");
    assert!(moved.exists(), "目标目录里该有它");
    assert!(!b.exists(), "原位置该空");

    // 复制不搬走原文件；再复制一次且 auto_suffix 才允许同名。
    let res = svc
        .copy_to(&MoveCopyReq {
            paths: vec![moved.to_string_lossy().to_string()],
            dest_dir: root.to_string_lossy().to_string(),
            auto_suffix: Some(false),
        })
        .expect("复制失败");
    assert_eq!(res.succeeded, 1, "复制该成功一次");
    assert!(moved.exists(), "复制不该搬走原文件");
    assert!(b.exists(), "复制出来的文件该在根目录");
    let res = svc
        .copy_to(&MoveCopyReq {
            paths: vec![moved.to_string_lossy().to_string()],
            dest_dir: root.to_string_lossy().to_string(),
            auto_suffix: Some(true),
        })
        .expect("带序号复制失败");
    assert_eq!(res.succeeded, 1, "auto_suffix 该允许同名");
    let suffixed = res.results[0].to.clone();
    assert!(
        root.join("b (2).txt").exists() || suffixed.ends_with("b (2).txt"),
        "auto_suffix 该落到带序号的新名字"
    );

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

    // 新建目录与文件：已存在即拒。
    let nd = root.join("fresh");
    let res = svc
        .mkdir(&MkdirReq {
            path: nd.to_string_lossy().to_string(),
        })
        .expect("建目录失败");
    assert_eq!(res.succeeded, 1, "建目录该成功");
    assert!(
        svc.mkdir(&MkdirReq {
            path: nd.to_string_lossy().to_string()
        })
        .is_err(),
        "重复建同一目录必须拒绝"
    );
    let nf = root.join("fresh.txt");
    let res = svc
        .create_file(&MkdirReq {
            path: nf.to_string_lossy().to_string(),
        })
        .expect("建文件失败");
    assert_eq!(res.succeeded, 1, "建文件该成功");
    assert!(
        svc.create_file(&MkdirReq {
            path: nf.to_string_lossy().to_string()
        })
        .is_err(),
        "重复建同一文件必须拒绝（不覆盖）"
    );
    assert_eq!(
        fs::read_to_string(&nf).expect("读回 nf"),
        "",
        "新建文件该是空的"
    );
}

#[test]
fn test_file_ops_rejects_relative_and_dotdot() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let state = boot(&dir);
    let svc = &state.services.file_ops;

    // 相对路径：判据（存在性、同级）都依赖一个隐式工作目录，必须拒。
    let err = svc
        .rename(&RenameReq {
            from: "a.txt".into(),
            to: "b.txt".into(),
        })
        .expect_err("相对路径必须拒绝");
    assert_eq!(err.code, "1000", "相对路径走空路径档的拒绝");

    // `..`：穿越判据在 canonicalize 之前就该拦住。
    let err = svc
        .rename(&RenameReq {
            from: dir.path().join("x").to_string_lossy().to_string(),
            to: dir
                .path()
                .join("..")
                .join("y")
                .to_string_lossy()
                .to_string(),
        })
        .expect_err("含 .. 的路径必须拒绝");
    assert_eq!(err.code, "1003", "含 .. 走越界档拒绝");
}
