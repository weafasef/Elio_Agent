"""
Elio 审计日志 GUI 预览工具

双击运行或在终端:
    python logview_gui.py                    # 查看当前目录日志
    python logview_gui.py today              # 只看今天的
    python logview_gui.py --dir D:/VS_python/Elio_Agent_v2/logs   # 指定日志目录

快捷键:
    Ctrl+F  聚焦搜索框
    Ctrl+R  刷新
    Ctrl+S  统计窗口
    Esc     清除过滤
"""

import json
import sys
import os
import tkinter as tk
from tkinter import ttk, messagebox
from datetime import datetime, timedelta
from pathlib import Path
from collections import Counter

# ── 路径 ──────────────────────────────────────────────

SCRIPT_DIR = Path(__file__).resolve().parent
LOGS_DIR = SCRIPT_DIR


# ── 事件类型颜色 (Rust 版 Elio) ────────────────────────
# (前景色, 背景色, 标签)

EVENT_COLORS = {
    # 用户
    "user.message":        ("#1565c0", "#e3f2fd", "用户"),
    "user.command":        ("#6a1b9a", "#f3e5f5", "命令"),
    # Elio 回复
    "elio.response":       ("#1a7a1a", "#e6ffe6", "Elio"),
    # 心跳 / 定时
    "system.heartbeat":    ("#999999", "#f0f0f0", "心跳"),
    # 系统
    "system.prompt":       ("#666666", "#f5f5f5", "提示词"),
    "system.error":        ("#b71c1c", "#ffebee", "错误"),
    "system.warning":      ("#f57f17", "#fff8e1", "警告"),
    "system.info":         ("#999999", "#f5f5f5", "系统"),
    # 记忆
    "memory.input":        ("#00838f", "#e0f7fa", "记入"),
    "memory.output":       ("#006064", "#e0f7fa", "记出"),
    # API
    "api.request":         ("#e65100", "#fff3e0", "API+"),
    "api.response":        ("#f57f17", "#fff8e1", "API-"),
    "api.error":           ("#b71c1c", "#ffebee", "APIx"),
    "api.usage":           ("#8e24aa", "#fce4ec", "Token"),
    # 工具
    "tool.invoke":         ("#0d47a1", "#e8eaf6", "工具+"),
    "tool.result":         ("#2e7d32", "#e8f5e9", "工具-"),
    "tool.error":          ("#b71c1c", "#ffebee", "工具x"),
}

DEFAULT_EVENT_COLOR = ("#333333", "#ffffff", "其他")


def get_event_style(evt_type: str):
    """获取事件类型的显示样式 (精确匹配 -> 前缀匹配 -> 默认)"""
    if evt_type in EVENT_COLORS:
        return EVENT_COLORS[evt_type]
    # 前缀匹配
    for prefix, color in EVENT_COLORS.items():
        if evt_type.startswith(prefix):
            return color
    return DEFAULT_EVENT_COLOR


# ── 内容解析 ──────────────────────────────────────────

def strip_think(text: str) -> str:
    """移除 <think>...</think> 块"""
    import re
    return re.sub(r'<think>.*?</think>', '', text, flags=re.DOTALL).strip()


def extract_en(text: str) -> str:
    """提取 <en>...</en> 内容"""
    import re
    m = re.search(r'<en>(.*?)</en>', text, re.DOTALL)
    return m.group(1).strip() if m else ""


def extract_zh(text: str) -> str:
    """提取 <zh>...</zh> 内容"""
    import re
    m = re.search(r'<zh>(.*?)</zh>', text, re.DOTALL)
    return m.group(1).strip() if m else ""


def has_think(text: str) -> bool:
    return '<think>' in text


