# 记忆系统

Elio 的四维图记忆系统，基于事件节点 + 有向图 + 倒排索引。

## 架构概览

```
MemorySystem trait ← MainLoop 只依赖这个接口
    └── GraphMemorySystem (默认实现)
           │
           ├── FastPath  ─── <100ms, 纯规则, 无 LLM
           ├── SlowPath  ─── 每 30s, DeepSeek 推演
           ├── GraphStore ─── 内存属性图
           ├── InvertedIndex ── 关键词 → 事件 ID
           ├── DiskIO ─────── JSONL 持久化
           └── ContextBridge ── 唯一输出 → 注入提示词
```

## 数据模型

### EventNode（事件节点）

记忆的基本单位，代表一条发生的事情：

```rust
pub struct EventNode {
    id: String,              // 唯一 ID (e.g. "evt_1718000000_1234")
    text: String,            // 事件内容
    event_type: EventType,   // UserMessage / AssistantMessage / ToolUse / ...
    timestamp: i64,          // 毫秒级 Unix 时间戳
    keywords: Vec<String>,   // 提取的关键词
    session_id: Option<String>,
    entities: Vec<String>,   // 关联实体
    metadata: HashMap<String, String>,
}
```

### Edge（图边）

连接两个事件节点的有向边：

```rust
pub struct Edge {
    source: String,           // 源节点 ID
    target: String,           // 目标节点 ID
    relation: RelationType,   // Precedes / LeadsTo / BecauseOf / ...
    confidence: f64,          // 置信度 [0.0, 1.0]
    timestamp: i64,
    reason: Option<String>,   // 推理依据
}
```

### RelationType（关系类型）

| 关系 | 维度 | 含义 |
|------|------|------|
| `Precedes` | 时间 | A 在 B 之前发生 |
| `LeadsTo` | 因果 | A 导致 B |
| `BecauseOf` | 因果 | B 因为 A |
| `Enables` | 因果 | A 使 B 成为可能 |
| `Prevents` | 因果 | A 阻止了 B |
| `ResponseTo` | 因果 | A 是对 B 的回应 |
| `RelatedTo` | 语义 | A 与 B 相关 |
| `SimilarTo` | 语义 | A 与 B 相似 |
| `PartOf` | 语义 | A 是 B 的一部分 |
| `References` | 实体 | A 引用 B |

## 数据流

### 输入 → FastPath（<100ms）

每条消息到达时立即触发：

```
用户消息 / Elio 回复 / 工具结果
    │
    ▼
FastPath::process()
    │
    ├── 1. 提取关键词 (extract_keywords)
    │     去停用词（中英文），长词优先，上限 10 个
    │
    ├── 2. 创建 EventNode
    │     id = "evt_{timestamp}_{random}"
    │
    ├── 3. 加入 GraphStore
    │
    ├── 4. 建立时间边 (Precedes)
    │     指向最近的事件
    │
    ├── 5. 搜索倒排索引
    │     关键词匹配 → 建立 RelatedTo 边
    │
    └── 6. 更新 ContextBridge
           合成叙事摘要，供提示词注入
```

### 定时 → SlowPath（每 30s）

由心跳触发，DeepSeek 驱动：

```
心跳 tick (30s)
    │
    ▼
SlowPath::tick()
    │
    ├── 收集未处理事件（最多 10 个）
    │
    ├── 1. 叙事补全
    │     LLM 分析事件 → 提取实体 → 建立 References 边
    │
    ├── 2. 因果推断
    │     LLM 分析因果关系 → 建立 LeadsTo/BecauseOf 边
    │
    ├── 3. 置信度过滤
    │     只保留 confidence >= 0.7 的边
    │
    └── 4. 更新 ContextBridge
```

### 持久化

```
DiskIO (JSONL)
    │
    ├── events.jsonl     ← 事件追加写入
    ├── edges.jsonl      ← 边追加写入
    └── inverted_index.json  ← 索引全量写入
```

启动时 `load()` 恢复全部数据，每 30 秒 `save()` 快照。

## 4D 图遍历

从根节点出发，沿四个维度分别深度遍历：

```
TraversalDimension:
    Temporal ── Precedes 边
    Semantic ── RelatedTo / SimilarTo / PartOf 边
    Causal ──── LeadsTo / BecauseOf / Enables / Prevents / ResponseTo 边
    Entity ──── References 边
```

每个维度独立 DFS，最大 2 跳，返回路径及置信度乘积。

## 替换后端

`MemorySystem` trait 只有 7 个方法，可以轻松替换为向量数据库等后端：

```rust
#[async_trait]
pub trait MemorySystem: Send + Sync {
    fn record_event(&mut self, event: MemoryEvent);
    fn get_context(&self) -> String;
    async fn tick(&mut self);
    fn save(&self) -> Result<(), DiskError>;
    fn load(&mut self) -> Result<(), DiskError>;
    fn stats(&self) -> MemoryStats;
    fn clear(&mut self);
}
```

## 旧数据兼容

支持读取 TypeScript 版 Elio 的 `.elio/memory/` 数据：
- `rawText` / `text` 字段别名
- `eventType` / `sessionId` camelCase 兼容
- `sourceId` / `targetId` / `subtype` / `weight` 旧边格式
- 关系类型大小写不敏感
