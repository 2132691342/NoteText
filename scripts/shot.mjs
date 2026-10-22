// 截图：抓当前视口存成 PNG。
//
// 用法：node scripts/shot.mjs [端口] [输出路径] [URL 片段]
import { connect } from './cdp.mjs'
import fs from 'node:fs'

const port = Number(process.argv[2] || 9225)
const out = process.argv[3] || '.check/shot.png'
const urlMatch = process.argv[4] || ''

const { screenshot, close } = await connect({ port, urlMatch })
await screenshot(out)
console.log('已保存', out, fs.statSync(out).size, 'bytes')
close()