def get_event_summary(evt: dict, max_len=100):
    """提取事件摘要用于列表显示 (优化 Rust 版格式)"""
    t = evt.get("type", "?")
    data = evt.get("data", "")

    # ── Rust 版 Elio 格式 (data 是字符串) ──
    if isinstance(data, str) and data:
        if t == "elio.response":
            en = extract_en(data)
            if en:
                prefix = "[思] " if has_think(data) else ""
                en_clean = en.replace("\n", " ").strip()
                if len(en_clean) > max_len:
                    en_clean = en_clean[:max_len] + "..."
                return prefix + en_clean
            # 没 <en> 标签，直接截取
            clean = strip_think(data).replace("\n", " ").strip()
            if len(clean) > max_len:
                clean = clean[:max_len] + "..."
            return clean

        if t == "system.heartbeat":
            return ""

        if t in ("memory.input", "memory.output"):
            text = data.replace("\n", " ")
            if len(text) > max_len:
                text = text[:max_len] + "..."
            return text

        if t in ("system.prompt", "user.message", "api.request", "api.response"):
            text = data.replace("\n", " ")
            if len(text) > max_len:
                text = text[:max_len] + "..."
            return text

        # 默认: 直接截取 data
        text = data.replace("\n", " ")
        if len(text) > max_len:
            text = text[:max_len] + "..."
        return text

    # ── 旧版格式 (payload dict) ──
    p = evt.get("payload", {})
    if isinstance(p, dict):
        if t == "user.message":
            content = p.get("content", "")
            prefix = "[cmd] " if p.get("isCommand", False) else ""
            return (prefix + content)[:max_len].replace("\n", " ")
        if t == "tool.invoke":
            name = p.get("toolName", "?")
            inp = p.get("input", {})
            if "command" in inp:
                return f"{name}: {inp['command'][:max_len]}"
            return name
        if t == "tool.result":
            name = p.get("toolName", "?")
            ok = "+" if p.get("success") else "-"
            return f"{name} {ok}  {p.get('durationMs',0)}ms"
        if t == "api.request":
            model = p.get("model", "?")
            return f"model={model}  messages={p.get('messageCount',0)}  tools={p.get('toolCount',0)}"
        if t == "api.response":
            content = p.get("content", "")
            if isinstance(content, str):
                return content[:max_len]
        if t == "api.usage":
            inp = p.get("inputTokens", 0)
            out = p.get("outputTokens", 0)
            return f"in={inp}  out={out}  cost=${p.get('cost',0):.4f}"
    return ""


def get_event_timestamp(evt: dict) -> str:
    return evt.get("timestamp", evt.get("eventTimestamp", ""))


# ── 日志加载 ──────────────────────────────────────────

def list_log_files(custom_dir=None):
    """列出所有 .jsonl 日志文件，按日期降序"""
    log_dir = Path(custom_dir) if custom_dir else LOGS_DIR
    if not log_dir.exists():
        return [], log_dir
    files = sorted(log_dir.glob("20*.jsonl"), reverse=True)
    return files, log_dir


