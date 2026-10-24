// 错误码 → 人话提示 + 下一步动作。
// 后端给的 message 已经是人话，这里补两件后端做不到的事：
import { normalizeError } from '@/api'
import { tr } from '@/i18n'
import type { AppError } from '@/types/api'

/** 按错误码给下一步动作。没有匹配时给空串（不编造动作）。 */
export function nextStep(err: AppError): string {
  const c = err.code
  if (c === '1005' || c === 'no-backend') return ''
  if (c === '1000') return tr('next.1000')
  if (c === '1001') return tr('next.1001')
  if (c === '1002') return tr('next.1002')
  if (c === '1003') return tr('next.1003')
  if (c === '1004') return tr('next.1004')
  if (c === '2001' || c === '2002') return tr('next.2000')
  if (c === '3001') return tr('next.3001')
  if (c === '4001') return tr('next.4001')
  if (c === '5001') return tr('next.5001')
  if (c === '5002') return tr('next.5002')
  if (c === '6001') return tr('next.6001')
  if (c === '7001') return tr('next.7001')
  if (c.startsWith('8')) return tr('next.8000')
  if (c === '9001') return tr('next.9001')
  if (c === '9002') return tr('next.9002')
  if (c === '9003') return tr('next.9003')
  if (c === '9004') return tr('next.9004')
  if (c === '9005') return tr('next.9005')
  return ''
}

/** 把任意异常转成一句完整的用户提示。 */
export function humanError(e: unknown): { message: string; step: string } {
  const err = normalizeError(e)
  return { message: err.message, step: nextStep(err) }
}

/** 常见 io 错误的细节翻译：把 os error 32 之类翻成用户能动手的那句话。 */
export function translateIoDetail(details: string): string {
  if (/os error 32|being used by another process/i.test(details)) {
    return tr('os.32')
  }
  if (/os error 5|Access is denied/i.test(details)) {
    return tr('os.5')
  }
  if (/os error 3|The system cannot find the path/i.test(details)) {
    return tr('os.3')
  }
  if (/os error 2|No such file/i.test(details)) {
    return tr('os.2')
  }
  return details
}
