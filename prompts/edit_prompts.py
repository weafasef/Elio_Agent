#!/usr/bin/env python3
"""Elio 提示词编辑工具 — GUI 版"""

import tkinter as tk
from tkinter import ttk, messagebox, simpledialog
from pathlib import Path

PROMPTS_DIR = Path(__file__).resolve().parent.parent / "prompts"

# 提示词组装顺序（与 prompt.rs build_system_prompt 一致）
ASSEMBLY_ORDER = [
    # assemble_intro()
    "identity",
    "language",
    "speech_blocks",
    "worldview",
    "loyalty",
    # assemble_system()
    "system_1",
    "system_2",
    "system_3",
    "system_4",
    "system_5",
    # assemble_doing_tasks()
    "doing_tasks_scope",
    "doing_tasks_code",
    "doing_tasks_rules",
    # 独立 actions
    "actions",
    # assemble_tools()
    "tools_dedicated",
    "tools_parallel",
    "tools_task",
    # assemble_tone()
    "tone_emoji",
    "tone_warmth",
    "tone_format",
    # efficiency
    "efficiency_public",
    "efficiency_ant",
    # agent（不属于主流程，但与工具相关）
    "agent_fork",
    "agent_subagent",
    "sub_agent",
]


class PromptEditor(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("Elio 提示词编辑器")
        self.geometry("900x650+200+200")
        self.minsize(750, 500)

        self._build_ui()
        self._load_file_list()

    # ── UI 构建 ────────────────────────────────────────────

    def _build_ui(self):
        # 主布局：左列表 + 右编辑区
        paned = ttk.PanedWindow(self, orient=tk.HORIZONTAL)
        paned.pack(fill=tk.BOTH, expand=True, padx=6, pady=6)

        # ── 左侧面板 ──
        left = ttk.Frame(paned, width=260)
        paned.add(left, weight=0)

        toolbar = ttk.Frame(left)
        toolbar.pack(fill=tk.X, pady=(0, 4))

        ttk.Button(toolbar, text="🔄 刷新", command=self._load_file_list, width=8).pack(side=tk.LEFT, padx=1)
        ttk.Button(toolbar, text="➕ 新建", command=self._new_file, width=8).pack(side=tk.LEFT, padx=1)
        ttk.Button(toolbar, text="🗑️ 删除", command=self._delete_file, width=8).pack(side=tk.LEFT, padx=1)

        self.listbox = tk.Listbox(left, font=("Consolas", 11), selectbackground="#0078d7")
        self.listbox.pack(fill=tk.BOTH, expand=True)
        self.listbox.bind("<<ListboxSelect>>", self._on_select)

        # ── 右侧面板 ──
        right = ttk.Frame(paned)
        paned.add(right, weight=1)

        # 文件名标题
        self.title_var = tk.StringVar()
        title_label = ttk.Label(right, textvariable=self.title_var, font=("Segoe UI", 12, "bold"))
        title_label.pack(anchor=tk.W, pady=(0, 4))

        # 编辑区
        text_frame = ttk.Frame(right)
        text_frame.pack(fill=tk.BOTH, expand=True)

        self.text = tk.Text(
            text_frame,
            wrap=tk.WORD,
            font=("Microsoft YaHei", 11),
            undo=True,
            padx=8,
            pady=8,
            relief=tk.FLAT,
            borderwidth=1,
        )
        self.text.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        scrollbar = ttk.Scrollbar(text_frame, orient=tk.VERTICAL, command=self.text.yview)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.text.config(yscrollcommand=scrollbar.set)

        # 底栏
        bottom = ttk.Frame(right)
        bottom.pack(fill=tk.X, pady=(6, 0))

        self.info_var = tk.StringVar(value="未选择文件")
        ttk.Label(bottom, textvariable=self.info_var, font=("Segoe UI", 9)).pack(side=tk.LEFT)

        ttk.Button(bottom, text="💾 保存", command=self._save_file, width=10).pack(side=tk.RIGHT, padx=2)

    # ── 文件操作 ────────────────────────────────────────────

    def _sort_files(self, files):
        """按 ASSEMBLY_ORDER 排序，未匹配的放到末尾"""
        def sort_key(f):
            stem = f.stem
            if stem in ASSEMBLY_ORDER:
                return (0, ASSEMBLY_ORDER.index(stem))
            return (1, stem)
        return sorted(files, key=sort_key)

    def _load_file_list(self):
        self.listbox.delete(0, tk.END)
        if not PROMPTS_DIR.exists():
            self._files = []
        else:
            all_files = sorted(PROMPTS_DIR.iterdir())
            # 只保留 .txt 文件
            txt_files = [f for f in all_files if f.suffix == ".txt"]
            self._files = self._sort_files(txt_files)

        for f in self._files:
            size = f.stat().st_size
            icon = "📄" if size > 0 else "⬜"
            self.listbox.insert(tk.END, f" {icon} {f.stem}")
        self._current_file = None
        self.title_var.set("")
        self.text.delete("1.0", tk.END)
        self.info_var.set(f"{len(self._files)} 个文件（按组装顺序）")

    def _on_select(self, event):
        sel = self.listbox.curselection()
        if not sel:
            return
        idx = sel[0]
        filepath = self._files[idx]
        self._current_file = filepath

        if filepath.stat().st_size == 0:
            content = ""
        else:
            content = filepath.read_text(encoding="utf-8")

        self.text.delete("1.0", tk.END)
        self.text.insert("1.0", content)
        self.text.edit_reset()
        self.text.mark_set(tk.INSERT, "1.0")
        self.text.focus_set()

        # 显示序号和文件名
        stem = filepath.stem
        if stem in ASSEMBLY_ORDER:
            order = ASSEMBLY_ORDER.index(stem) + 1
            pos = f"#{order}"
        else:
            pos = "附录"
        self.title_var.set(f"📄 {filepath.name}  [{pos}]")
        self.info_var.set(f"{len(content)} 字 | {(filepath.stat().st_size)}B")

    def _save_file(self, event=None):
        if not self._current_file:
            messagebox.showwarning("提示", "请先选择一个文件")
            return
        content = self.text.get("1.0", tk.END).rstrip("\n")
        self._current_file.write_text(content + "\n", encoding="utf-8")
        self.info_var.set(f"{len(content)} 字 | 已保存")
        self._load_file_list()
        # 恢复选中
        for i, f in enumerate(self._files):
            if f == self._current_file:
                self.listbox.selection_set(i)
                self._current_file = f
                stem = f.stem
                if stem in ASSEMBLY_ORDER:
                    order = ASSEMBLY_ORDER.index(stem) + 1
                    pos = f"#{order}"
                else:
                    pos = "附录"
                self.title_var.set(f"📄 {f.name}  [{pos}]")
                break

    def _new_file(self):
        name = simpledialog.askstring("新建提示词", "文件名（不含 .txt）:")
        if not name:
            return
        new_file = PROMPTS_DIR / (name + ".txt")
        if new_file.exists():
            messagebox.showwarning("提示", f"{name}.txt 已存在")
            return
        new_file.write_text("", encoding="utf-8")
        self._load_file_list()
        # 选中新建的文件
        for i, f in enumerate(self._files):
            if f == new_file:
                self.listbox.selection_set(i)
                self._on_select(None)
                break

    def _delete_file(self):
        if not self._current_file:
            return
        name = self._current_file.name
        if not messagebox.askyesno("确认删除", f"确定删除 {name}？", icon="warning"):
            return
        self._current_file.unlink()
        self._current_file = None
        self._load_file_list()


if __name__ == "__main__":
    app = PromptEditor()
    app.mainloop()
