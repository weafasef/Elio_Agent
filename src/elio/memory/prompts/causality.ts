/**
 * Slow Path prompt: infer causal, semantic, and entity edges
 * between a target event and its neighbor events.
 */

export interface EdgeInferenceTarget {
  id: string
  narrative: string
  entities: string[]
  rawText: string
}

export function buildCausalityPrompt(
  target: EdgeInferenceTarget,
  neighbors: EdgeInferenceTarget[],
): string {
  if (neighbors.length === 0) {
    return ''
  }

  const targetText = target.narrative || target.rawText
  const neighborList = neighbors
    .map((n, i) => {
      const text = n.narrative || n.rawText
      return `${i + 1}. [${n.id}] ${text}  (实体: ${n.entities.join(', ') || '无'})`
    })
    .join('\n')

  return `你是一个记忆系统的关系推理模块。请判断目标事件与每个邻居事件之间是否存在隐含关系。

## 目标事件
[${target.id}] ${targetText}
已知实体: ${target.entities.join(', ') || '无'}

## 邻居事件
${neighborList}

## 关系类型
- CAUSAL: LEADS_TO (目标导致邻居), BECAUSE_OF (目标由邻居导致), ENABLES, PREVENTS, RESPONSE_TO
- SEMANTIC: SIMILAR_TO (话题相似), RELATED_TO, PART_OF, CONTAINS
- ENTITY: MENTIONED_IN (目标提到邻居的实体), REFERS_TO (目标引用邻居)

## 任务
对每个邻居，判断是否存在上述关系。只返回置信度 ≥ 0.7 的关系。
用 JSON 数组格式返回（只返回 JSON，不要其他文字）：

[
  {
    "sourceId": "${target.id}",
    "targetId": "邻居ID",
    "type": "CAUSAL|SEMANTIC|ENTITY",
    "subtype": "具体子类型",
    "weight": 0.85
  }
]

如果没有任何高置信度关系，返回空数组 []。`
}
