//! 记录边界扫描与解析：分隔文本（CSV / TSV）。
//! 三条链路共用同一套状态机，记录边界必须一致——否则页边界与索引对不上：
//! - 字节级扫描器：建索引用（快，不解码）；
//! - 文本级流式解析器：分页读取与整文件重写用（要字段值）；
//! - 整段解析：单块读取用。

use memchr::memchr;

/// 一条记录的结束点。
#[derive(Debug, Clone, Copy)]
pub struct RecordEnd {
    /// 记录结束偏移（相对所喂那一段的起点）。
    pub end: u64,
    /// 该记录的列数。
    pub fields: usize,
}

/// 扫描单位：单字节编码用 u8，UTF-16 用 u16。
pub trait ScanUnit: Copy + Eq {
    /// 一个单位占几个字节（用于把单位下标换算成文件偏移）。
    const BYTES: usize;
    /// 引号字符。
    const QUOTE: Self;
    /// 换行字符。
    const NEWLINE: Self;

    /// 下一个换行 / 引号 / 分隔符在哪（返回最先出现者）。
    fn find_special(buf: &[Self], delim: Self, from: usize) -> Option<(usize, Self)>;

    /// 下一个引号在哪。
    fn find_quote(buf: &[Self], from: usize) -> Option<usize>;
}

impl ScanUnit for u8 {
    const BYTES: usize = 1;
    const QUOTE: u8 = b'"';
    const NEWLINE: u8 = b'\n';

    /// SIMD 找三类特殊字节：命中密集的 CSV 一行可能几十字节，逐字节比慢一个量级。
    fn find_special(buf: &[u8], delim: u8, from: usize) -> Option<(usize, u8)> {
        // 分隔符与引号 / 换行可能相同（NUL 分隔的 JSONL），这时 memchr3 不适用。
        if delim == b'"' || delim == b'\n' {
            let rest = &buf[from..];
            let nl = memchr(b'\n', rest);
            let qt = memchr(b'"', rest);
            let dl = memchr(delim, rest);
            let mut best: Option<(usize, u8)> = None;
            for (pos, unit) in [(nl, b'\n'), (qt, b'"'), (dl, delim)] {
                if let Some(p) = pos {
                    if best.map_or(true, |(bp, _)| p < bp) {
                        best = Some((p, unit));
                    }
                }
            }
            return best.map(|(p, u)| (from + p, u));
        }
        // memchr3 一次扫出三类特殊字节：比三次 memchr 少两遍内存遍历。
        memchr::memchr3(b'\n', b'"', delim, &buf[from..]).map(|p| {
            let at = from + p;
            let unit = buf[at];
            (at, unit)
        })
    }

    fn find_quote(buf: &[u8], from: usize) -> Option<usize> {
        memchr(b'"', &buf[from..]).map(|i| from + i)
    }
}

impl ScanUnit for u16 {
    const BYTES: usize = 2;
    const QUOTE: u16 = b'"' as u16;
    const NEWLINE: u16 = 0x0A;

    /// UTF-16 的换行 / 引号 / 分隔符都是两字节，逐单位找即可（该编码的 CSV 极少）。
    fn find_special(buf: &[u16], delim: u16, from: usize) -> Option<(usize, u16)> {
        (from..buf.len())
            .find(|&i| {
                let u = buf[i];
                u == 0x0A || u == b'"' as u16 || u == delim
            })
            .map(|i| (i, buf[i]))
    }

    fn find_quote(buf: &[u16], from: usize) -> Option<usize> {
        (from..buf.len()).find(|&i| buf[i] == b'"' as u16)
    }
}

/// 字节级记录扫描器：找出记录结束点，统计列数。
///
/// 状态机与 `service::file::parse_delimited` 同构：引号包裹、`""` 转义、字段内换行。
/// 引号字段的闭合引号要看下一个字符才能定性，所以状态跨块延续。
pub struct RecordScanner<U: ScanUnit> {
    delim: U,
    /// 在引号字段内。
    in_quotes: bool,
    /// 刚读到引号，闭合还是转义待下一个单位决定。
    pending_quote: bool,
    /// 当前字段还没有内容：裸引号是开引号还是普通字符由它决定。
    field_empty: bool,
    /// 当前记录已完成的列数。
    fields: usize,
    /// 见过的最大列数。
    max_fields: usize,
    /// 已完成的记录数。
    pub records: u64,
}

