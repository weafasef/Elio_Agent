# cc-haha 项目笔记

## 启动方式

### 1. 环境要求
- **Bun** >= 1.3.14（`npm install -g bun`）
- **Python** >= 3.10（LiteLLM 代理需要）
- **pip** 已安装 `litellm[proxy]`

### 2. 配置 DeepSeek

编辑 `.env`：
```bash
ANTHROPIC_AUTH_TOKEN=sk-local-proxy
ANTHROPIC_BASE_URL=http://localhost:4000
ANTHROPIC_MODEL=deepseek-v4-flash
```

编辑 `litellm_config.yaml`：
```yaml
model_list:
  - model_name: deepseek-v4-flash
    litellm_params:
      model: openai/deepseek-v4-flash
      api_key: <你的 DeepSeek API Key>
      api_base: https://api.deepseek.com/v1
```

### 3. 运行

```bash
# 交互模式
./bin/claude-haha

# 单次问答
./bin/claude-haha -p "你的问题"
```

LiteLLM 代理会自动启动/停止，无需手动操作。

---

## 记忆系统详解

## 目录结构

```
~/.claude/projects/<sanitized-git-root>/memory/
├── MEMORY.md          # 索引文件（每行一个链接，最多200行/25KB）
├── <topic>.md         # 具体记忆文件（Markdown + YAML frontmatter）
└── team/              # 团队记忆（需单独开启）
    ├── MEMORY.md
    └── ...
```

物理路径由 `CLAUDE_CODE_REMOTE_MEMORY_DIR` 环境变量 > `autoMemoryDirectory` 配置 > `~/.claude` 决定。

核心代码在 `src/memdir/` 目录下。

---

## 记忆类型（4种）

| 类型 | 用途 | 示例 |
|------|------|------|
| `user` | 用户身份、角色、偏好 | "用户是前端工程师，喜欢函数式风格" |
| `feedback` | 用户反馈：该做什么/避免什么 | "用户讨厌过度注释" |
| `project` | 项目上下文（代码/git无法推导的） | "下周一要上线，别改核心模块" |
| `reference` | 外部资源指针 | "Linear 项目地址：https://..." |

每条记忆文件以 YAML frontmatter 开头：

```markdown
---
name: <kebab-case-slug>
description: <一行摘要，用于判断相关性>
type: user | feedback | project | reference
---
<记忆内容>
```

**反模式（不应该保存的内容）：**
- 代码模式、架构、文件路径（代码里能推导的）
- Git 历史、最近改动（git log 是权威来源）
- 调试方案、修复方法（修复已经在代码里了）
- 临时任务、进行中的工作
- CLAUDE.md 已有的内容

---

## 记忆生命周期

### 1. 创建（3条路径）

| 路径 | 触发时机 | 方式 |
|------|---------|------|
| **主 Agent 直接写** | 对话中模型决定保存 | 直接调 Write/Edit 工具写 memory 目录 |
| **后台提取** | 每次对话回合结束 | Fork 一个子 Agent，分析最近对话，提取记忆 |
| **手动 /remember** | 用户主动调用 | 审查所有记忆层，提出清理/升级建议 |

后台提取（`src/services/extractMemories/`）：
- 在每次对话回合结束时触发（`stopHooks.ts`）
- 用 Forked Agent（共享主 Agent 的 prompt cache，省 token）
- 限制 5 回合、工具权限仅限于 memory 目录
- 如果主 Agent 已经写了记忆文件，本次提取自动跳过
- 提取失败时消息不会被标记为"已处理"，下次重试

### 2. 加载/召回

**始终加载：** `MEMORY.md` 索引文件在会话开始时连同 CLAUDE.md 一起注入系统提示词和用户上下文（`src/constants/prompts.ts` line 495 → `loadMemoryPrompt()` → `buildMemoryLines()`）。

**相关性召回**（`src/memdir/findRelevantMemories.ts`）：
- 收到用户消息后，扫描所有记忆文件的 frontmatter
- 调 Sonnet 做语义相关性排序，选出最多 5 条
- 排除最近用过的工具文档（正在用的文档不需要召回）
- 附带上 `mtime` 供模型判断记忆是否过时

### 3. 整合/AutoDream（做梦）

24小时 + 5次会话后自动触发（`src/services/autoDream/`）：

```
时间门(24h) → 扫描节流(10min) → 会话门(5次) → 文件锁(PID) → 执行
```

执行过程分4个阶段：
1. **Orient** — 浏览记忆目录，读 MEMORY.md
2. **Gather** — 收集近期信号（日志 → 记忆漂移 → 会话记录搜索）
3. **Consolidate** — 合并到主题文件，相对日期转绝对日期，删除矛盾事实
4. **Prune** — 更新 MEMORY.md，移除过时条目，强制执行行数/字节数上限

### 4. 清理

- 用户明确说"忘掉XX" → 模型删除对应文件
- 召回时发现记忆与当前代码状态冲突 → 信任代码，更新/删除记忆
- AutoDream 第3-4阶段自动检测并解决矛盾

---

## 关键源文件

| 文件 | 作用 |
|------|------|
| `src/memdir/memoryTypes.ts` | 记忆类型定义、系统提示词模板 |
| `src/memdir/memdir.ts` | 核心提示词构建、MEMORY.md 管理 |
| `src/memdir/paths.ts` | 路径解析、自动记忆开关 |
| `src/memdir/memoryScan.ts` | 扫描 .md 文件、解析 frontmatter |
| `src/memdir/findRelevantMemories.ts` | 相关性召回（Sonnet side-query） |
| `src/memdir/memoryAge.ts` | 记忆年龄计算、过期警告 |
| `src/services/extractMemories/` | 后台记忆提取 |
| `src/services/autoDream/` | 记忆整合/做梦 |
| `src/constants/prompts.ts` | 记忆提示词注入系统提示词的位置（line 495） |
| `src/query/stopHooks.ts` | 每回合结束后触发提取和做梦检查 |
| `src/utils/backgroundHousekeeping.ts` | 初始化提取/做梦/会话记忆 |

---

## 给 Elio 改造的要点

1. **路径全局化**：当前按项目隔离 → Elio 需要 `~/.elio/memory/` 全局记忆，不再绑定 git 仓库
2. **记忆类型扩展**：增加 `emotional`（情感记忆）、`relationship`（关系追踪）、`self_reflection`（自我反思）
3. **取消"不能保存代码相关"的限制**：当前反模式限制对 Elio 不适用，她需要记住关于用户的一切（包括技术偏好）
4. **提取提示词重写**：当前聚焦编程项目 → 改为聚焦人际关系、情感信号、用户偏好
5. **AutoDream → Elio Dreams**：从"整合编程记忆"变为"自我反思周期"，包括日常反思、深度做梦、人格校准
6. **MEMORY.md 索引保留**：这个单文件索引机制简单有效，Elio 可以复用
7. **Forked Agent 模式保留**：共享缓存 + 权限限制的子 Agent 是很好的设计，Elio 的反思/做梦可以复用
