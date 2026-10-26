// i18n 审计：key 引用完整性、zh/en 对齐、模板裸中文、后端错误码与编码清单覆盖。
// 通过时只打一行；加 --verbose 额外列出未被静态引用的 key。
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)))
const SRC = join(ROOT, 'frontend', 'src')
const RUST_SRC = join(ROOT, 'src-tauri', 'src')
const verbose = process.argv.includes('--verbose')

const zhSrc = readFileSync(join(SRC, 'locales/zh-CN.ts'), 'utf8')
const enSrc = readFileSync(join(SRC, 'locales/en.ts'), 'utf8')

function walk(dir, ext, out = []) {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e)
    if (statSync(p).isDirectory()) {
      if (e !== 'node_modules' && e !== 'target') walk(p, ext, out)
    } else if (ext.test(e)) out.push(p)
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

// labelKey 是命令表的主形态，漏掉它会漏掉整类「界面显示原始 key」的 bug
const KEY_REF = [
  /\bt\(\s*'([^']+)'/g,
  /\bt\(\s*"([^"]+)"/g,
  /\b(?:labelKey|hintKey|titleKey|placeholderKey):\s*'([^']+)'/g,
]
const CJK = /[\u4e00-\u9fa5]/
const RAW_KEY_CALL = /\b(toast\.(ok|bad|warn|info)|askText|confirmText)\(\s*'[a-z][\w.]*'/

const used = new Map()
const hardcoded = []
const rawKeyCalls = []

for (const f of walk(SRC, /\.(ts|vue)$/)) {
  const s = readFileSync(f, 'utf8')
  const rel = relative(SRC, f)
  if (rel.includes('locales') || rel.endsWith('i18n.ts')) continue

  for (const re of KEY_REF) {
    for (const m of s.matchAll(re)) {
      const key = m[1]
      if (!used.has(key)) used.set(key, new Set())
      used.get(key).add(rel)
    }
  }

  if (f.endsWith('.vue')) checkTemplate(s, rel)
  s.split('\n').forEach((line, i) => {
    if (RAW_KEY_CALL.test(line)) rawKeyCalls.push(`${rel}:${i + 1}  ${line.trim().slice(0, 80)}`)
  })
}

/** 模板段里出现中文：引号内的字面量与裸文本节点都算写死文案。 */
function checkTemplate(src, rel) {
  const tpl = (src.match(/<template>[\s\S]*?<\/template>/g) || []).join('\n')
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
    // 去掉标签、属性字面量与插值后仍有中文，就是写死的文本节点
    const bare = line
      .replace(/<[^>]*>/g, ' ')
      .replace(/'[^']*'|"[^"]*"|`[^`]*`/g, ' ')
      .replace(/\{\{[\s\S]*?\}\}/g, ' ')
    if (CJK.test(bare)) hardcoded.push(`${rel}  ${bare.trim().slice(0, 60)}`)
  }
}

// 后端错误码的 message 固定中文，前端必须按码各给一份文案
const rustCodes = new Set()
for (const f of walk(RUST_SRC, /\.rs$/)) {
  const s = readFileSync(f, 'utf8')
  for (const m of s.matchAll(/pub const [A-Z_0-9]+: &str = "(\d{4})"/g)) rustCodes.add(m[1])
  for (const m of s.matchAll(/(?:new|wrap)\(\s*"(\d{4})"/g)) rustCodes.add(m[1])
}

function localeBlock(src, from, until) {
  return src.slice(src.indexOf(from), src.indexOf(until, src.indexOf(from)))
}

/** 某段 locale 里缺哪些 id（错误码 / 编码 id）。 */
function missingIds(src, blockFrom, blockUntil, ids) {
  const have = new Set(
    [...localeBlock(src, blockFrom, blockUntil).matchAll(/'([\w-]+)':/g)].map((m) => m[1]),
  )
  return [...ids].sort().filter((id) => !have.has(id))
}

// 编码 id 来自 encoding.rs 的清单与 supported() 两处
const encSrc = readFileSync(join(RUST_SRC, 'service/encoding.rs'), 'utf8')
const encIds = new Set([...encSrc.matchAll(/id: "([a-z0-9_-]+)"/g)].map((m) => m[1]))
for (const m of encSrc.matchAll(/^\s+\("([a-z0-9_-]+)", "/gm)) encIds.add(m[1])

const checks = [
  [
    '引用了但 zh-CN 缺',
    [...used].filter(([k]) => !zh.has(k)).map(([k, f]) => `${k}  <- ${[...f].join(', ')}`),
  ],
  ['引用了但 en 缺', [...used.keys()].filter((k) => !en.has(k))],
  ['只在 zh-CN 有', [...zh].filter((k) => !en.has(k))],
  ['只在 en 有', [...en].filter((k) => !zh.has(k))],
  ['模板里的裸中文', hardcoded],
  ['提示里直接传了 key 而非 t(key)', rawKeyCalls],
  ['zh-CN 缺的错误码文案', missingIds(zhSrc, 'errCode: {', 'next: {', rustCodes)],
  ['en 缺的错误码文案', missingIds(enSrc, 'errCode: {', 'next: {', rustCodes)],
  ['zh-CN 缺的编码显示名', missingIds(zhSrc, 'encodingLabel: {', '},', encIds)],
  ['en 缺的编码显示名', missingIds(enSrc, 'encodingLabel: {', '},', encIds)],
]

let bad = 0
for (const [title, items] of checks) {
  if (!items.length) continue
  bad += items.length
  console.log(`\n=== ${title} (${items.length}) ===`)
  for (const it of items) console.log(it)
}

if (verbose) {
  const orphan = [...zh].filter((k) => !used.has(k))
  console.log(`\n未被静态引用的 key: ${orphan.length}（动态拼接属正常）`)
  for (const k of orphan) console.log(`  ${k}`)
}

if (bad === 0) {
  console.log(
    `i18n 审计通过：zh ${zh.size} / en ${en.size} 个 key，错误码 ${rustCodes.size}，编码 ${encIds.size}`,
  )
} else {
  console.log(`\n问题合计: ${bad}`)
}
process.exit(bad === 0 ? 0 : 1)
