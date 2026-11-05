//! 归档长链路：可写格式全生命周期、只读格式解析、异常包护栏。
//!
//! 归档这一域的失败面全是「静默的坏」：往返丢字节、中文名与层级塌陷、
//! 改名后认不出 tar 家族、缺密码与密码错混成一个错、损坏包测试照样说「好」。
//! 每条链路按失败形态分组，失败报文直接指认坏在哪一步。

mod common;

use common::{
    build_ar, build_cpio, create_req, extract_req, pack_and_unpack, sample_tree, task_ctx,
};
use std::fs;
use std::path::PathBuf;

use notetext_lib::domain::archive::{
    AddEntriesReq, CreateArchiveReq, DeleteEntriesReq, PreviewEntryReq,
};
use notetext_lib::pkg::error::codes;
use notetext_lib::service::archive as arch;

/// 可写格式链路：15 种格式创建→解压逐字节一致、改名仍认 tar 家族、
/// 追加 / 删除条目、分卷切分与合并还原。
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

/// 只读格式链路：CAB 真实读写、只读格式拒绝创建、cpio/ar 手工样本解析、
/// 仓库内真实 RAR / LHA 包的列条目、解压与预览。
#[test]
fn archive_readonly_formats_list_extract_and_refuse_create() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let ctx = task_ctx();
    let (root, _bin) = sample_tree(dir.path());
    let root_s = root.to_string_lossy().to_string();

    // CAB：写 → 列 → 解，三件事都要对得上。
    let cab = dir.path().join("pack.cab");
    arch::create::create_archive(&create_req(vec![root_s.clone()], &cab, "cab"), &ctx)
        .unwrap_or_else(|e| panic!("CAB 创建失败：{e}"));
    let listing = arch::list::list_archive(&cab, None).expect("CAB 列表失败");
    let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.ends_with("说明.txt")),
        "CAB 条目表缺 说明.txt，实际 {names:?}"
    );
    let out = dir.path().join("x-cab");
    arch::extract::extract_archive(&extract_req(&cab, &out), &ctx)
        .unwrap_or_else(|e| panic!("CAB 解压失败：{e}"));
    assert_eq!(
        fs::read(out.join("样本目录").join("说明.txt")).expect("CAB 解压后缺 说明.txt"),
        b"hello note text\r\nsecond line\n",
        "CAB 往返后文本内容变了"
    );

    // 只读格式：创建必须报「不支持创建」而不是走到一半才失败。
    for fmt in ["rar", "lzh", "cpio", "ar"] {
        let dest = dir.path().join(format!("bad.{fmt}"));
        let err = arch::create::create_archive(&create_req(vec![root_s.clone()], &dest, fmt), &ctx)
            .expect_err(&format!("{fmt} 不该被允许创建"));
        assert_eq!(err.code, codes::ARCHIVE_UNSUPPORTED, "{fmt} 实际报 {err}");
        assert!(!dest.exists(), "{fmt} 创建失败不该留下半个文件");
    }

    // 只读容器（cpio / ar）头部偏移算错一个字节解出来就是垃圾：手工构造后必须列得出、解得出。
    let cpio_path = dir.path().join("pack.cpio");
    fs::write(&cpio_path, build_cpio(b"hello cpio", b"readme.txt")).expect("写 cpio 失败");
    let cpio_list = arch::list::list_archive(&cpio_path, None).expect("cpio 列表失败");
    assert_eq!(cpio_list.entries[0].name, "readme.txt", "cpio 条目名读错");
    arch::extract::extract_archive(&extract_req(&cpio_path, &dir.path().join("x-cpio")), &ctx)
        .unwrap_or_else(|e| panic!("cpio 解压失败：{e}"));

    let ar_path = dir.path().join("pack.ar");
    fs::write(&ar_path, build_ar(b"ar-body-data", "control.tar")).expect("写 ar 失败");
    let ar_list = arch::list::list_archive(&ar_path, None).expect("ar 列表失败");
    assert_eq!(ar_list.entries[0].name, "control.tar", "ar 条目名读错");
    arch::extract::extract_archive(&extract_req(&ar_path, &dir.path().join("x-ar")), &ctx)
        .unwrap_or_else(|e| panic!("ar 解压失败：{e}"));

    // 仓库内的真实 RAR / LHA 样本：头解析错会表现为空列表或一屏乱码。
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let rar = fx.join("unicode.rar");
    let listing = arch::list::list_archive(&rar, None).expect("RAR 列表失败");
    assert_eq!(listing.entries.len(), 1, "RAR 应只列出 1 个条目");
    assert_eq!(listing.format, "rar");
    let out = dir.path().join("x-rar");
    let res = arch::extract::extract_archive(&extract_req(&rar, &out), &ctx)
        .unwrap_or_else(|e| panic!("RAR 解压失败：{e}"));
    assert_eq!(res.file_count, 1, "RAR 应解出 1 个文件");
    assert_eq!(res.total_size, 17, "RAR 解出的字节数不对");

    let lzh = fx.join("sample.lzh");
    let listing = arch::list::list_archive(&lzh, None).expect("LHA 列表失败");
    assert_eq!(listing.format, "lzh");
    assert_eq!(listing.entries[0].name, "readme.txt", "LHA 条目名读错");
    let out = dir.path().join("x-lzh");
    arch::extract::extract_archive(&extract_req(&lzh, &out), &ctx)
        .unwrap_or_else(|e| panic!("LHA 解压失败：{e}"));
    assert_eq!(
        fs::read(out.join("readme.txt")).expect("LHA 解压后缺条目"),
        b"hello lzh",
        "LHA 解出来的内容不对"
    );

    // 只读格式的预览也要能用：用户在浏览浮窗里点一下条目就该看到前几行。
    let preview = arch::preview::preview_entry(
        &PreviewEntryReq {
            archive: lzh.to_string_lossy().to_string(),
            entry: "readme.txt".into(),
            password: None,
            max_bytes: 8,
        },
        &ctx,
    )
    .expect("LHA 预览失败");
    assert_eq!(
        preview.content.as_deref(),
        Some("hello lz"),
        "LHA 预览读到的内容不对"
    );
}

/// 异常包链路：路径穿越必须整包拒绝、缺密码与密码错要分得开、
/// 损坏包测试必须报失败清单而不是说「好」。
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
