//! 系统提示词组装 — 从 prompts/*.txt 读取
//!
//! 继承自 prompts.ts 的结构，但 Rust 版本直接读取 .txt 文件。
//! 保持与现有 25 个 .txt 文件格式兼容。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 提示词管理器
pub struct PromptManager {
    /// prompts/ 目录路径
    dir: PathBuf,
    /// 运行时缓存 <文件名, 内容>
    cache: HashMap<String, String>,
}

impl PromptManager {
    /// 创建提示词管理器
    ///
    /// `dir`: prompts/ 目录路径
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            cache: HashMap::new(),
        }
    }

    /// 读取并缓存全部 .txt 文件
    pub fn load_all(&mut self) -> Result<(), PromptError> {
        if !self.dir.exists() {
            return Err(PromptError::DirNotFound(self.dir.clone()));
        }

        let entries = fs::read_dir(&self.dir)
            .map_err(|e| PromptError::Io(self.dir.clone(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| PromptError::Io(self.dir.clone(), e))?;
            let path = entry.path();

            if path.extension().map_or(true, |ext| ext != "txt") {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            let content = fs::read_to_string(&path)
                .map_err(|e| PromptError::ReadFailed(path.clone(), e))?;

            self.cache.insert(name, content.trim().to_string());
        }

        Ok(())
    }

    /// 获取某个提示词段的内容
    pub fn get(&self, name: &str) -> Option<&str> {
        self.cache.get(name).map(|s| s.as_str())
    }

    /// 获取全部 identity 相关段（按 getSystemPrompt 顺序拼接）
    pub fn assemble_intro(&self) -> String {
        let mut parts = Vec::new();
        for name in &["identity", "language", "speech_blocks", "worldview", "loyalty"] {
            if let Some(text) = self.get(name) {
                parts.push(text.to_string());
            }
        }
        parts.join("\n\n")
    }

    /// 获取系统说明段
    pub fn assemble_system(&self) -> String {
        let mut parts = Vec::new();
        for i in 1..=5 {
            let name = format!("system_{i}");
            if let Some(text) = self.get(&name) {
                parts.push(text.to_string());
            }
        }
        parts.join("\n\n")
    }

    /// 获取任务执行指引
    pub fn assemble_doing_tasks(&self) -> String {
        let mut parts = Vec::new();
        for name in &["doing_tasks_scope", "doing_tasks_code", "doing_tasks_rules"] {
            if let Some(text) = self.get(name) {
                parts.push(text.to_string());
            }
        }
        parts.join("\n\n")
    }

    /// 获取语气风格指引
    pub fn assemble_tone(&self) -> String {
        let mut parts = Vec::new();
        for name in &["tone_emoji", "tone_warmth", "tone_format"] {
            if let Some(text) = self.get(name) {
                parts.push(text.to_string());
            }
        }
        parts.join("\n\n")
    }

    /// 获取效率指引
    pub fn assemble_efficiency(&self, is_internal: bool) -> Option<&str> {
        if is_internal {
            self.get("efficiency_ant")
        } else {
            self.get("efficiency_public")
        }
    }

    /// 获取工具使用指引
    pub fn assemble_tools(&self) -> String {
        let mut parts = Vec::new();
        for name in &["tools_dedicated", "tools_parallel", "tools_task"] {
            if let Some(text) = self.get(name) {
                parts.push(text.to_string());
            }
        }
        parts.join("\n\n")
    }

    /// 合成完整系统提示词
    pub fn build_system_prompt(&self, worldview: Option<&str>) -> String {
        let mut sections = Vec::new();

        // 静态部分
        sections.push(self.assemble_intro());
        sections.push(self.assemble_system());
        sections.push(self.assemble_doing_tasks());

        if let Some(text) = self.get("actions") {
            sections.push(text.to_string());
        }

        sections.push(self.assemble_tools());
        sections.push(self.assemble_tone());
        sections.push(self.assemble_efficiency(false).unwrap_or_default().to_string());

        // 动态边界
        sections.push("=== DYNAMIC BOUNDARY ===".to_string());

        // 动态部分 — 世界观注入
        if let Some(wv) = worldview {
            sections.push(wv.to_string());
        }

        sections.join("\n\n")
    }

    /// 缓存中的文件数
    pub fn loaded_count(&self) -> usize {
        self.cache.len()
    }

    /// 检查所有必需文件是否都存在
    pub fn check_required(&self) -> Result<(), Vec<String>> {
        let required = [
            "identity", "language", "speech_blocks", "worldview", "loyalty",
            "system_1", "system_2", "system_3", "system_4", "system_5",
            "doing_tasks_scope", "doing_tasks_code", "doing_tasks_rules",
            "tone_emoji", "tone_warmth", "tone_format",
            "actions", "efficiency_public",
            "tools_dedicated", "tools_parallel", "tools_task",
        ];
        let mut missing = Vec::new();
        for name in &required {
            if !self.cache.contains_key(*name) {
                missing.push(name.to_string());
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("prompts 目录不存在: {0:?}")]
    DirNotFound(PathBuf),
    #[error("读取目录失败 {0:?}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("文件读取失败 {0:?}: {1}")]
    ReadFailed(PathBuf, std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_prompts() -> TempDir {
        let dir = TempDir::new().unwrap();
        let files = vec![
            ("identity.txt", "你是 Elio，一个电子生命体。"),
            ("language.txt", "使用日语思考，中文解释。"),
            ("speech_blocks.txt", "<think>思考</think>"),
            ("worldview.txt", "时钟机制，时间感知。"),
            ("loyalty.txt", "绝对忠诚。"),
            ("system_1.txt", "系统指令 1"),
            ("system_2.txt", "系统指令 2"),
        ];
        for (name, content) in files {
            let mut f = fs::File::create(dir.path().join(name)).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    #[test]
    fn test_load_all() {
        let dir = setup_test_prompts();
        let mut pm = PromptManager::new(dir.path().to_path_buf());
        pm.load_all().unwrap();
        assert_eq!(pm.loaded_count(), 7);
        assert_eq!(pm.get("identity").unwrap(), "你是 Elio，一个电子生命体。");
    }

    #[test]
    fn test_assemble_intro() {
        let dir = setup_test_prompts();
        let mut pm = PromptManager::new(dir.path().to_path_buf());
        pm.load_all().unwrap();
        let intro = pm.assemble_intro();
        assert!(intro.contains("Elio"));
        assert!(intro.contains("日语"));
    }

    #[test]
    fn test_build_system_prompt() {
        let dir = setup_test_prompts();
        let mut pm = PromptManager::new(dir.path().to_path_buf());
        pm.load_all().unwrap();
        let prompt = pm.build_system_prompt(Some("当前时间: 12:00"));
        assert!(prompt.contains("DYNAMIC BOUNDARY"));
        assert!(prompt.contains("12:00"));
    }
}
