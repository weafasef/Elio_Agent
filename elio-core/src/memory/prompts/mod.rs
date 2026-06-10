//! LLM 提示词模板 — 记忆系统使用的提示词

/// 构建叙事补全提示词
pub fn build_narrative_prompt<'a>(
    events: impl IntoIterator<Item = &'a NarrativeContextEvent<'a>>,
) -> String {
    let events_str: Vec<String> = events
        .into_iter()
        .map(|e| format!("[{:?}] {}", e.event_type, e.text))
        .collect();

    format!(
        r#"你是一个记忆叙事助手。请根据以下时间线中的事件，撰写一段简洁的叙事总结（中文，100字以内）。
要求：
1. 提取关键实体（人物、地点、事物）
2. 识别事件之间的关联
3. 保持客观，不要想象未发生的事

事件时间线：
{events}

请以 JSON 格式输出：
{{"narrative": "叙事文本", "entities": ["实体1", "实体2"]}}"#,
        events = events_str.join("\n")
    )
}

/// 因果推断提示词
pub fn build_causality_prompt<'a>(
    target: &str,
    context: impl IntoIterator<Item = &'a CausalityContextEvent<'a>>,
) -> String {
    let ctx_str: Vec<String> = context
        .into_iter()
        .map(|e| format!("[{:?}] {}\n  → {:.100}", e.event_type, e.id, e.text))
        .collect();

    format!(
        r#"分析事件之间是否存在因果关系。只考虑明显、可验证的因果关系。

目标事件: {target}

上下文事件:
{ctx}

请以 JSON 格式输出，如果不存在因果关系则输出 {{"edges": []}}：
{{"edges": [
  {{"target": "事件ID", "relation": "leads_to|because_of|enables|prevents|response_to", "confidence": 0.0-1.0, "reason": "简要原因"}}
]}}"#,
        target = target,
        ctx = ctx_str.join("\n---\n")
    )
}

/// 实体提取提示词
pub fn build_entity_prompt(text: &str) -> String {
    format!(
        r#"从以下文本中提取关键实体（人物、地点、事物、概念）。
每个实体用简短名称表示（2-10个字）。

文本: {text}

以 JSON 格式输出：
{{"entities": ["实体1", "实体2"]}}"#
    )
}

/// 叙事上下文事件
pub struct NarrativeContextEvent<'a> {
    pub event_type: &'a str,
    pub text: &'a str,
}

/// 因果上下文事件
pub struct CausalityContextEvent<'a> {
    pub id: &'a str,
    pub event_type: &'a str,
    pub text: &'a str,
}
