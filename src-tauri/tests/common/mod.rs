//! 集成测试的共享设施：隔离装配 + 跨域复用的样本构造器。
//!
//! 两条约束：
//! 1. 测试与生产共用同一条装配链（`bootstrap::build`），不引 mock 框架；
//! 2. 每个域的样本在自己的文件里构造，本模块只放真正跨域复用的那几样。

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

/// 构造一个标签行（会话往返用）。
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

/* ---------------- 归档样本（压缩域复用） ---------------- */

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

/// 压缩请求：默认参数，只改来源 / 目标 / 格式。
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

/// 解压请求：覆盖同名文件，不建子目录。
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

/* ---------------- XLSX 样本（表格编辑域复用） ---------------- */

/// 用原始部件手工拼一个带样式的 xlsx：两张表 + 共享串 + styles.xml。
pub fn build_styled_xlsx(path: &Path) {
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
pub fn read_zip_entry_bytes(path: &Path, name: &str) -> Vec<u8> {
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

/// 造一个合法的 Unix compress（.Z）样本：头 + 定长 9 位码字流的 LZW。
///
/// 测试必须自己编码，不能拿被测的解码器「自证」：解码器把码长增长点算错、
/// 或把字典初始化错，自己编自己解仍然能对上。这里只覆盖短输入（码长不会增长，
/// 字典也不清表），所以编码器可以写得极简：每个码字都是「已出现的最长匹配」或「新字节」，
/// 全部按 9 位定长写出——这正是解码器第一段的输入。
pub fn build_compress_z(body: &[u8]) -> Vec<u8> {
    // 头：魔数 + flags（0 = 非块模式）+ maxbits（9）。
    let mut out = vec![0x1Fu8, 0x9D, 0x09];

    // 标准 LZW：256 个基础码字 + 258 起的新串（256/257 是保留码）。
    let mut dict: std::collections::BTreeMap<Vec<u8>, u16> = std::collections::BTreeMap::new();
    let mut next: u16 = 258;
    let mut codes: Vec<u16> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();

    for &b in body {
        cur.push(b);
        if cur.len() == 1 {
            continue;
        }
        if dict.contains_key(&cur) {
            continue;
        }
        // 当前串不在字典里：发出前缀，登记当前串。
        let prefix = &cur[..cur.len() - 1];
        let code = if prefix.len() == 1 {
            u16::from(prefix[0])
        } else {
            *dict.get(prefix).expect("前缀必在字典里")
        };
        codes.push(code);
        dict.insert(cur.clone(), next);
        next += 1;
        cur = vec![b];
    }
    if !cur.is_empty() {
        let code = if cur.len() == 1 {
            u16::from(cur[0])
        } else {
            *dict.get(&cur).expect("尾巴必在字典里")
        };
        codes.push(code);
    }

    // 9 位码字按低位在前打包。
    let mut bitbuf: u32 = 0;
    let mut bits: u32 = 0;
    for code in codes {
        bitbuf |= u32::from(code) << bits;
        bits += 9;
        while bits >= 8 {
            out.push((bitbuf & 0xFF) as u8);
            bitbuf >>= 8;
            bits -= 8;
        }
    }
    if bits > 0 {
        out.push((bitbuf & 0xFF) as u8);
    }
    out
}

/// 造一个最小 ISO9660 镜像：根目录 + 两个文件（一个文本、一个二进制）。
///
/// 手写而不是拿现成镜像：只读引擎的价值全在「卷描述符偏移算对、
/// 目录记录长度对齐、根目录项指到第几个扇区」这几处，
/// 一份真实镜像解不出来时，用户看到的是空列表或一屏乱码。
pub fn build_iso9660(text: &[u8], bin: &[u8]) -> Vec<u8> {
    const SECTOR: usize = 2048;

    fn both16(v: u16) -> Vec<u8> {
        let mut out = v.to_le_bytes().to_vec();
        out.extend_from_slice(&v.to_be_bytes());
        out
    }
    fn both32(v: u32) -> Vec<u8> {
        let mut out = v.to_le_bytes().to_vec();
        out.extend_from_slice(&v.to_be_bytes());
        out
    }
    fn dirrec(name: &[u8], extent: u32, size: u32, is_dir: bool) -> Vec<u8> {
        let mut rec_len = 33 + name.len();
        if rec_len % 2 == 1 {
            rec_len += 1;
        }
        let mut r = vec![rec_len as u8, 0];
        r.extend_from_slice(&both32(extent));
        r.extend_from_slice(&both32(size));
        r.extend_from_slice(&[126, 1, 1, 0, 0, 0, 0]);
        r.extend_from_slice(&[if is_dir { 2 } else { 0 }, 0, 0]);
        r.extend_from_slice(&both16(1));
        r.push(name.len() as u8);
        r.extend_from_slice(name);
        r.resize(rec_len, 0);
        r
    }
    fn pad(b: &mut Vec<u8>) {
        while b.len() % SECTOR != 0 {
            b.push(0);
        }
    }

    let root_extent: u32 = 18;
    let mut layout: Vec<(&[u8], u32, u32, &[u8])> = Vec::new();
    let mut extent: u32 = 19;
    for (name, body) in [
        (b"HELLO.TXT;1".as_slice(), text),
        (b"DATA.BIN;1".as_slice(), bin),
    ] {
        layout.push((name, extent, body.len() as u32, body));
        extent += (body.len() + SECTOR - 1) as u32 / SECTOR as u32;
    }

    // 先用占位大小算一遍目录长度，再回填真实长度（root 的大小写在自身条目里）。
    let mut root: Vec<u8> = Vec::new();
    root.extend_from_slice(&dirrec(b"\x00", root_extent, 0, true));
    root.extend_from_slice(&dirrec(b"\x01", root_extent, 0, true));
    for (name, ext, size, _) in &layout {
        root.extend_from_slice(&dirrec(name, *ext, *size, false));
    }
    pad(&mut root);
    let root_size = root.len() as u32;

    let mut root2: Vec<u8> = Vec::new();
    root2.extend_from_slice(&dirrec(b"\x00", root_extent, root_size, true));
    root2.extend_from_slice(&dirrec(b"\x01", root_extent, root_size, true));
    for (name, ext, size, _) in &layout {
        root2.extend_from_slice(&dirrec(name, *ext, *size, false));
    }
    pad(&mut root2);

    let mut img = vec![0u8; 16 * SECTOR];
    let mut pvd = vec![0u8; SECTOR];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[8..40].fill(b' ');
    pvd[40..72].copy_from_slice(&{
        let mut v = b"NOTETEXT".to_vec();
        v.resize(32, b' ');
        v
    });
    pvd[80..84].copy_from_slice(&extent.to_le_bytes());
    pvd[84..88].copy_from_slice(&extent.to_be_bytes());
    pvd[120..122].copy_from_slice(&1u16.to_le_bytes());
    pvd[122..124].copy_from_slice(&1u16.to_be_bytes());
    pvd[124..126].copy_from_slice(&1u16.to_le_bytes());
    pvd[126..128].copy_from_slice(&1u16.to_be_bytes());
    pvd[128..130].copy_from_slice(&(SECTOR as u16).to_le_bytes());
    pvd[130..132].copy_from_slice(&(SECTOR as u16).to_be_bytes());
    let rr = dirrec(b"\x00", root_extent, root_size, true);
    pvd[156..156 + rr.len()].copy_from_slice(&rr);
    pvd[190..318].fill(b' ');
    pvd[318..446].fill(b' ');
    pvd[446..574].fill(b' ');
    pvd[574..702].fill(b' ');
    pvd[702..740].fill(b' ');
    pvd[740..776].fill(b' ');
    pvd[776..813].fill(b' ');
    let t = b"2024010100000000\x00";
    for start in [813usize, 830, 847, 864] {
        pvd[start..start + 17].copy_from_slice(t);
    }
    pvd[881] = 1;
    img.extend_from_slice(&pvd);

    let mut term = vec![0u8; SECTOR];
    term[0] = 255;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 1;
    img.extend_from_slice(&term);

    assert_eq!(img.len(), root_extent as usize * SECTOR);
    img.extend_from_slice(&root2);
    for (_, ext, _, body) in &layout {
        while img.len() < *ext as usize * SECTOR {
            img.push(0);
        }
        img.extend_from_slice(body);
        while img.len() % SECTOR != 0 {
            img.push(0);
        }
    }
    img
}
