// 四种配置格式的解析与序列化：转换浮窗与一键快操作共用同一份实现。
// 只在前端做：这些格式在配置文件场景里都是小文档，不值得为它过一次 IPC。
export type ConvertFormat = 'json' | 'yaml' | 'toml' | 'xml'

/** 解析成普通 JS 对象：XML 的标签名即键，属性以 @ 前缀挂在节点上。 */
export async function parseFormat(text: string, fmt: ConvertFormat): Promise<unknown> {
  if (fmt === 'json') return JSON.parse(text)
  if (fmt === 'yaml') {
    const { load } = await import('js-yaml')
    return load(text)
  }
  if (fmt === 'toml') {
    const TOML = await import('@iarna/toml')
    return TOML.parse(text)
  }
  const { XMLParser } = await import('fast-xml-parser')
  const parser = new XMLParser({
    ignoreAttributes: false,
    attributeNamePrefix: '@_',
    // 数字 / 布尔自动转型：配置里 <port>8080</port> 该是数字不是字符串。
    parseTagValue: true,
    parseAttributeValue: true,
    trimValues: true,
  })
  return parser.parse(text)
}

/** 序列化：统一 2 空格缩进，便于 diff。 */
export async function stringifyFormat(
  data: unknown,
  fmt: ConvertFormat,
  indent = 2,
): Promise<string> {
  if (fmt === 'json') return JSON.stringify(data, null, indent)
  if (fmt === 'yaml') {
    const { dump } = await import('js-yaml')
    return dump(data, { indent, lineWidth: 120, noRefs: true })
  }
  if (fmt === 'toml') {
    const TOML = await import('@iarna/toml')
    return TOML.stringify(data as never)
  }
  const { XMLBuilder } = await import('fast-xml-parser')
  return new XMLBuilder({
    ignoreAttributes: false,
    attributeNamePrefix: '@_',
    format: true,
    indentBy: ' '.repeat(indent),
  }).build(data as never)
}

/** a → b 一步转换。 */
export async function convertText(
  text: string,
  from: ConvertFormat,
  to: ConvertFormat,
  indent = 2,
): Promise<string> {
  return stringifyFormat(await parseFormat(text, from), to, indent)
}

/**
 * 加转义：把正文变成 JSON 字符串字面量的内容（换行变 \n、引号加反斜杠）。
 *
 * JSON.stringify 的转义规则就是各语言字符串字面量的公共子集，
 * 不自己写替换表——手写表永远漏角落（\u2028 这类）。
 */
export function escapeText(text: string): string {
  return JSON.stringify(text).slice(1, -1)
}

/**
 * 去转义：把字符串字面量内容还原成真实字符（\n 变换行）。
 * 输入不是合法转义序列时抛错，由调用方给「下一步」提示。
 */
export function unescapeText(text: string): string {
  return JSON.parse(`"${text.replace(/\r?\n/g, '\\n')}"`) as string
}