impl<U: ScanUnit> RecordScanner<U> {
    pub fn new(delim: U) -> Self {
        Self {
            delim,
            in_quotes: false,
            pending_quote: false,
            field_empty: true,
            fields: 0,
            max_fields: 0,
            records: 0,
        }
    }

    /// 见过的最大列数。
    pub fn max_fields(&self) -> usize {
        self.max_fields
    }

    /// 喂一段单位流，产出其中完整结束的那些记录。
    pub fn feed(&mut self, buf: &[U], base: u64, out: &mut Vec<RecordEnd>) {
        let mut i = 0usize;

        // 跨块待决引号在首位解决：成对的第二个引号是转义。
        if self.pending_quote {
            if buf.is_empty() {
                return;
            }
            if buf[0] == U::QUOTE {
                self.pending_quote = false;
                self.field_empty = false;
                i = 1;
            } else {
                self.pending_quote = false;
                self.in_quotes = false;
            }
        }

        while i < buf.len() {
            if self.in_quotes {
                // 引号字段内：只有引号有意义，换行与分隔符都是内容。
                let Some(q) = U::find_quote(buf, i) else {
                    self.field_empty = false;
                    return;
                };
                if q > i {
                    self.field_empty = false;
                }
                match buf.get(q + 1).copied() {
                    Some(c) if c == U::QUOTE => {
                        // `""` 转义：引号字段继续，字段里多一个引号字符。
                        self.field_empty = false;
                        i = q + 2;
                    }
                    Some(_) => {
                        // 闭引号：回到字段外，下一个单位按字段外规则处理。
                        self.in_quotes = false;
                        i = q + 1;
                    }
                    None => {
                        // 引号在块尾：判定推到下一块。
                        self.pending_quote = true;
                        return;
                    }
                }
                continue;
            }

            // 字段外：找下一个换行 / 引号 / 分隔符。
            let Some((pos, unit)) = U::find_special(buf, self.delim, i) else {
                self.field_empty = false;
                return;
            };
            if pos > i {
                self.field_empty = false;
            }
            if unit == U::QUOTE {
                if self.field_empty {
                    self.in_quotes = true;
                } else {
                    // 字段中间的裸引号：普通字符。
                    self.field_empty = false;
                }
                i = pos + 1;
            } else if unit == self.delim {
                self.fields += 1;
                self.field_empty = true;
                i = pos + 1;
            } else {
                // 换行：记录结束。
                self.fields += 1;
                self.max_fields = self.max_fields.max(self.fields);
                out.push(RecordEnd {
                    end: base + (pos as u64 + 1) * U::BYTES as u64,
                    fields: self.fields,
                });
                self.records += 1;
                self.fields = 0;
                self.field_empty = true;
                i = pos + 1;
            }
        }
    }
}

/// JSON 顶层数组的记录扫描器：按元素边界切。
///
/// 数组元素可以跨行（美化的 JSON 每个对象占十几行），所以按行切不成立。
/// 状态机跟踪花括号深度与字符串态，在深度 1 的 `,` / `]` 处断开。
pub struct JsonArrayScanner {
    /// 当前花括号 / 方括号深度。
    depth: usize,
    /// 在字符串里。
    in_string: bool,
    /// 上一位是反斜杠（转义）。
    escaped: bool,
    /// 已越过开头的 `[`。
    started: bool,
    /// 已完成的记录数。
    pub records: u64,
    /// 见过的最大列数（JSON 数组按 1 计）。
    max_fields: usize,
}

impl Default for JsonArrayScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonArrayScanner {
    pub fn new() -> Self {
        Self {
            depth: 0,
            in_string: false,
            escaped: false,
            started: false,
            records: 0,
            max_fields: 0,
        }
    }

    /// 见过的最大列数。
    pub fn max_fields(&self) -> usize {
        self.max_fields
    }

    /// 喂一段字节，产出其中完整结束的那些元素。
    pub fn feed(&mut self, buf: &[u8], base: u64, out: &mut Vec<RecordEnd>) {
        for (i, &b) in buf.iter().enumerate() {
            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                } else if b == b'\\' {
                    self.escaped = true;
                } else if b == b'"' {
                    self.in_string = false;
                }
                continue;
            }
            match b {
                b'"' => self.in_string = true,
                b'[' | b'{' => {
                    self.depth += 1;
                    self.started = true;
                }
                b']' => {
                    if self.depth > 0 {
                        self.depth -= 1;
                    }
                    // 深度 0 的 `]` 是数组结束：最后一个元素到此为止。
                    if self.depth == 0 && self.started {
                        out.push(RecordEnd {
                            end: base + i as u64 + 1,
                            fields: 1,
                        });
                        self.records += 1;
                        self.started = false;
                    }
                }
                b'}' => {
                    if self.depth > 0 {
                        self.depth -= 1;
                    }
                }
                // 深度 1 的 `,` 是元素分隔符（顶层数组里深度至少为 1）。
                b',' if self.depth == 1 && self.started => {
                    out.push(RecordEnd {
                        end: base + i as u64 + 1,
                        fields: 1,
                    });
                    self.records += 1;
                    self.started = false;
                }
                _ => {}
            }
        }
    }
}

