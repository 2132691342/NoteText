// i18n 审计：key 引用完整性、zh/en 对齐、模板硬编码中文、后端错误码覆盖。
// 用法：node scripts/i18n-audit.mjs（从任何目录跑都行）
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

// 脚本放在 scripts/，前端源码在 ../frontend/src。按脚本位置定位，不依赖 cwd。
const ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const SRC = join(ROOT, 'frontend', 'src')
const RUST_SRC = join(ROOT, 'src-tauri', 'src')

function walk(dir, ext, out = []) {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e)
    const st = statSync(p)
    if (st.isDirectory()) {
      if (e !== 'node_modules' && e !== 'target') walk(p, ext, out)
    } else if (ext.test(e)) {
      out.push(p)
    }
  }
  return out
}

/** 按缩进还原嵌套，拍平成 a.b.c。 */
function flatKeys(src) {
  const set = new Set()
  const stack = []
  for (const line of src.split('\n')) {
    const sec = /^(\s*)([A-Za-z_$][\w$]*):\s*\{/.exec(line)
    if (sec) {
      const indent = sec[1].length
      while (stack.length && stack[stack.length - 1].indent >= indent) stack.pop()
      stack.push({ indent, name: sec[2] })
      continue
    }
    const leaf = /^(\s*)([A-Za-z_$][\w$]*):\s*['"`]/.exec(line)
    if (leaf && stack.length) {
      const indent = leaf[1].length
      while (stack.length && stack[stack.length - 1].indent >= indent) stack.pop()
      set.add(stack.map((s) => s.name).join('.') + '.' + leaf[2])
    }
  }
  return set
}

// labelKey 是命令表的主形态，漏掉它就会漏掉整整一类「显示原始 key」的 bug
const KEY_REF = [
  /\bt\(\s*'([^']+)'/g,
  /\bt\(\s*"([^"]+)"/g,
  /\blabelKey:\s*'([^']+)'/g,
  /\bhintKey:\s*'([^']+)'/g,
  /\btitleKey:\s*'([^']+)'/g,
  /\bplaceholderKey:\s*'([^']+)'/g,
]

const CJK = /[\u4e00-\u9fa5]/

/** 模板段里的裸中文：字符串字面量 + 文本节点。 */
function hardcodedCjk(src) {
  const hits = []
  let inBlockComment = false
  src.split('\n').forEach((line, i) => {
    const trimmed = line.trim()
    // 块注释跨行，状态要带到下一行
    if (inBlockComment) {
      if (trimmed.includes('-->') || trimmed.includes('*/')) inBlockComment = false
      return
    }
    if (/^(\/\/|\*|\/\*|<!--)/.test(trimmed)) {
      if (!trimmed.includes('-->') && !trimmed.includes('*/')) inBlockComment = true
      return
    }
    for (const m of line.matchAll(/'([^']*)'|"([^"]*)"|`([^`]*)`/g)) {
      const val = m[1] ?? m[2] ?? m[3] ?? ''
      if (CJK.test(val)) hits.push({ line: i + 1, text: val.trim().slice(0, 60) })
    }
    // 文本节点：去掉标签与插值后仍有中文，就是写死的文案
    const bare = line
      .replace(/<[^>]*>/g, ' ')
      .replace(/'[^']*'|"[^"]*"|`[^`]*`/g, ' ')
      .replace(/\{\{[\s\S]*?\}\}/g, ' ')
    if (CJK.test(bare)) hits.push({ line: i + 1, text: bare.trim().slice(0, 60) })
  })
  return hits
}

const zh = flatKeys(readFileSync(join(SRC, 'locales/zh-CN.ts'), 'utf8'))
const en = flatKeys(readFileSync(join(SRC, 'locales/en.ts'), 'utf8'))

