"""
Elio 审计日志 GUI 预览工具

双击运行或在终端:
    python logview_gui.py                    # 查看当前目录日志
    python logview_gui.py today              # 只看今天的
    python logview_gui.py --dir D:/VS_python/Elio_Agent_v2/logs   # 指定日志目录
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


# ── 事件类型颜色 ──────────────────────────────────────

# tkinter 颜色方案: (前景色, 背景色, 标签)
EVENT_COLORS = {
    "session.start":       ("#1a7a1a", "#e6ffe6", ">> 会话开始"),
    "session.end":         ("#666666", "#f0f0f0", "<< 会话结束"),
    "session.idle":        ("#999999", "#f5f5f5", ".. 空闲"),
    "user.message":        ("#1565c0", "#e3f2fd", ":: 用户消息"),
    "user.command":        ("#6a1b9a", "#f3e5f5", "## 用户命令"),
    "context.system_prompt": ("#666666", "#f5f5f5", ">> 系统提示"),
    "context.memory_loaded": ("#00838f", "#e0f7fa", "!! 记忆加载"),
    "context.claude_md":   ("#666666", "#f5f5f5", ".. CLAUDE.md"),
    "context.git_status":  ("#666666", "#f5f5f5", ".. Git 状态"),
    "context.environment": ("#666666", "#f5f5f5", ".. 环境信息"),
    "api.request":         ("#e65100", "#fff3e0", "^^ API 请求"),
    "api.thinking":        ("#00838f", "#e0f7fa", "** 思考"),
    "api.response":        ("#f57f17", "#fff8e1", "vv API 响应"),
    "api.stream_chunk":    ("#999999", "#f5f5f5", ".. 流块"),
    "api.error":           ("#b71c1c", "#ffebee", "XX API 错误"),
    "api.usage":           ("#8e24aa", "#fce4ec", "$$ Token"),
    "tool.invoke":         ("#0d47a1", "#e8eaf6", ">> 工具调用"),
    "tool.result":         ("#2e7d32", "#e8f5e9", "<< 工具结果"),
    "tool.error":          ("#b71c1c", "#ffebee", "XX 工具错误"),
    "memory.recall":       ("#00838f", "#e0f7fa", ".. 记忆回想"),
    "memory.save":         ("#00838f", "#e0f7fa", ".. 记忆保存"),
    "memory.consolidate":  ("#00838f", "#e0f7fa", ".. 记忆整合"),
    "emotion.snapshot":    ("#ad1457", "#fce4ec", "!! 情感快照"),
    "emotion.trigger":     ("#ad1457", "#fce4ec", "!! 情感触发"),
    "dream.start":         ("#6a1b9a", "#f3e5f5", "** 梦境开始"),
    "dream.end":           ("#6a1b9a", "#f3e5f5", "** 梦境结束"),
    "dream.insight":       ("#6a1b9a", "#f3e5f5", "** 梦境洞察"),
    "system.error":        ("#b71c1c", "#ffebee", "XX 系统错误"),
    "system.warning":      ("#f57f17", "#fff8e1", "!! 系统警告"),
    "system.info":         ("#999999", "#f5f5f5", ".. 系统信息"),
    # Rust 版 Elio 事件类型
    "system.prompt":       ("#666666", "#f5f5f5", ">> 系统提示"),
    "elio.response":       ("#1a7a1a", "#e6ffe6", "<< Elio 回复"),
    "system.heartbeat":    ("#999999", "#f0f0f0", ".. 心跳"),
}

DEFAULT_EVENT_COLOR = ("#333333", "#ffffff", ".. 其他")


def get_event_style(evt_type: str):
    """获取事件类型的显示样式"""
    if evt_type in EVENT_COLORS:
        return EVENT_COLORS[evt_type]
    # 按前缀匹配
    for prefix, color in EVENT_COLORS.items():
        if evt_type.startswith(prefix):
            return color
    return DEFAULT_EVENT_COLOR


# ── 日志加载 ──────────────────────────────────────────

def list_log_files(custom_dir=None):
    """列出所有 .jsonl 日志文件，按日期降序"""
    if custom_dir:
        log_dir = Path(custom_dir)
    else:
        log_dir = LOGS_DIR
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


def get_event_summary(evt: dict, max_len=80):
    """提取事件摘要用于列表显示"""
    t = evt.get("type", "?")
    p = evt.get("payload", evt)  # Rust 版用 data 字段，旧版用 payload

    # Rust 版新格式（log.rs）
    data = evt.get("data", p)
    if isinstance(data, str) and data:
        text = data.replace("\n", " ")[:max_len]
        return text

    # 旧版格式
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


# ── GUI ───────────────────────────────────────────────

class LogViewerApp:
    def __init__(self, root, initial_date=None, custom_log_dir=None):
        self.root = root
        self.root.title("Elio 审计日志")
        self.root.geometry("1280x800")
        self.custom_log_dir = custom_log_dir
        self.current_log_dir = LOGS_DIR

        # ── 左侧日志列表 ──
        left_frame = ttk.Frame(root, width=300)
        left_frame.pack(side=tk.LEFT, fill=tk.Y, padx=(5, 0), pady=5)

        ttk.Label(left_frame, text="日志文件", font=("", 11, "bold")).pack(anchor=tk.W)

        # 刷新按钮
        btn_frame = ttk.Frame(left_frame)
        btn_frame.pack(fill=tk.X)
        ttk.Button(btn_frame, text="刷新", command=self.refresh_file_list, width=8).pack(side=tk.LEFT)
        ttk.Label(btn_frame, text="", width=2).pack(side=tk.LEFT)
        self.dir_label = ttk.Label(btn_frame, text="", foreground="gray")
        self.dir_label.pack(side=tk.LEFT, fill=tk.X, expand=True)

        self.file_listbox = tk.Listbox(left_frame, width=40, height=20, font=("Consolas", 10))
        self.file_listbox.pack(fill=tk.BOTH, expand=True)
        self.file_listbox.bind("<<ListboxSelect>>", self.on_file_select)

        # ── 中间事件列表 ──
        mid_frame = ttk.Frame(root)
        mid_frame.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=5, pady=5)

        self.filter_frame = ttk.Frame(mid_frame)
        self.filter_frame.pack(fill=tk.X)

        ttk.Label(self.filter_frame, text="过滤类型:").pack(side=tk.LEFT)
        self.filter_var = tk.StringVar(value="全部")
        self.filter_combo = ttk.Combobox(self.filter_frame, textvariable=self.filter_var, width=25)
        self.filter_combo.pack(side=tk.LEFT, padx=5)
        self.filter_combo.bind("<<ComboboxSelected>>", lambda e: self.refresh_event_list())

        ttk.Label(self.filter_frame, text="搜索:").pack(side=tk.LEFT, padx=(10, 0))
        self.search_var = tk.StringVar()
        self.search_entry = ttk.Entry(self.filter_frame, textvariable=self.search_var, width=20)
        self.search_entry.pack(side=tk.LEFT, padx=5)
        self.search_entry.bind("<KeyRelease>", lambda e: self.refresh_event_list())

        ttk.Label(self.filter_frame, text="日期:").pack(side=tk.LEFT, padx=(10, 0))
        self.date_var = tk.StringVar()
        self.date_entry = ttk.Entry(self.filter_frame, textvariable=self.date_var, width=12)
        self.date_entry.pack(side=tk.LEFT, padx=5)
        self.date_entry.bind("<KeyRelease>", lambda e: self.refresh_event_list())

        self.stats_label = ttk.Label(self.filter_frame, text="", foreground="gray")
        self.stats_label.pack(side=tk.RIGHT, padx=5)

        # 事件列表（带颜色）
        self.event_listbox = tk.Listbox(mid_frame, font=("Consolas", 10), exportselection=False)
        self.event_listbox.pack(fill=tk.BOTH, expand=True)
        self.event_listbox.bind("<<ListboxSelect>>", self.on_event_select)

        # ── 右侧详情 ──
        right_frame = ttk.Frame(root, width=500)
        right_frame.pack(side=tk.RIGHT, fill=tk.Y, padx=(0, 5), pady=5)

        detail_header = ttk.Frame(right_frame)
        detail_header.pack(fill=tk.X)
        ttk.Label(detail_header, text="事件详情", font=("", 11, "bold")).pack(side=tk.LEFT)
        ttk.Button(detail_header, text="复制全部", command=self.copy_detail).pack(side=tk.RIGHT)

        self.detail_text = tk.Text(right_frame, font=("Consolas", 10), wrap=tk.WORD, width=50)
        self.detail_text.pack(fill=tk.BOTH, expand=True)

        # ── 状态栏 ──
        self.status_var = tk.StringVar()
        self.status_bar = ttk.Label(root, textvariable=self.status_var, relief=tk.SUNKEN, anchor=tk.W)
        self.status_bar.pack(side=tk.BOTTOM, fill=tk.X)

        # ── 数据 ──
        self.all_events = []
        self.current_events = []
        self.current_file = None

        # ── 初始化 ──
        self.refresh_file_list()

    # ───────────── 方法 ─────────────

    def refresh_file_list(self):
        """刷新左侧文件列表"""
        self.file_listbox.delete(0, tk.END)
        files, log_dir = list_log_files(self.custom_log_dir)
        self.current_log_dir = log_dir
        self.dir_label.config(text=str(log_dir))
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

            # 搜索
            if search_text:
                summary = get_event_summary(evt).lower()
                type_match = search_text in t.lower()
                data_match = search_text in summary
                if not type_match and not data_match:
                    continue

            self.current_events.append(evt)

            # 显示
            fg, bg, tag = get_event_style(t)
            summary = get_event_summary(evt)
            summary_clean = summary.replace("\n", " ").strip()
            if len(summary_clean) > 100:
                summary_clean = summary_clean[:100] + "..."

            display = f"{ts[11:19] if len(ts)>=19 else ts}  {tag:12s} {summary_clean}"
            self.event_listbox.insert(tk.END, display)
            idx = self.event_listbox.size() - 1
            self.event_listbox.itemconfig(idx, fg=fg, bg=bg)

        # 状态栏
        stats = f"共 {len(self.all_events)} 事件，显示 {len(self.current_events)}"
        if self.current_file:
            stats += f" | {self.current_file.name}"
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
        self.detail_text.insert("1.0", json.dumps(evt, ensure_ascii=False, indent=2))

    def copy_detail(self):
        """复制详情到剪贴板"""
        content = self.detail_text.get("1.0", tk.END).strip()
        if content:
            self.root.clipboard_clear()
            self.root.clipboard_append(content)
            self.status_var.set("已复制到剪贴板")
        else:
            self.status_var.set("没有内容可复制")


# ── 附加功能：统计窗口 ────────────────────────────────

class StatsWindow:
    def __init__(self, parent, events, log_dir):
        self.win = tk.Toplevel(parent)
        self.win.title("事件统计")
        self.win.geometry("800x600")

        # 工具栏
        toolbar = ttk.Frame(self.win)
        toolbar.pack(fill=tk.X)
        ttk.Label(toolbar, text="日志目录:").pack(side=tk.LEFT)
        ttk.Label(toolbar, text=str(log_dir), foreground="gray").pack(side=tk.LEFT, padx=5)
        ttk.Button(toolbar, text="导出...", command=self.export).pack(side=tk.RIGHT)

        # 主内容
        panes = ttk.PanedWindow(self.win, orient=tk.HORIZONTAL)
        panes.pack(fill=tk.BOTH, expand=True, padx=5, pady=5)

        # 左侧统计树
        left_frame = ttk.Frame(panes)
        panes.add(left_frame, weight=1)
        self.tree = ttk.Treeview(left_frame, columns=("count", "prop"), show="tree")
        self.tree.heading("#0", text="事件类型")
        self.tree.heading("count", text="次数")
        self.tree.heading("prop", text="占比")
        self.tree.column("count", width=80, anchor=tk.E)
        self.tree.column("prop", width=80, anchor=tk.E)
        self.tree.pack(fill=tk.BOTH, expand=True)

        # 计算统计
        type_counter = Counter(e.get("type", "?") for e in events)
        total = len(events)
        for t, cnt in type_counter.most_common():
            prop = f"{cnt/total*100:.1f}%"
            self.tree.insert("", tk.END, text=t, values=(cnt, prop))

        # 右侧详情
        self.detail_text = tk.Text(panes, font=("Consolas", 9), wrap=tk.WORD)
        panes.add(self.detail_text, weight=1)

        self.tree.bind("<<TreeviewSelect>>", self.on_type_select)
        self.events = events
        self.log_dir = log_dir

    def on_type_select(self, event):
        sel = self.tree.selection()
        if not sel:
            return
        evt_type = self.tree.item(sel[0], "text")
        self.detail_text.delete("1.0", tk.END)
        for evt in self.events:
            if evt.get("type") == evt_type:
                ts = get_event_timestamp(evt)
                summary = get_event_summary(evt, 60)
                self.detail_text.insert(tk.END, f"[{ts}] {summary}\n")

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

    # 解析命令行参数
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
