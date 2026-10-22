// 错误码 → 人话提示 + 下一步动作。
//
// 后端给的 message 已经是人话，这里补两件后端做不到的事：
// 按码段给「下一步」，以及把常见系统错误（文件被占用 / 权限不足）翻成用户能动手的那句话。
import { normalizeError } from '@/api'
import type { AppError } from '@/types/api'

/** 按错误码给下一步动作。没有匹配时给空串（不编造动作）。 */
export function nextStep(err: AppError): string {
  const c = err.code
  if (c === '1005' || c === 'no-backend') return ''
  if (c === '1001') return '确认文件没有被其他程序占用，然后重试。'
  if (c === '1003') return '换个位置保存，或检查路径里有没有 .. 之类的越界写法。'
  if (c === '1004') return '用「只读前段」模式打开大文件。'
  if (c === '2001' || c === '2002') return '重启应用；若持续出现，请到设置 → 关于里打开日志目录反馈。'
  if (c === '3001') return '在状态栏的编码菜单里手动选择正确编码重新载入。'
  if (c === '4001') return '刷新标签后重试。'
  if (c === '5001') return '用 JSON 工具先校验格式。'
  if (c === '5002') return '检查正则表达式，或去掉正则选项再试。'
  if (c === '6001') return '重启应用；若仍失败，换一个目录作为工作目录。'
  if (c === '7001') return '重启应用。'
  if (c.startsWith('8')) return '重启应用后重试。'
  return ''
}

/** 把任意异常转成一句完整的用户提示。 */
export function humanError(e: unknown): { message: string; step: string } {
  const err = normalizeError(e)
  return { message: err.message, step: nextStep(err) }
}

/** 常见 io 错误的细节翻译：把 os error 32 之类翻成中文。 */
export function translateIoDetail(details: string): string {
  if (/os error 32|being used by another process/i.test(details)) {
    return '文件正被其他程序占用'
  }
  if (/os error 5|Access is denied/i.test(details)) {
    return '没有访问权限'
  }
  if (/os error 3|The system cannot find the path/i.test(details)) {
    return '路径不存在'
  }
  if (/os error 2|No such file/i.test(details)) {
    return '文件不存在'
  }
  return details
}
