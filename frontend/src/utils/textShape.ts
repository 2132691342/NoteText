// 文本形态探测：打开分流与编辑器挂载共用的宽行判断。
// 探测读后端头部 2MB 采样，结果按路径缓存——一次运行里同一文件只付一次采样钱。

import { api } from '@/api'
import type { TextShapeVO } from '@/types/api'

/** CodeMirror 渲染会冻结主线程的行宽上限（字节）：超过就该走分块只读。 */
export const WIDE_LINE_BYTES = 256 * 1024
/** 超过这个大小的文件才值得花一次探测：更小的文件全量读毫无压力。 */
export const SHAPE_PROBE_MIN = 4 * 1024 * 1024

const cache = new Map<string, TextShapeVO>()
const MAX_CACHED = 64

/** 探测文本形态（带缓存）。失败返回 null：调用方按普通流程走。 */
export async function probeShape(path: string): Promise<TextShapeVO | null> {
  const hit = cache.get(path)
  if (hit) return hit
  try {
    const shape = await api.probeTextShape(path)
    if (cache.size >= MAX_CACHED) {
      const oldest = cache.keys().next().value
      if (oldest !== undefined) cache.delete(oldest)
    }
    cache.set(path, shape)
    return shape
  } catch {
    return null
  }
}

/** 是否含超宽行。 */
export function isWide(shape: TextShapeVO | null): boolean {
  return !!shape && shape.max_line_bytes > WIDE_LINE_BYTES
}
