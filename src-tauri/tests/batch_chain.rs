//! 批量操作的安全护栏：扫描不写盘、应用只改勾选的、冲突不覆盖。
//!
//! 失败表现：批量替换静默改掉用户没勾选的文件；或重命名把已有文件覆盖——
//! 两种都是「用户的文件没了」，且没有撤销键。

use std::fs;
use std::path::PathBuf;

use notetext_lib::app::bootstrap::{self, AppState};
use notetext_lib::domain::batch::{
    BatchFindReq, BatchRenameReq, BatchReplaceReq, FindMode, RenameApplyReq, RenameMode, RenamePair,
};

mod common;
use common::temp_paths;

fn setup() -> (tempfile::TempDir, AppState, PathBuf) {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = temp_paths(&dir);
    let state = bootstrap::build(&paths).expect("装配失败");
    let root = dir.path().join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    (dir, state, root)
}

#[test]
fn scan_reports_without_touching_files() {
    let (_dir, state, root) = setup();
    fs::write(root.join("a.txt"), "hello world\nhello again\n").expect("写 a 失败");

    let scan = state
        .services
        .batch
        .scan_find(&BatchFindReq {
            dir: root.to_string_lossy().to_string(),
            glob: "*.txt".into(),
            find: "hello".into(),
            mode: Some(FindMode::Plain),
            case_sensitive: Some(false),
            recursive: Some(true),
        })
        .expect("扫描失败");

    assert_eq!(scan.hits.len(), 1, "只有 a.txt 含 hello");
    assert_eq!(scan.total_count, 2, "a.txt 里 hello 出现两次");
    // 扫描是「先看再决定」，此时落盘就等于绕过用户的确认。
    assert!(
        fs::read_to_string(root.join("a.txt"))
            .expect("读回 a")
            .contains("hello"),
        "扫描阶段不能改动文件"
    );
}

#[test]
fn apply_replaces_only_the_checked_files() {
    let (_dir, state, root) = setup();
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "hello world\nhello again\n").expect("写 a 失败");
    fs::write(&b, "nothing here\n").expect("写 b 失败");
    let before_b = fs::read_to_string(&b).expect("读 b");

    let res = state
        .services
        .batch
        .apply_replace(&BatchReplaceReq {
            dir: root.to_string_lossy().to_string(),
            glob: "*.txt".into(),
            find: "hello".into(),
            replace: "hi".into(),
            mode: Some(FindMode::Plain),
            case_sensitive: Some(false),
            recursive: Some(true),
            files: vec![a.to_string_lossy().to_string()],
        })
        .expect("应用失败");

    assert_eq!(res.changed, 1, "只应改一个文件");
    assert_eq!(res.replacements, 2, "两处 hello 都替换");
    assert_eq!(
        fs::read_to_string(&b).expect("读回 b"),
        before_b,
        "没勾选的文件不能被改"
    );
}

#[test]
fn rename_conflict_is_flagged_and_cannot_be_applied() {
    let (_dir, state, root) = setup();
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "hello world\n").expect("写 a 失败");
    fs::write(&b, "nothing here\n").expect("写 b 失败");

    let plan = state
        .services
        .batch
        .plan_rename(&BatchRenameReq {
            files: vec![a.to_string_lossy().to_string()],
            mode: Some(RenameMode::Replace),
            find: "a".into(),
            replace: "b".into(),
            start: Some(1),
            prefix: Some(true),
        })
        .expect("预览失败");
    assert_eq!(plan.items.len(), 1, "一条重命名计划");
    assert!(plan.items[0].conflict, "目标 b.txt 已存在，必须标冲突");
    assert_eq!(plan.applicable, 0, "冲突项不可应用");

    // 强行应用要整体拒绝，绝不能覆盖掉别人的文件。
    let err = state
        .services
        .batch
        .apply_rename(&RenameApplyReq {
            items: vec![RenamePair {
                from: a.to_string_lossy().to_string(),
                to: b.to_string_lossy().to_string(),
            }],
        })
        .expect_err("覆盖已存在文件必须报错");
    assert!(!err.message.is_empty(), "错误要带一句人话");
    assert!(
        fs::read_to_string(&b).expect("读回 b").contains("nothing"),
        "被占用的目标文件不能被覆盖"
    );
}

#[test]
fn rename_without_conflict_actually_lands() {
    let (_dir, state, root) = setup();
    let c = root.join("c.txt");
    fs::write(&c, "x").expect("写 c 失败");

    // 没冲突的计划要能真落地：只标冲突不落地，用户点了「应用」什么都没发生。
    let ok = state
        .services
        .batch
        .apply_rename(&RenameApplyReq {
            items: vec![RenamePair {
                from: c.to_string_lossy().to_string(),
                to: root.join("d.txt").to_string_lossy().to_string(),
            }],
        })
        .expect("重命名失败");
    assert_eq!(ok.changed, 1, "应改成一个文件");
    assert!(root.join("d.txt").exists(), "新名字要存在");
    assert!(!c.exists(), "旧名字要消失");
}
