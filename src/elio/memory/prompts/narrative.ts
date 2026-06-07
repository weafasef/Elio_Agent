/**
 * Slow Path prompt: extract narrative, entities, and emotion from an event
 * in the context of its neighbors.
 */

export interface NarrativeContextEvent {
  id: string
  speaker: string
  text: string
  timestamp: string
}

export function buildNarrativePrompt(
  currentEvent: NarrativeContextEvent,
  neighbors: NarrativeContextEvent[],
): string {
  const neighborText = neighbors.length > 0
    ? neighbors.map((n, i) =>
        `${i + 1}. [${n.id}] ${n.timestamp} ${n.speaker}: ${n.text}`
      ).join('\n')
    : '(无邻居事件)'

  return `你是一个记忆系统的深度分析模块。请分析以下对话事件及其上下文。

## 当前事件
- ID: ${currentEvent.id}
- 时间: ${currentEvent.timestamp}
- 说话者: ${currentEvent.speaker}
- 内容: ${currentEvent.text}

## 邻居事件（前后相关的对话）
${neighborText}

## 任务
请用 JSON 格式返回分析结果（只返回 JSON，不要其他文字）：

{
  "narrative": "用一段中文自然语言描述当前事件发生了什么，结合上下文理解其含义。控制在2-3句话。",
  "entities": ["提取涉及的实体：人名、项目名、技术名词、事物名等"],
  "emotion": {
    "label1": 0.8,
    "label2": 0.6
  }
}

情绪标签示例：开心、疲惫、愤怒、担忧、兴奋、失望、坚定、困惑、放松、焦虑
如果无法判断情绪，返回空对象 {}。`
}