def load_logs(filepath: Path):
    """加载 JSONL 文件"""
    events = []
    try:
        with open(filepath, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    evt = json.loads(line)
                    events.append(evt)
                except json.JSONDecodeError:
                    continue
    except FileNotFoundError:
        pass
    return events


# ── 详情格式化 ──────────────────────────────────────────

def format_detail(evt: dict) -> str:
    """格式化事件详情 (针对 Rust Elio 格式优化)"""
    t = evt.get("type", "?")
    ts = get_event_timestamp(evt)
    data = evt.get("data", "")
    source = evt.get("source", "")

    lines = [
        f"时间: {ts}",
        f"类型: {t}",
        f"来源: {source or '(无)'}",
        "─" * 50,
    ]

    # elio.response: 结构化显示 think / en / zh
    if t == "elio.response" and isinstance(data, str):
        think_blocks = []
        import re
        for m in re.finditer(r'<think>(.*?)</think>', data, re.DOTALL):
            think_blocks.append(m.group(1).strip())

        if think_blocks:
            lines.append("【思考】")
            for i, block in enumerate(think_blocks):
                lines.append(f"  [{i+1}] {block}")
                lines.append("")

        en = extract_en(data)
        if en:
            lines.append(f"【回复 en】{en}")
            lines.append("")

        zh = extract_zh(data)
        if zh:
            lines.append(f"【回复 zh】{zh}")
            lines.append("")

        # 如果都没有匹配到，显示原始内容
        if not think_blocks and not en and not zh:
            lines.append(data)
    elif isinstance(data, str):
        lines.append(data)
    elif isinstance(data, dict):
        lines.append(json.dumps(data, ensure_ascii=False, indent=2))

    return "\n".join(lines)


# ── GUI ───────────────────────────────────────────────

class LogViewerApp:
    def __init__(self, root, initial_date=None, custom_log_dir=None):
        self.root = root
        self.root.title("Elio 审计日志")
        self.root.geometry("1280x800")
        self.custom_log_dir = custom_log_dir
        self.current_log_dir = LOGS_DIR

        # ── 左侧: 文件列表 ──
        left_frame = ttk.Frame(root, width=280)
        left_frame.pack(side=tk.LEFT, fill=tk.Y, padx=(5, 0), pady=5)
        left_frame.pack_propagate(False)

        ttk.Label(left_frame, text="日志文件", font=("", 11, "bold")).pack(anchor=tk.W)

        btn_frame = ttk.Frame(left_frame)
        btn_frame.pack(fill=tk.X)
        ttk.Button(btn_frame, text="刷新 (Ctrl+R)", command=self.refresh_file_list).pack(side=tk.LEFT)
        self.dir_label = ttk.Label(left_frame, text="", foreground="gray")
        self.dir_label.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=5)

        self.file_listbox = tk.Listbox(left_frame, font=("Consolas", 10))
        self.file_listbox.pack(fill=tk.BOTH, expand=True, pady=(5, 0))
        self.file_listbox.bind("<<ListboxSelect>>", self.on_file_select)

        # 类型分布迷你面板
        type_frame = ttk.LabelFrame(left_frame, text="事件类型", padding=2)
        type_frame.pack(fill=tk.X, pady=(5, 0))
        self.type_tree = ttk.Treeview(type_frame, height=8, columns=("cnt",), show="tree headings")
        self.type_tree.heading("#0", text="类型")
        self.type_tree.heading("cnt", text="数")
        self.type_tree.column("cnt", width=40, anchor=tk.E)
        self.type_tree.pack(fill=tk.BOTH, expand=True)
        self.type_tree.bind("<<TreeviewSelect>>", self.on_type_filter)

        ttk.Button(type_frame, text="统计 (Ctrl+S)", command=self.open_stats).pack(fill=tk.X, pady=2)

        # ── 中间+右侧: 可拖拽分割面板 ──
        main_panes = ttk.PanedWindow(root, orient=tk.HORIZONTAL)
        main_panes.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=5, pady=5)

        # ── 中间: 事件列表 ──
        mid_frame = ttk.Frame(main_panes)
        main_panes.add(mid_frame, weight=3)

        # 过滤栏
        self.filter_frame = ttk.Frame(mid_frame)
        self.filter_frame.pack(fill=tk.X)

        ttk.Label(self.filter_frame, text="搜索:").pack(side=tk.LEFT)
        self.search_var = tk.StringVar()
        self.search_entry = ttk.Entry(self.filter_frame, textvariable=self.search_var, width=22)
        self.search_entry.pack(side=tk.LEFT, padx=5)
        self.search_entry.bind("<KeyRelease>", lambda e: self.refresh_event_list())

        ttk.Label(self.filter_frame, text="类型:").pack(side=tk.LEFT, padx=(10, 0))
        self.filter_var = tk.StringVar(value="全部")
        self.filter_combo = ttk.Combobox(self.filter_frame, textvariable=self.filter_var, width=18)
        self.filter_combo.pack(side=tk.LEFT, padx=5)
        self.filter_combo.bind("<<ComboboxSelected>>", lambda e: self.refresh_event_list())

        ttk.Label(self.filter_frame, text="日期:").pack(side=tk.LEFT, padx=(10, 0))
        self.date_var = tk.StringVar()
        self.date_entry = ttk.Entry(self.filter_frame, textvariable=self.date_var, width=10)
        self.date_entry.pack(side=tk.LEFT, padx=5)
        self.date_entry.bind("<KeyRelease>", lambda e: self.refresh_event_list())

        self.stats_label = ttk.Label(self.filter_frame, text="", foreground="gray")
        self.stats_label.pack(side=tk.RIGHT, padx=5)

        # 事件列表
        self.event_listbox = tk.Listbox(mid_frame, font=("Consolas", 10), exportselection=False)
        self.event_listbox.pack(fill=tk.BOTH, expand=True)
        self.event_listbox.bind("<<ListboxSelect>>", self.on_event_select)

        # ── 右侧: 详情 ──
        right_frame = ttk.Frame(main_panes)
        main_panes.add(right_frame, weight=5)

        detail_header = ttk.Frame(right_frame)
        detail_header.pack(fill=tk.X)
        ttk.Label(detail_header, text="事件详情", font=("", 11, "bold")).pack(side=tk.LEFT)
        ttk.Button(detail_header, text="复制", command=self.copy_detail).pack(side=tk.RIGHT)

        self.detail_text = tk.Text(right_frame, font=("Consolas", 10), wrap=tk.WORD)
        self.detail_text.pack(fill=tk.BOTH, expand=True)

        # ── 状态栏 ──
        self.status_var = tk.StringVar(value="就绪")
        self.status_bar = ttk.Label(root, textvariable=self.status_var, relief=tk.SUNKEN, anchor=tk.W)
        self.status_bar.pack(side=tk.BOTTOM, fill=tk.X)

        # ── 数据 ──
        self.all_events = []
        self.current_events = []
        self.current_file = None

        # ── 键盘快捷键 ──
        root.bind("<Control-f>", lambda _e: self.search_entry.focus_set())
        root.bind("<Control-r>", lambda _e: self.refresh_file_list())
        root.bind("<Control-s>", lambda _e: self.open_stats())
        root.bind("<Escape>", lambda _e: self.clear_filters())

        # ── 初始化 ──
        if initial_date:
            self.date_var.set(initial_date)
        self.refresh_file_list()

    # ───────────── 方法 ─────────────

    def clear_filters(self):
        """清除所有过滤条件"""
        self.search_var.set("")
        self.filter_var.set("全部")
        self.date_var.set("")
        self.refresh_event_list()

    def refresh_file_list(self):
        """刷新左侧文件列表"""
        self.file_listbox.delete(0, tk.END)
        files, log_dir = list_log_files(self.custom_log_dir)
        self.current_log_dir = log_dir
        self.dir_label.config(text=log_dir.name if log_dir.exists() else str(log_dir))
        self.file_paths = files
        if not files:
            self.file_listbox.insert(tk.END, "(无日志文件)")
            self.status_var.set(f"日志目录: {log_dir} — 无日志文件")
            return
        for f in files:
            size = f.stat().st_size
            modified = datetime.fromtimestamp(f.stat().st_mtime).strftime("%m-%d %H:%M")
            label = f"{f.stem}  ({size//1024}KB  {modified})"
            self.file_listbox.insert(tk.END, label)
        self.file_listbox.selection_set(0)
        self.on_file_select()

    def on_file_select(self, event=None):
        """选中文件 → 加载事件"""
        sel = self.file_listbox.curselection()
        if not sel:
            return
        idx = sel[0]
        if idx >= len(self.file_paths):
            return
        filepath = self.file_paths[idx]
        self.current_file = filepath
        self.all_events = load_logs(filepath)
        self._populate_filters()
        self.refresh_event_list()

    def _populate_filters(self):
        """从已加载事件中提取类型列表，更新下拉框和类型树"""
        type_counter = Counter(e.get("type", "?") for e in self.all_events)

        # 过滤下拉框
        types = ["全部"] + [t for t, _ in type_counter.most_common()]
        self.filter_combo["values"] = types

        # 类型树
        self.type_tree.delete(*self.type_tree.get_children())
        for evt_type, cnt in type_counter.most_common():
            self.type_tree.insert("", tk.END, text=evt_type, values=(cnt,))

    def on_type_filter(self, event):
        """点击类型树 → 过滤该类型"""
        sel = self.type_tree.selection()
        if sel:
            evt_type = self.type_tree.item(sel[0], "text")
            self.filter_var.set(evt_type)
            self.refresh_event_list()

    def refresh_event_list(self):
        """刷新事件列表（应用过滤）"""
        self.event_listbox.delete(0, tk.END)
        filter_type = self.filter_var.get()
        search_text = self.search_var.get().lower()
        date_filter = self.date_var.get().strip()

        self.current_events = []
        for evt in self.all_events:
            t = evt.get("type", "?")
            ts = get_event_timestamp(evt)

            # 类型过滤
            if filter_type != "全部" and t != filter_type:
                continue

            # 日期过滤
            if date_filter and date_filter not in ts:
                continue

            # 搜索 (类型 + 内容)
            if search_text:
                summary = get_event_summary(evt).lower()
                data = (evt.get("data", "") if isinstance(evt.get("data"), str) else "").lower()
                if search_text not in t.lower() and search_text not in summary and search_text not in data:
                    continue

            self.current_events.append(evt)

            # 显示
            fg, bg, tag = get_event_style(t)
            summary = get_event_summary(evt)
            summary_clean = summary.replace("\n", " ").strip()
            if len(summary_clean) > 120:
                summary_clean = summary_clean[:120] + "..."

            # 时间 HH:MM:SS + 标签 + 摘要
            time_str = ts[11:19] if len(ts) >= 19 else ts
            display = f"{time_str}  {tag:6s}  {summary_clean}"
            self.event_listbox.insert(tk.END, display)
            idx = self.event_listbox.size() - 1
            self.event_listbox.itemconfig(idx, fg=fg, bg=bg)

        # 状态栏
        visible = len(self.current_events)
        total = len(self.all_events)
        stats = f"显示 {visible} / 共 {total} 事件"
        if self.current_file:
            stats += f"  |  {self.current_file.name}"
        self.stats_label.config(text=stats)
        self.status_var.set(stats)

    def on_event_select(self, event=None):
        """选中事件 → 显示详情"""
        sel = self.event_listbox.curselection()
        if not sel:
            return
        idx = sel[0]
        if idx >= len(self.current_events):
            return
        evt = self.current_events[idx]
        self.detail_text.delete("1.0", tk.END)
        formatted = format_detail(evt)
        self.detail_text.insert("1.0", formatted)

    def copy_detail(self):
        """复制详情到剪贴板"""
        content = self.detail_text.get("1.0", tk.END).strip()
        if content:
            self.root.clipboard_clear()
            self.root.clipboard_append(content)
            self.status_var.set("已复制到剪贴板")
        else:
            self.status_var.set("没有内容可复制")

    def open_stats(self):
        """打开统计窗口"""
        if self.all_events:
            StatsWindow(self.root, self.all_events, self.current_log_dir)


