// 分包体检：看每个 chunk 的体积与它拉进了哪些库。
//
// 为什么要有这个：首屏体积由分包策略决定，而策略写错时
// 「只写 txt 的用户也下载了 2.3MB 的 xlsx」这种问题在构建日志里看不出来。
//
// 用法：先 npm --prefix frontend run build，再 node scripts/inspect-chunks.mjs
import fs from 'node:fs'
import path from 'node:path'

const dist = path.join('frontend', 'dist', 'assets')
if (!fs.existsSync(dist)) {
  console.error('没有构建产物：先跑 npm --prefix frontend run build')
  process.exit(1)
}

const LIBS = [
  'vue-pdf-embed',
  'docx-preview',
  'xlsx',
  'pdfjs',
  'echarts',
  'zrender',
  '@wangeditor',
  'wangeditor',
  'mavon',
  'codemirror',
  '@codemirror',
  '@lezer',
  'element-plus',
  '@element-plus',
  'vue-i18n',
  '@intlify',
  'pinia',
  'vue-router',
]

const files = fs.readdirSync(dist).filter((f) => f.endsWith('.js'))
const rows = []
for (const f of files) {
  const s = fs.readFileSync(path.join(dist, f), 'utf8')
  const hits = LIBS.filter((l) => s.includes(l))
  rows.push({ file: f, kb: +(s.length / 1024).toFixed(1), libs: hits.join(', ') })
}

rows.sort((a, b) => b.kb - a.kb)

const total = rows.reduce((n, r) => n + r.kb, 0)
console.log(`共 ${rows.length} 个 chunk，合计 ${total.toFixed(1)} kB\n`)
console.log('体积'.padEnd(12) + 'chunk'.padEnd(38) + '包含的库')
console.log('-'.repeat(100))
for (const r of rows) {
  console.log(
    `${String(r.kb).padStart(8)} kB  ${r.file.padEnd(36)} ${r.libs || '—'}`,
  )
}

const big = rows.filter((r) => r.kb > 500)
if (big.length) {
  console.log(`\n超过 500 kB 的 chunk：${big.map((r) => r.file).join(', ')}`)
  console.log('这些应当是按需加载的；首屏只应加载入口 + vendor。')
}
