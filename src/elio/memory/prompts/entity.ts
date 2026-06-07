/**
 * Slow Path prompt: focused entity extraction from events.
 * Used as a fallback or when the narrative prompt's entity list needs enrichment.
 */

export function buildEntityPrompt(
  eventText: string,
  knownEntities: string[] = [],
): string {
  const known = knownEntities.length > 0
    ? `\n已从上下文中识别的实体: ${knownEntities.join(', ')}`
    : ''

  return `你是一个实体提取模块。从以下对话中提取有意义的实体。${known}

## 对话内容
${eventText}

## 任务
提取以下类型的实体：
- 人名（如：产品经理、小王、张三）
- 项目名/产品名（如：支付模块、用户系统）
- 技术名词（如：同步回调、Redis、API）
- 事物名/概念（如：架构问题、生产事故）

用 JSON 数组格式返回（只返回 JSON，不要其他文字）：
["实体1", "实体2", "实体3"]

每个实体应该是 2-8 个字的简洁表述。最多提取 10 个实体。
如果没有明显实体，返回空数组 []。`
}
