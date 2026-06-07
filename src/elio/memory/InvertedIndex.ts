import type { GraphStore } from './GraphStore.js'

// ── Stop Words ───────────────────────────────────────────────────────────

const STOP_WORDS = new Set([
  // Chinese
  '的', '了', '在', '是', '我', '有', '和', '就', '不', '人', '都', '一',
  '一个', '上', '也', '很', '到', '说', '要', '去', '你', '会', '着',
  '没有', '看', '好', '自己', '这', '他', '她', '它', '们', '那', '些',
  '什么', '怎么', '哪', '吗', '吧', '呢', '啊', '哦', '嗯', '哈',
  '但', '而', '或', '与', '及', '之', '其', '从', '被', '把',
  '对', '向', '以', '为', '所', '者', '个', '中', '可以',
  '已经', '还', '又', '才', '刚', '正', '在', '再', '就',
  '这个', '那个', '哪个', '这里', '那里', '哪里', '怎么',
  '只是', '因为', '所以', '如果', '虽然', '但是', '然后',
  '的话', '时候', '觉得', '应该', '可能', '已经', '没有',
  '没', '太', '真', '挺', '比较', '非常', '更', '最',
  '来', '去', '做', '搞', '弄', '让', '给', '拿', '找',
  // English
  'the', 'a', 'an', 'is', 'are', 'was', 'were', 'be', 'been',
  'being', 'have', 'has', 'had', 'do', 'does', 'did', 'will',
  'would', 'could', 'should', 'may', 'might', 'shall', 'can',
  'to', 'of', 'in', 'for', 'on', 'with', 'at', 'by', 'from',
  'it', 'its', 'this', 'that', 'these', 'those', 'i', 'me',
  'my', 'we', 'our', 'you', 'your', 'he', 'she', 'they',
  'and', 'but', 'or', 'not', 'no', 'if', 'so', 'as', 'than',
  'then', 'just', 'also', 'very', 'too', 'only', 'now',
  'here', 'there', 'what', 'which', 'who', 'how', 'when',
  'where', 'all', 'both', 'each', 'every', 'some', 'any',
])

// ── Helpers ──────────────────────────────────────────────────────────────

const CJK_RE = /[一-鿿㐀-䶿]+/g
const ALPHA_RE = /[a-zA-Z0-9_]{2,}/g

function tokenize(text: string): string[] {
  const tokens: string[] = []

  // Extract CJK character sequences
  const cjkMatches = text.match(CJK_RE)
  if (cjkMatches) {
    for (const seq of cjkMatches) {
      if (seq.length <= 3) {
        // Short sequences are already good keywords (e.g. "支付", "同步回调")
        tokens.push(seq)
      } else {
        // Long sequences: push whole + bigrams for compound discovery
        tokens.push(seq)
        for (let i = 0; i < seq.length - 1; i++) {
          tokens.push(seq.slice(i, i + 2))
        }
      }
    }
  }

  // Extract alphabetic tokens (2+ chars)
  const alphaMatches = text.match(ALPHA_RE)
  if (alphaMatches) {
    for (const token of alphaMatches) {
      tokens.push(token.toLowerCase())
    }
  }

  return tokens
}

function filterStopWords(tokens: string[]): string[] {
  return tokens.filter(t => {
    if (t.length < 2) return false
    if (STOP_WORDS.has(t)) return false
    if (STOP_WORDS.has(t.toLowerCase())) return false
    return true
  })
}

/**
 * Extract meaningful keywords from raw text.
 * Simple: tokenize → remove stop words → deduplicate.
 * No LLM needed.
 */
export function extractKeywords(text: string): string[] {
  const tokens = tokenize(text)
  const filtered = filterStopWords(tokens)
  return [...new Set(filtered)]
}

// ── InvertedIndex ────────────────────────────────────────────────────────

export class InvertedIndex {
  private index: Map<string, Set<string>> = new Map()

  /** Index an event by its text content. */
  add(eventId: string, text: string): void {
    const keywords = extractKeywords(text)
    for (const kw of keywords) {
      if (!this.index.has(kw)) {
        this.index.set(kw, new Set())
      }
      this.index.get(kw)!.add(eventId)
    }
  }

  /** Remove an event from the index. */
  remove(eventId: string): void {
    for (const eventIds of this.index.values()) {
      eventIds.delete(eventId)
    }
  }

  /** Update an event's indexed text (remove old keywords, add new ones). */
  update(eventId: string, newText: string): void {
    this.remove(eventId)
    this.add(eventId, newText)
  }

  /**
   * Search for events matching the given query text.
   * Returns event IDs ranked by hit count (descending).
   */
  search(queryText: string, topK: number = 10): string[] {
    const queryKeywords = extractKeywords(queryText)
    if (queryKeywords.length === 0) return []

    const hitMap = new Map<string, number>()

    for (const kw of queryKeywords) {
      const eventIds = this.index.get(kw)
      if (!eventIds) continue
      for (const id of eventIds) {
        hitMap.set(id, (hitMap.get(id) ?? 0) + 1)
      }
    }

    return Array.from(hitMap.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, topK)
      .map(([id]) => id)
  }

  /** Rebuild the entire index from a GraphStore. */
  rebuild(graph: GraphStore): void {
    this.index.clear()
    for (const event of graph.getAllEvents()) {
      this.add(event.id, event.rawText)
    }
  }

  get size(): number {
    return this.index.size
  }

  getKeywordCount(): number {
    return this.index.size
  }

  /** Get all indexed keywords. */
  getKeywords(): string[] {
    return Array.from(this.index.keys())
  }

  /** Get events containing a specific keyword. */
  getEventsForKeyword(keyword: string): string[] {
    const ids = this.index.get(keyword)
    return ids ? Array.from(ids) : []
  }

  toJSON(): Record<string, string[]> {
    const obj: Record<string, string[]> = {}
    for (const [kw, ids] of this.index) {
      obj[kw] = Array.from(ids)
    }
    return obj
  }

  static fromJSON(data: Record<string, string[]>): InvertedIndex {
    const idx = new InvertedIndex()
    for (const [kw, ids] of Object.entries(data)) {
      idx.index.set(kw, new Set(ids))
    }
    return idx
  }
}
