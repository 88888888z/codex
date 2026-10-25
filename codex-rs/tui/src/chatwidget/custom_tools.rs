//! `/tools` diagnostics for user-configured executable tools.

impl super::ChatWidget {
    pub(crate) fn show_custom_tools(&mut self) {
        let path = self.local_settings.codex_home.join("tools.toml");
        match crate::custom_tools::CustomToolRegistry::load(path.clone().to_path_buf()) {
            Ok(registry) if registry.is_empty() => {
                self.add_info_message(
                    crate::i18n::ui_text("No custom tools configured", "未配置自定义工具")
                        .to_string(),
                    Some(format!(
                        "{}\n\n{}",
                        path.display(),
                        crate::i18n::ui_text(
                            "Create tools.toml with one or more [[tools]] entries, then restart Codex. Example:\n\n[[tools]]\nname = \"analyze_file\"\ndescription = \"Analyze a local file\"\ncommand = \"python3\"\nargs = [\"/absolute/path/to/analyze.py\"]\napproval_mode = \"prompt\"\nread_only = true\ninput_schema = { type = \"object\", properties = { path = { type = \"string\" } }, required = [\"path\"] }",
                            "创建 tools.toml，写入一个或多个 [[tools]]，然后重启 Codex。例如：\n\n[[tools]]\nname = \"analyze_file\"\ndescription = \"分析本地文件\"\ncommand = \"python3\"\nargs = [\"/absolute/path/to/analyze.py\"]\napproval_mode = \"prompt\"\nread_only = true\ninput_schema = { type = \"object\", properties = { path = { type = \"string\" } }, required = [\"path\"] }",
                        )
                    )),
                );
            }
            Ok(registry) => {
                let summaries = registry.summaries().join("\n\n");
                self.add_info_message(
                    crate::i18n::ui_text("Custom tools", "自定义工具").to_string(),
                    Some(format!(
                        "{}\n\n{}\n\n{}",
                        path.display(),
                        summaries,
                        crate::i18n::ui_text(
                            "Edit tools.toml to change tools. Restart Codex to reload them.",
                            "编辑 tools.toml 可修改工具；重启 Codex 后重新加载。"
                        )
                    )),
                );
            }
            Err(error) => {
                self.add_error_message(format!(
                    "{}\n{}",
                    crate::i18n::ui_text("Invalid custom tool configuration", "自定义工具配置无效"),
                    error
                ));
            }
        }
    }
}
