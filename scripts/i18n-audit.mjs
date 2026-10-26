// i18n 审计：key 引用完整性、zh/en 对齐、模板硬编码中文、后端错误码与编码清单覆盖。
// 五类问题编译期都抓不到，交给这里机械核对；从任何目录跑都行：
//   node scripts/i18n-audit.mjs
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

// 脚本在 scripts/，源码按脚本位置反推，不依赖 cwd。
const ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const SRC = join(ROOT, 'frontend', 'src')
const RUST_SRC = join(ROOT, 'src-tauri', 'src')

const zhSrc = readFileSync(join(SRC, 'locales/zh-CN.ts'), 'utf8')
const enSrc = readFileSync(join(SRC, 'locales/en.ts'), 'utf8')

function walk(dir, ext, out = []) {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e)
    if (statSync(p).isDirectory()) {
      if (e !== 'node_modules' && e !== 'target') walk(p, ext, out)
    } else if (ext.test(e)) {
      out.push(p)
    }
  }
  return out
}

/** 按缩进还原嵌套，把 locale 源码拍平成 a.b.c 的 key 集合。 */
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

const zh = flatKeys(zhSrc)
const en = flatKeys(enSrc)

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

const used = new Map()
const hardcoded = []
const rawKeyCalls = []
for (const f of walk(SRC, /\.(ts|vue)$/)) {
  const s = readFileSync(f, 'utf8')
  const rel = relative(SRC, f)
  if (rel.includes('locales') || rel.endsWith('i18n.ts')) continue
  for (const re of KEY_REF) {
    for (const m of s.matchAll(re)) {
      if (!used.has(m[1])) used.set(m[1], new Set())
      used.get(m[1]).add(rel)
    }
  }

  // 只查 .vue 的模板段；注释里的中文说明不算文案
  const tpl = f.endsWith('.vue')
    ? (s.match(/<template>[\s\S]*?<\/template>/g) || []).join('\n')
    : ''
  let inComment = false
  for (const line of tpl.split('\n')) {
    const t = line.trim()
    if (inComment) {
      if (t.includes('-->')) inComment = false
      continue
    }
    if (t.startsWith('<!--')) {
      if (!t.includes('-->')) inComment = true
      continue
    }
    for (const m of line.matchAll(/'([^']*)'|"([^"]*)"|`([^`]*)`/g)) {
      const val = m[1] ?? m[2] ?? m[3] ?? ''
      if (CJK.test(val)) hardcoded.push(`${rel}  ${val.trim().slice(0, 60)}`)
    }
    // 文本节点：去掉标签与插值后仍有中文，就是写死的文案
    const bare = line
      .replace(/<[^>]*>/g, ' ')
      .replace(/'[^']*'|"[^"]*"|`[^`]*`/g, ' ')
      .replace(/\{\{[\s\S]*?\}\}/g, ' ')
    if (CJK.test(bare)) hardcoded.push(`${rel}  ${bare.trim().slice(0, 60)}`)
  }

  // toast / 弹窗提示直接传 key 而不是 t(key)：界面上会显示原始 key
  s.split('\n').forEach((line, i) => {
    if (/\b(toast\.(ok|bad|warn|info)|askText|confirmText)\(\s*'[a-z][\w.]*'/.test(line)) {
      rawKeyCalls.push(`${rel}:${i + 1}  ${line.trim().slice(0, 80)}`)
    }
  })
}

const problems = []
function report(title, items) {
  for (const it of items) problems.push(`[${title}] ${it}`)
}

report(
  '引用了但没定义',
  [...used].filter(([k]) => !zh.has(k)).map(([k, f]) => `${k}  <- ${[...f].join(', ')}`),
)
report('en 缺 key', [...used.keys()].filter((k) => !en.has(k)))
report('zh-CN 多出 key', [...zh].filter((k) => !en.has(k)))
report('en 多出 key', [...en].filter((k) => !zh.has(k)))
report('模板里的裸中文', hardcoded)
report('提示直接传 key', rawKeyCalls)

// 后端错误码：Rust 侧 message 固定中文，前端必须按码各给一份文案（errCode 段）
const rustCodes = new Set()
for (const f of walk(RUST_SRC, /\.rs$/)) {
  const s = readFileSync(f, 'utf8')
  for (const m of s.matchAll(/pub const [A-Z_0-9]+: &str = "(\d{4})"/g)) rustCodes.add(m[1])
  for (const m of s.matchAll(/(?:new|wrap)\(\s*"(\d{4})"/g)) rustCodes.add(m[1])
}
function localeSegment(src, from, until) {
  return src.slice(src.indexOf(from), src.indexOf(until, src.indexOf(from)))
}
function missingCodes(src) {
  const have = new Set(
    [...localeSegment(src, 'errCode: {', 'next: {').matchAll(/'(\d{4})':/g)].map((m) => m[1]),
  )
  return [...rustCodes].sort().filter((c) => !have.has(c))
}
report('zh-CN 缺的错误码文案', missingCodes(zhSrc))
report('en 缺的错误码文案', missingCodes(enSrc))

// 编码清单：后端给 id，显示名在 locales 的 encodingLabel 段
const encSrc = readFileSync(join(RUST_SRC, 'service/encoding.rs'), 'utf8')
const encIds = new Set([...encSrc.matchAll(/id: "([a-z0-9_-]+)"/g)].map((m) => m[1]))
for (const m of encSrc.matchAll(/^\s+\("([a-z0-9_-]+)", "/gm)) encIds.add(m[1])
function missingEnc(src) {
  const have = new Set(
    [...localeSegment(src, 'encodingLabel: {', '},').matchAll(/'([a-z0-9_-]+)':/g)].map((m) => m[1]),
  )
  return [...encIds].sort().filter((id) => !have.has(id))
}
report('zh-CN 缺的编码显示名', missingEnc(zhSrc))
report('en 缺的编码显示名', missingEnc(enSrc))

if (problems.length) {
  for (const p of problems) console.log(p)
  console.log(`\n${problems.length} 处问题`)
  process.exit(1)
}
console.log(`i18n 通过：key ${zh.size}，错误码 ${rustCodes.size}，编码 ${encIds.size}`)
