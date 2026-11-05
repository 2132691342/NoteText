//! 只读归档长链路：CAB 真实读写、只读格式拒绝创建、cpio/ar 手工样本解析、RAR/LHA 真实包解析与预览。

mod common;

use common::{build_ar, build_cpio, extract_req, sample_tree, task_ctx};
use std::fs;
use std::path::Path;

use notetext_lib::domain::archive::PreviewEntryReq;
use notetext_lib::pkg::error::codes;
use notetext_lib::service::archive as arch;

#[test]
fn archive_readonly_formats_list_extract_and_refuse_create() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let ctx = task_ctx();
    let (root, _bin) = sample_tree(dir.path());
    let root_s = root.to_string_lossy().to_string();

    // CAB：写 → 列 → 解，三件事都要对得上。
    let cab = dir.path().join("pack.cab");
    arch::create::create_archive(&common::create_req(vec![root_s.clone()], &cab, "cab"), &ctx)
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
        let err = arch::create::create_archive(
            &common::create_req(vec![root_s.clone()], &dest, fmt),
            &ctx,
        )
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
}

#[test]
fn archive_reads_real_rar_and_lha_packages() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let ctx = task_ctx();
    let fx = Path::new(env!("CARGO_MANIFEST_DIR"))
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