/// 文本级流式解析器：跨块产出记录（重写整文件时用）。
///
/// 状态跨块延续，所以引号字段里的换行不会把记录切断。
pub struct StreamParser {
    delim: char,
    quoted: bool,
    pending_quote: bool,
    field_empty: bool,
    field: String,
    row: Vec<String>,
    /// 已完成的记录数。
    pub records: u64,
}

impl StreamParser {
    pub fn new(delim: char) -> Self {
        Self {
            delim,
            quoted: false,
            pending_quote: false,
            field_empty: true,
            field: String::new(),
            row: Vec::new(),
            records: 0,
        }
    }

    /// 喂一段文本，产出其中完整结束的那些记录。
    pub fn feed(&mut self, text: &str, out: &mut Vec<Vec<String>>) {
        for ch in text.chars() {
            if self.pending_quote {
                if ch == '"' {
                    // `""` 转义。
                    self.pending_quote = false;
                    self.field.push('"');
                    self.field_empty = false;
                    continue;
                }
                self.pending_quote = false;
                self.quoted = false;
            }
            if self.quoted {
                if ch == '"' {
                    self.pending_quote = true;
                } else {
                    self.field.push(ch);
                    self.field_empty = false;
                }
                continue;
            }
            if ch == '"' && self.field_empty {
                self.quoted = true;
            } else if ch == self.delim {
                self.row.push(std::mem::take(&mut self.field));
                self.field_empty = true;
            } else if ch == '\r' {
                // 引号外的 CR 丢弃（CRLF 与 CR 都算行尾）。
            } else if ch == '\n' {
                self.row.push(std::mem::take(&mut self.field));
                out.push(std::mem::take(&mut self.row));
                self.records += 1;
                self.field_empty = true;
            } else {
                self.field.push(ch);
                self.field_empty = false;
            }
        }
    }

    /// 文本结束时收尾：末尾没换行的残余算最后一条。
    pub fn finish(&mut self, out: &mut Vec<Vec<String>>) {
        if self.pending_quote {
            self.pending_quote = false;
            self.quoted = false;
        }
        if !self.field.is_empty() || !self.row.is_empty() {
            self.row.push(std::mem::take(&mut self.field));
            out.push(std::mem::take(&mut self.row));
            self.records += 1;
        }
    }
}

/// 整段解析：解析一段文本里的所有记录。
///
/// 状态机与 `RecordScanner` 一致，所以记录数与索引对得上。
pub fn parse_delimited(text: &str, delim: char) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut p = StreamParser::new(delim);
    p.feed(text, &mut out);
    p.finish(&mut out);
    out
}

/// 最小引号化：字段只在必要时才包引号。
pub fn quote_field(value: &str, delim: char) -> String {
    if value.contains(delim) || value.contains('"') || value.contains('\n') || value.contains('\r')
    {
        let mut out = String::with_capacity(value.len() + 2);
        out.push('"');
        for ch in value.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
        out
    } else {
        value.to_string()
    }
}

/// 一条记录序列化成一行文本（含行尾）。
pub fn serialize_row(fields: &[String], delim: char, eol: &str) -> String {
    let mut out = String::new();
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(delim);
        }
        out.push_str(&quote_field(f, delim));
    }
    out.push_str(eol);
    out
}

/// 嗅探分隔符：前若干行里出现次数最多的候选。
pub fn sniff_delimiter(text: &str) -> char {
    const CANDIDATES: [char; 4] = [',', '\t', ';', '|'];
    let head: String = text.chars().take(64 * 1024).collect();
    let mut best = ',';
    let mut best_hits = 0usize;
    for c in CANDIDATES {
        let hits = head
            .lines()
            .take(10)
            .map(|l| l.matches(c).count())
            .sum::<usize>();
        if hits > best_hits {
            best_hits = hits;
            best = c;
        }
    }
    best
}
