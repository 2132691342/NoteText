// CDP 客户端：连到已开远程调试端口的浏览器，发送 CDP 命令。
//
// 用途：视觉与运行时实测（图标真实尺寸、字号、对比度、动效）。
// 浏览器需带 --remote-debugging-port=<port> 启动。
//
// 依赖 Node 18+（原生 WebSocket 与 fetch）。

const DEFAULT_PORT = 9225

/** 创建一个 CDP 会话，自动挑出目标页。 */
export async function connect({ port = DEFAULT_PORT, urlMatch } = {}) {
  const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
  const pages = list.filter((t) => t.type === 'page')
  const page = urlMatch ? pages.find((t) => t.url.includes(urlMatch)) : pages[0]
  if (!page) {
    throw new Error(
      `没找到页面。CDP 端口 ${port} 上现有：${pages.map((t) => t.url).join(', ') || '（无）'}`,
    )
  }

  let id = 0
  const pending = new Map()
  const ws = new WebSocket(page.webSocketDebuggerUrl)
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const mid = ++id
      pending.set(mid, resolve)
      ws.send(JSON.stringify({ id: mid, method, params }))
    })
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data)
    if (m.id && pending.has(m.id)) {
      pending.get(m.id)(m)
      pending.delete(m.id)
    }
  }
  await new Promise((resolve) => {
    ws.onopen = resolve
  })

  await send('Page.enable')
  await send('Runtime.enable')

  /** 在页面里求值，返回 JS 值。异常以 { error } 形式返回而不是抛出。 */
  const evaluate = async (expression) => {
    const r = await send('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    })
    if (r.result?.exceptionDetails) {
      const d = r.result.exceptionDetails
      return { error: d.exception?.description || d.text }
    }
    return r.result?.result?.value
  }

  /** 截当前视口，存到 out。 */
  const screenshot = async (out) => {
    const r = await send('Page.captureScreenshot', { format: 'png' })
    if (!r.result?.data) throw new Error('截图失败：没有拿到图像数据')
    const fs = await import('node:fs')
    fs.writeFileSync(out, Buffer.from(r.result.data, 'base64'))
    return out
  }

  return { send, evaluate, screenshot, close: () => ws.close() }
}
