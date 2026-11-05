//! 集成测试共享设施：隔离装配与几组长链路反复用到的构造器。
//! 各 `chain_*.rs` 通过 `mod common;` 复用，本文件本身不会被编译成独立测试二进制。

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use notetext_lib::app::bootstrap::{self, AppState};
use notetext_lib::domain::archive::{CreateArchiveReq, ExtractArchiveReq};
use notetext_lib::domain::file::{FileContentVO, ReadMode};
use notetext_lib::domain::session::TabDO;
use notetext_lib::pkg::paths::AppPaths;
use notetext_lib::service::archive as arch;
use notetext_lib::service::archive::ctx::TaskCtx;

/// 装配隔离实例：临时目录 + 真实装配链，不碰 `%APPDATA%\NoteText`。
pub fn boot() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = AppPaths::from_root(dir.path());
    let state = bootstrap::build(&paths).expect("装配失败");
    (dir, state)
}

/// 构造一个标签行（用于会话往返）。
pub fn tab(ord: i32, path: &str, title: &str) -> TabDO {
    TabDO {
        ord,
        path: path.into(),
        title: title.into(),
        language: "text".into(),
        encoding: "utf-8".into(),
        line_ending: "crlf".into(),
        has_bom: false,
        cursor_line: 1,
        cursor_col: 1,
        scroll_ratio: 0.0,
        dirty: false,
        split: false,
        view_type: "code".into(),
        updated_at: 0,
        id: ord as i64,
    }
}

/// 写字节再读回，返回服务层读结果。
pub fn roundtrip(state: &AppState, file: &Path, bytes: &[u8]) -> FileContentVO {
    fs::write(file, bytes).expect("写测试文件失败");
    state
        .services
        .file
        .read(&file.to_string_lossy(), ReadMode::Full)
        .expect("读失败")
}

/// 归档样本：中文目录名 + 嵌套子目录 + 可压缩段与随机段，往返要逐字节一致。
pub fn sample_tree(dir: &Path) -> (PathBuf, PathBuf) {
    let root = dir.join("样本目录");
    let sub = root.join("子目录");
    fs::create_dir_all(&sub).expect("建样本目录失败");
    fs::write(root.join("说明.txt"), b"hello note text\r\nsecond line\n").expect("写样本失败");
    let mut blob = Vec::with_capacity(256 * 1024);
    for i in 0..256 * 1024 {
        blob.push((i % 251) as u8);
    }
    blob.extend_from_slice(&[0xE4, 0xB8, 0xAD, 0xE6, 0x96, 0x87, 0x00, 0xFF]);
    let bin = sub.join("数据.bin");
    fs::write(&bin, &blob).expect("写样本失败");
    (root, bin)
}

pub fn create_req(sources: Vec<String>, dest: &Path, format: &str) -> CreateArchiveReq {
    CreateArchiveReq {
        sources,
        dest: dest.to_string_lossy().to_string(),
        format: format.to_string(),
        method: None,
        level: None,
        password: None,
        encrypt_filenames: false,
        solid: false,
        split_to: None,
        exclude: Vec::new(),
        comment: None,
    }
}

pub fn extract_req(archive: &Path, dest: &Path) -> ExtractArchiveReq {
    ExtractArchiveReq {
        archive: archive.to_string_lossy().to_string(),
        dest: dest.to_string_lossy().to_string(),
        entries: None,
        password: None,
        overwrite: "overwrite".into(),
        create_subdir: false,
    }
}

/// 压缩引擎任务上下文：0 总量表示不限量，测试不需要进度回调。
pub fn task_ctx() -> TaskCtx {
    TaskCtx::new("test".into(), 0)
}

/// 打包再解压到独立目录，返回 (包路径, 解压产物根目录)。
pub fn pack_and_unpack(dir: &Path, src: &Path, format: &str, tag: &str) -> (PathBuf, PathBuf) {
    let archive = dir.join(format!("{tag}.{format}"));
    let req = create_req(vec![src.to_string_lossy().to_string()], &archive, format);
    arch::create::create_archive(&req, &task_ctx())
        .unwrap_or_else(|e| panic!("{format} 创建失败：{e}"));
    let out = dir.join(format!("x-{tag}"));
    arch::extract::extract_archive(&extract_req(&archive, &out), &task_ctx())
        .unwrap_or_else(|e| panic!("{format} 解压失败：{e}"));
    (archive, out)
}

