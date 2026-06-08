/**
 * WorldviewBuffer — Elio 的外部感知缓冲区
 *
 * 外部事件（用户消息、系统事件、未来图像等）到达时立刻写入。
 * 主循环 tick 时 drain 全部感知 → 格式化为世界观文本 → 注入 Elio 上下文。
 *
 * 模块级单例 — 同 ContextBridge 的零依赖模式。
 */

export type Percept = {
  type: 'user_message'        // 未来扩展: 'image', 'other_message', 'system_event'
  speaker: string              // 'master' | 未来其他人名
  text: string
  timestamp: Date
}

let buffer: Percept[] = []

export const WorldviewBuffer = {
  /** 外部事件到达时立刻写入，fire-and-forget */
  push(percept: Percept): void {
    buffer.push(percept)
  },

  /** 取出当前全部感知并清空 — 防止同一条消息被重复注入 */
  drain(): Percept[] {
    const items = [...buffer]
    buffer = []
    return items
  },

  isEmpty(): boolean {
    return buffer.length === 0
  },

  /** 将感知条目格式化为自然语言 */
  formatForWorldview(percepts: Percept[]): string {
    if (percepts.length === 0) return ''
    return percepts
      .map(p => {
        const time = p.timestamp.toLocaleTimeString('zh-CN', { hour12: false })
        return `${p.speaker}说 (${time}):\n\"${p.text}\"`
      })
      .join('\n\n')
  },
}