# ── 统计窗口 ──────────────────────────────────────────

class StatsWindow:
    def __init__(self, parent, events, log_dir):
        self.win = tk.Toplevel(parent)
        self.win.title("事件统计")
        self.win.geometry("900x600")

        toolbar = ttk.Frame(self.win)
        toolbar.pack(fill=tk.X, padx=5, pady=5)
        ttk.Label(toolbar, text=f"日志目录: {log_dir}", foreground="gray").pack(side=tk.LEFT)
        ttk.Button(toolbar, text="导出...", command=self.export).pack(side=tk.RIGHT)

        panes = ttk.PanedWindow(self.win, orient=tk.HORIZONTAL)
        panes.pack(fill=tk.BOTH, expand=True, padx=5, pady=5)

        # 左侧: 类型统计 + 时间线
        left_frame = ttk.Frame(panes)
        panes.add(left_frame, weight=1)

        # 类型分布
        ttk.Label(left_frame, text="类型分布", font=("", 10, "bold")).pack(anchor=tk.W)
        self.tree = ttk.Treeview(left_frame, columns=("count", "prop"), show="tree headings")
        self.tree.heading("#0", text="事件类型")
        self.tree.heading("count", text="次数")
        self.tree.heading("prop", text="占比")
        self.tree.column("count", width=80, anchor=tk.E)
        self.tree.column("prop", width=80, anchor=tk.E)
        self.tree.pack(fill=tk.BOTH, expand=True)

        type_counter = Counter(e.get("type", "?") for e in events)
        total = len(events)
        for t, cnt in type_counter.most_common():
            prop = f"{cnt/total*100:.1f}%"
            self.tree.insert("", tk.END, text=t, values=(cnt, prop))

        # 小时分布
        ttk.Label(left_frame, text="\n小时分布", font=("", 10, "bold")).pack(anchor=tk.W)
        hour_frame = ttk.Frame(left_frame)
        hour_frame.pack(fill=tk.X)
        hour_counter = Counter()
        for e in events:
            ts = get_event_timestamp(e)
            if len(ts) >= 13:
                hour_counter[ts[11:13]] += 1
        hour_text = "  ".join(f"{h}h:{c}" for h, c in sorted(hour_counter.items()))
        ttk.Label(hour_frame, text=hour_text, font=("Consolas", 9), foreground="gray",
                  wraplength=350).pack(anchor=tk.W, pady=2)

        # 右侧: 选中类型的详情
        right_frame = ttk.Frame(panes)
        panes.add(right_frame, weight=1)

        ttk.Label(right_frame, text="事件详情", font=("", 10, "bold")).pack(anchor=tk.W)
        self.detail_text = tk.Text(right_frame, font=("Consolas", 9), wrap=tk.WORD)
        self.detail_text.pack(fill=tk.BOTH, expand=True)

        self.tree.bind("<<TreeviewSelect>>", self.on_type_select)
        self.events = events
        self.log_dir = log_dir
        self.total = total

    def on_type_select(self, event):
        sel = self.tree.selection()
        if not sel:
            return
        evt_type = self.tree.item(sel[0], "text")
        self.detail_text.delete("1.0", tk.END)
        lines = []
        for evt in self.events:
            if evt.get("type") == evt_type:
                ts = get_event_timestamp(evt)
                summary = get_event_summary(evt, 80)
                time_str = ts[11:19] if len(ts) >= 19 else ts
                lines.append(f"[{time_str}] {summary}")
        self.detail_text.insert("1.0", "\n".join(lines))

    def export(self):
        from tkinter import filedialog
        path = filedialog.asksaveasfilename(defaultextension=".json",
            filetypes=[("JSON", "*.json")], title="导出统计")
        if not path:
            return
        type_counter = Counter(e.get("type", "?") for e in self.events)
        with open(path, "w", encoding="utf-8") as f:
            json.dump({
                "logDir": str(self.log_dir),
                "total": len(self.events),
                "types": dict(type_counter.most_common()),
            }, f, ensure_ascii=False, indent=2)
        messagebox.showinfo("导出完成", f"已导出 {len(type_counter)} 个类型到\n{path}")


# ── 主入口 ────────────────────────────────────────────

def main():
    root = tk.Tk()

    initial_date = None
    custom_log_dir = None
    i = 1
    while i < len(sys.argv):
        arg = sys.argv[i]
        if arg == "--dir" and i + 1 < len(sys.argv):
            custom_log_dir = sys.argv[i + 1]
            i += 2
        elif arg == "today":
            initial_date = datetime.now().strftime("%Y-%m-%d")
            i += 1
        elif arg == "yesterday":
            initial_date = (datetime.now() - timedelta(days=1)).strftime("%Y-%m-%d")
            i += 1
        elif arg.startswith("20"):
            initial_date = arg
            i += 1
        else:
            i += 1

    app = LogViewerApp(root, initial_date, custom_log_dir)
    root.mainloop()


if __name__ == "__main__":
    main()
