/**
 * WorldviewBuffer — Elio 的外部感知缓冲区 + 短期记忆环
 *
 * 外部事件（用户消息、系统事件、未来图像等）到达时立刻写入。
 * 主循环 tick 时 drain 全部感知 → 存入短期记忆环（最近7条）
 * → 格式化为世界观文本 → 注入 Elio 上下文。
 *
 * 模块级单例 — 同 ContextBridge 的零依赖模式。
 */

export type Percept = {
  type: 'user_message'
  speaker: string
  text: string
  timestamp: Date
}

/** 一个时间切片：一次 tick 的感知快照 */
export interface PerceptionSlice {
  time: Date
  summary: string     // "master说: ..." 或 "(无事件)"
}

const MAX_RECENT_SLICES = 7

let buffer: Percept[] = []
let recentSlices: PerceptionSlice[] = []

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

  /**
   * 将当前感知存入短期记忆环（保留最近7条）。
   * 每次 tick 调用一次 — 在主循环 buildWorldview 中调用。
   */
  commitSlice(percepts: Percept[]): void {
    const now = new Date()
    const timeLabel = now.toLocaleTimeString('zh-CN', { hour12: false })

    let summary: string
    if (percepts.length === 0) {
      summary = `[${timeLabel}] (无事件)`
    } else {
      summary = percepts
        .map(p => `[${timeLabel}] ${p.speaker}: "${p.text.slice(0, 80)}${p.text.length > 80 ? '...' : ''}"`)
        .join('\n')
    }

    recentSlices.push({ time: now, summary })
    // 环形淘汰：保留最近7条
    if (recentSlices.length > MAX_RECENT_SLICES) {
      recentSlices = recentSlices.slice(-MAX_RECENT_SLICES)
    }
  },

  /** 获取最近 N 条感知切片（用于 worldview 展示） */
  getRecentSlices(): PerceptionSlice[] {
    return [...recentSlices]
  },

  /** 清空短期记忆环（会话重置时） */
  clearSlices(): void {
    recentSlices = []
  },

  /** 将感知条目格式化为自然语言 */
  formatForWorldview(percepts: Percept[]): string {
    if (percepts.length === 0) return ''
    return percepts
      .map(p => {
        const time = p.timestamp.toLocaleTimeString('zh-CN', { hour12: false })
        return `${p.speaker}说 (${time}):\n"${p.text}"`
      })
      .join('\n\n')
  },
}
