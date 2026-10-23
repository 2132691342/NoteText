// 编码显示名：后端只给 id 与一份中文兜底名，显示名归前端 i18n 管。
// 为什么不让后端出一个完整的 label：编码名是界面文案，跟着界面语言走。

import { tr } from '@/i18n'

/** 编码 id → 当前语言的显示名。没有专门文案时用 id 本身（比空白好定位）。 */
export function encodingLabel(id: string): string {
  const key = 'encodingLabel.' + id
  const text = tr(key)
  return !text || text === key ? id.toUpperCase() : text
}

/** 编码 id → 短名：状态栏空间紧张，只留常用缩写。 */
export function encodingShort(id: string): string {
  const map: Record<string, string> = {
    'utf-8': 'UTF-8',
    'utf-8-bom': 'UTF-8 BOM',
    'utf-16le': 'UTF-16 LE',
    'utf-16be': 'UTF-16 BE',
    'gb18030': 'GB18030',
    'big5': 'Big5',
    'shift_jis': 'Shift_JIS',
    'euc-jp': 'EUC-JP',
    'euc-kr': 'EUC-KR',
    'windows-1252': 'CP1252',
    'iso-8859-1': 'Latin-1',
    'koi8-r': 'KOI8-R',
  }
  return map[id] ?? id.toUpperCase()
}