const used = new Map()
const hardcoded = []
const unTdKeys = []
for (const f of walk(SRC, /\.(ts|vue)$/)) {
  const s = readFileSync(f, 'utf8')
  const rel = relative(SRC, f)
  if (rel.includes('locales') || rel.endsWith('i18n.ts')) continue
  for (const re of KEY_REF) {
    let m
    while ((m = re.exec(s))) {
      if (!used.has(m[1])) used.set(m[1], new Set())
      used.get(m[1]).add(rel)
    }
  }
  const tpl = f.endsWith('.vue')
    ? (s.match(/<template>[\s\S]*?<\/template>/g) || []).join('\n')
    : s
  const h = hardcodedCjk(tpl)
  if (h.length) hardcoded.push({ file: rel, hits: h })

  // toast / 弹窗提示直接传 key 而不是 t(key)：界面上会显示原始 key
  s.split('\n').forEach((line, i) => {
    if (/\b(toast\.(ok|bad|warn|info)|askText|confirmText)\(\s*'[a-z][\w.]*'/.test(line)) {
      unTdKeys.push(rel + ':' + (i + 1) + '  ' + line.trim().slice(0, 80))
    }
  })
}

let bad = 0

/** 打一小节：空节打「(无)」，有内容逐行列出并计入问题数。 */
function section(title, items) {
  console.log('\n=== ' + title + ' (' + items.length + ') ===')
  if (!items.length) {
    console.log('(无)')
    return
  }
  bad += items.length
  for (const it of items) console.log(it)
}

console.log('key 统计: zh=' + zh.size + ' en=' + en.size + ' 被引用=' + used.size)

section(
  '引用了但 zh-CN 缺',
  [...used].filter(([k]) => !zh.has(k)).map(([k, f]) => k + '  <- ' + [...f].join(', ')),
)
section('引用了但 en 缺', [...used.keys()].filter((k) => !en.has(k)))
section('只在 zh-CN 有', [...zh].filter((k) => !en.has(k)))
section('只在 en 有', [...en].filter((k) => !zh.has(k)))

// 后端错误码：Rust 侧 message 固定中文，前端必须按码各给一份文案
const rustCodes = new Set()
for (const f of walk(RUST_SRC, /\.rs$/)) {
  const s = readFileSync(f, 'utf8')
  for (const m of s.matchAll(/pub const [A-Z_0-9]+: &str = "(\d{4})"/g)) rustCodes.add(m[1])
  for (const m of s.matchAll(/(?:new|wrap)\(\s*"(\d{4})"/g)) rustCodes.add(m[1])
}
const sortedCodes = [...rustCodes].sort()
function errCodes(localeSrc, name) {
  const seg = localeSrc.slice(localeSrc.indexOf('errCode: {'), localeSrc.indexOf('next: {'))
  const have = new Set([...seg.matchAll(/'(\d{4})':/g)].map((m) => m[1]))
  const missing = sortedCodes.filter((c) => !have.has(c))
  return { name, total: sortedCodes.length, missing }
}
const zhErrs = errCodes(readFileSync(join(SRC, 'locales/zh-CN.ts'), 'utf8'), 'zh-CN')
const enErrs = errCodes(readFileSync(join(SRC, 'locales/en.ts'), 'utf8'), 'en')
console.log('\n后端错误码: ' + sortedCodes.length)
section('zh-CN 缺的错误码文案', zhErrs.missing)
section('en 缺的错误码文案', enErrs.missing)

// 编码清单：后端给 id，显示名在 locales 的 encodingLabel 段
const encIds = [
  ...new Set(
    [...readFileSync(join(RUST_SRC, 'service/encoding.rs'), 'utf8').matchAll(/id: "([a-z0-9_-]+)"/g)].map(
      (m) => m[1],
    ),
  ),
]
const encLoop = [
  ...new Set(
    [...readFileSync(join(RUST_SRC, 'service/encoding.rs'), 'utf8').matchAll(/^\s+\("([a-z0-9_-]+)", "/gm)].map(
      (m) => m[1],
    ),
  ),
]
const allEnc = [...new Set([...encIds, ...encLoop])].sort()
function encLabels(localeSrc) {
  const start = localeSrc.indexOf('encodingLabel: {')
  const seg = localeSrc.slice(start, localeSrc.indexOf('},', start))
  const have = new Set([...seg.matchAll(/'([a-z0-9_-]+)':/g)].map((m) => m[1]))
  return allEnc.filter((id) => !have.has(id))
}
console.log('\n编码清单: ' + allEnc.length)
section('zh-CN 缺的编码显示名', encLabels(readFileSync(join(SRC, 'locales/zh-CN.ts'), 'utf8')))
section('en 缺的编码显示名', encLabels(readFileSync(join(SRC, 'locales/en.ts'), 'utf8')))

const orphan = [...zh].filter((k) => !used.has(k))
console.log('\n未被静态引用的 key: ' + orphan.length + '（动态拼接属正常，仅供参考）')

section('提示里直接传了 key 而非 t(key)', unTdKeys)

if (hardcoded.length) {
  console.log('\n=== 模板里的裸中文 ===')
  let n = 0
  for (const { file, hits } of hardcoded) {
    for (const h of hits) {
      console.log(file + ':' + h.line + '  ' + h.text)
      n++
    }
  }
  bad += n
}

console.log(bad === 0 ? '\n全部通过' : '\n问题合计: ' + bad)
process.exit(bad === 0 ? 0 : 1)
