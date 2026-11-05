//! 归档长链路：15 种格式创建→解压逐字节一致、改名仍认 tar 家族、追加/删除条目、分卷合并还原、解压护栏。

mod common;

use common::{create_req, extract_req, pack_and_unpack, sample_tree, task_ctx};
use std::fs;
use std::path::PathBuf;

use notetext_lib::domain::archive::{AddEntriesReq, CreateArchiveReq, DeleteEntriesReq};
use notetext_lib::pkg::error::codes;
use notetext_lib::service::archive as arch;

#[test]
fn archive_lifecycle_covers_all_formats_update_and_split() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let (root, bin) = sample_tree(dir.path());
    let want_bin = fs::read(&bin).expect("回读样本");
    let want_txt = fs::read(root.join("说明.txt")).expect("回读样本");
    let root_s = root.to_string_lossy().to_string();
    let ctx = task_ctx();

    // 容器格式压整个目录；单文件格式只能压一个文件。
    for fmt in [
        "7z", "zip", "tar", "tgz", "txz", "tbz2", "tzst", "tlz4", "tbr",
    ] {
        let (_, out) = pack_and_unpack(dir.path(), &root, fmt, "dir");
        let got = out.join("样本目录");
        assert_eq!(
            fs::read(got.join("说明.txt")).unwrap_or_else(|_| panic!("{fmt} 解压后缺 说明.txt")),
            want_txt,
            "{fmt} 往返后文本内容变了"
        );
        assert_eq!(
            fs::read(got.join("子目录").join("数据.bin"))
                .unwrap_or_else(|_| panic!("{fmt} 解压后缺 子目录/数据.bin（层级或中文名塌了）")),
            want_bin,
            "{fmt} 往返后二进制内容变了"
        );
    }

    // 单文件格式产物名取自包名去掉压缩后缀，包名必须带上原文件名。
    for fmt in ["gz", "bz2", "zst", "xz", "lz4", "br"] {
        let (_, out) = pack_and_unpack(dir.path(), &bin, fmt, "数据.bin");
        assert_eq!(
            fs::read(out.join("数据.bin")).unwrap_or_else(|_| panic!("{fmt} 解压后缺产物")),
            want_bin,
            "{fmt} 单文件往返后内容变了"
        );
    }

    // .tar.xz / .tar.br 改名后仍要识别成 tar 家族：xz 有魔数、brotli 没有，两条路径都要走通。
    for (fmt, renamed) in [("txz", "renamed.tar.xz"), ("tbr", "renamed.tar.br")] {
        let (archive, _) = pack_and_unpack(dir.path(), &root, fmt, "rename");
        let dest = dir.path().join(renamed);
        fs::rename(&archive, &dest).unwrap_or_else(|e| panic!("{renamed} 改名失败：{e}"));
        let out = dir.path().join(format!("x-{renamed}"));
        arch::extract::extract_archive(&extract_req(&dest, &out), &ctx)
            .unwrap_or_else(|e| panic!("{renamed} 改名后识别失败：{e}"));
        assert!(
            out.join("样本目录").join("说明.txt").exists(),
            "{renamed} 改名后没被识别成 tar 家族（目录层级丢了）"
        );
    }

    // 追加 / 删除条目（重建式），原有条目不能丢。
    let pack = dir.path().join("pack.zip");
    arch::create::create_archive(&create_req(vec![root_s.clone()], &pack, "zip"), &ctx)
        .expect("创建失败");
    let extra = dir.path().join("新增.log");
    fs::write(&extra, b"new entry").expect("写新增文件");
    arch::update::add_entries(
        &AddEntriesReq {
            archive: pack.to_string_lossy().to_string(),
            sources: vec![extra.to_string_lossy().to_string()],
            password: None,
            exclude: Vec::new(),
        },
        &ctx,
    )
    .expect("追加失败");
    let info = arch::list::list_archive(&pack, None).expect("追加后列目录失败");
    assert!(
        info.entries.iter().any(|e| e.name == "新增.log"),
        "追加的条目要出现在归档里"
    );
    assert!(
        info.entries.iter().any(|e| e.name.ends_with("说明.txt")),
        "追加不能丢原有条目"
    );
    arch::update::delete_entries(
        &DeleteEntriesReq {
            archive: pack.to_string_lossy().to_string(),
            entries: vec!["样本目录/说明.txt".into()],
            password: None,
        },
        &ctx,
    )
    .expect("删除失败");
    let info = arch::list::list_archive(&pack, None).expect("删除后列目录失败");
    assert!(
        !info.entries.iter().any(|e| e.name == "样本目录/说明.txt"),
        "删除的条目必须消失"
    );
    assert!(
        info.entries.iter().any(|e| e.name == "新增.log"),
        "没删的条目必须还在"
    );

    // 分卷链：切卷 → 首卷直接打开解压 → 合并字节级还原。
    let vol = dir.path().join("vol.zip");
    arch::create::create_archive(&create_req(vec![root_s.clone()], &vol, "zip"), &ctx)
        .expect("创建分卷源失败");
    let original = fs::read(&vol).expect("回读原包");
    let parts = arch::split::split_file(&vol, 1024, &ctx).expect("分卷失败");
    assert!(parts.len() >= 2, "必须切出多卷");
    let first = PathBuf::from(&parts[0]);
    let info = arch::list::list_archive(&first, None).expect("首卷必须能直接打开");
    assert!(info.is_split, "分卷包要标记 is_split");
    let out = dir.path().join("from-parts");
    arch::extract::extract_archive(&extract_req(&first, &out), &ctx).expect("分卷解压失败");
    assert!(
        out.join("样本目录").join("说明.txt").exists(),
        "分卷包解压出的目录层级不对"
    );
    let joined = dir.path().join("joined.zip");
    arch::split::join_parts(&parts, &joined, &ctx).expect("合并失败");
    assert_eq!(
        fs::read(&joined).expect("回读合并"),
        original,
        "合并后必须字节级还原"
    );
}