/// 手工构造 cpio newc 单条目样本：头字段偏移算错一个字节解出来就是垃圾。
pub fn build_cpio(body: &[u8], name: &[u8]) -> Vec<u8> {
    let mut cpio = Vec::new();
    let push = |out: &mut Vec<u8>, name_len: usize, size: usize| {
        let h = format!(
            "070701{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}",
            0u32,
            0o100644u32,
            0u32,
            0u32,
            1u32,
            0u32,
            size as u32,
            0u32,
            0u32,
            0u32,
            0u32,
            name_len,
            0u32,
        );
        out.extend_from_slice(h.as_bytes());
    };
    push(&mut cpio, name.len(), body.len());
    cpio.extend_from_slice(name);
    while cpio.len() % 4 != 0 {
        cpio.push(0);
    }
    cpio.extend_from_slice(body);
    while cpio.len() % 4 != 0 {
        cpio.push(0);
    }
    push(&mut cpio, b"TRAILER!!!\0".len(), 0);
    cpio.extend_from_slice(b"TRAILER!!!\0");
    while cpio.len() % 4 != 0 {
        cpio.push(0);
    }
    cpio
}

/// 手工构造 ar 单条目样本。
pub fn build_ar(data: &[u8], name: &str) -> Vec<u8> {
    let mut ar = b"!<arch>\n".to_vec();
    let h = format!(
        "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
        name,
        "1700000000",
        "0",
        "0",
        "100644",
        data.len()
    );
    ar.extend_from_slice(h.as_bytes());
    ar.extend_from_slice(data);
    if data.len() % 2 == 1 {
        ar.push(b'\n');
    }
    ar
}

// ---------- 表格编辑样本：一个带样式的极小 xlsx ----------

/// 用原始部件手工拼一个带样式的 xlsx：两张表 + 共享串 + styles.xml。
pub fn build_styled_xlsx(path: &std::path::Path) {
    use std::io::Write;
    let file = fs::File::create(path).expect("建 xlsx 失败");
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let parts: [(&str, &[u8]); 6] = [
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("xl/workbook.xml", WORKBOOK_XML.as_bytes()),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.as_bytes()),
        ("xl/sharedStrings.xml", SHARED_STRINGS.as_bytes()),
        ("xl/styles.xml", STYLES_XML.as_bytes()),
        ("xl/worksheets/sheet1.xml", SHEET1_XML.as_bytes()),
    ];
    for (name, body) in parts {
        zip.start_file(name, opts).expect("写部件失败");
        zip.write_all(body).expect("写部件内容失败");
    }
    zip.start_file("xl/worksheets/sheet2.xml", opts)
        .expect("写 sheet2 失败");
    zip.write_all(SHEET2_XML.as_bytes())
        .expect("写 sheet2 内容失败");
    zip.finish().expect("收尾失败");
}

/// 读 xlsx 内某个部件的原始字节（用于断言「未编辑部件字节级原样」）。
pub fn read_zip_entry_bytes(path: &std::path::Path, name: &str) -> Vec<u8> {
    use std::io::Read;
    let bytes = fs::read(path).expect("读 xlsx 失败");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("解 zip 失败");
    let mut f = zip.by_name(name).expect("部件不存在");
    let mut out = Vec::new();
    f.read_to_end(&mut out).expect("读部件失败");
    out
}

pub const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>"#;

pub const WORKBOOK_XML: &str = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="数据" sheetId="1" r:id="rId1"/><sheet name="备注" sheetId="2" r:id="rId2"/></sheets></workbook>"#;

pub const WORKBOOK_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#;

pub const SHARED_STRINGS: &str = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>标题</t></si><si><t>备注内容</t></si></sst>"#;

pub const STYLES_XML: &str = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts><font><sz val="11"/></font><font><b/><sz val="16"/></font></fonts></styleSheet>"#;

pub const SHEET1_XML: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s" s="1"><v>0</v></c></row><row r="2"><c r="B2" s="2"><v>42</v></c></row></sheetData></worksheet>"#;

pub const SHEET2_XML: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>1</v></c></row></sheetData></worksheet>"#;