#[test]
fn archive_rejects_escape_and_distinguishes_password_from_corruption() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let ctx = task_ctx();
    let root = sample_tree(dir.path()).0;
    let root_s = root.to_string_lossy().to_string();

    // 穿越：整包失败而不是跳过坏条目——静默跳过会让用户以为解压完整。
    let evil = dir.path().join("evil.zip");
    {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(fs::File::create(&evil).expect("建恶意 zip 失败"));
        w.start_file("../evil.txt", zip::write::SimpleFileOptions::default())
            .expect("写恶意条目失败");
        w.write_all(b"pwned").expect("写恶意内容失败");
        w.finish().expect("收尾失败");
    }
    let out = dir.path().join("out");
    fs::create_dir_all(&out).expect("建解压目录失败");
    let err = arch::extract::extract_archive(&extract_req(&evil, &out), &ctx)
        .expect_err("穿越条目必须被拒绝");
    assert_eq!(err.code, codes::PATH_ESCAPE, "穿越必须报 1003，实际 {err}");
    assert!(
        !dir.path().join("evil.txt").exists()
            && fs::read_dir(&out).expect("列目录").next().is_none(),
        "含穿越条目的包不允许在目标目录内外落任何文件"
    );

    // 密码三态：不给 / 给错 / 给对。
    let secret = dir.path().join("secret.7z");
    arch::create::create_archive(
        &CreateArchiveReq {
            password: Some("正确密码123".into()),
            encrypt_filenames: true,
            ..create_req(vec![root_s.clone()], &secret, "7z")
        },
        &ctx,
    )
    .expect("加密 7z 创建失败");
    let no_pwd = arch::list::list_archive(&secret, None).expect_err("不给密码必须打不开");
    assert_eq!(
        no_pwd.code,
        codes::ARCHIVE_PASSWORD_REQUIRED,
        "实际 {no_pwd}"
    );
    let wrong = arch::list::list_archive(&secret, Some("错的")).expect_err("错误密码必须打不开");
    assert_eq!(wrong.code, codes::ARCHIVE_WRONG_PASSWORD, "实际 {wrong}");
    arch::list::list_archive(&secret, Some("正确密码123")).expect("正确密码必须能开");

    // 损坏：正文中间翻一个字节破坏 CRC，测试必须报失败清单而不是说「好」。
    let pack = dir.path().join("pack.zip");
    arch::create::create_archive(&create_req(vec![root_s.clone()], &pack, "zip"), &ctx)
        .expect("创建失败");
    let mut bytes = fs::read(&pack).expect("回读");
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    fs::write(&pack, &bytes).expect("写损坏包");
    let result = arch::test_archive::test_archive(
        &notetext_lib::domain::archive::TestArchiveReq {
            archive: pack.to_string_lossy().to_string(),
            password: None,
        },
        &ctx,
    )
    .expect("测试本身不该崩");
    assert!(!result.ok, "损坏的包必须测出失败");
    assert!(!result.failures.is_empty(), "失败清单要给出具体条目与原因");
}
