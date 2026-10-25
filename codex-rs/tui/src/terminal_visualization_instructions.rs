use crate::legacy_core::config::Config;
use codex_features::Feature;

pub(crate) const TERMINAL_VISUALIZATION_INSTRUCTIONS: &str = "\
- This surface is a terminal. When the formatting rules require a visual, include one in the final answer using compact ASCII diagrams, trees, timelines, or tables.
- Use tables for exact mappings or comparisons rather than collapsing known mappings into prose.
- Use trees for hierarchy or one-to-many relationships, and diagrams or timelines for sequence, change, or state transferred between records across event order.
- Use only ASCII characters in visuals.";
pub(crate) const TERMINAL_VISUALIZATION_INSTRUCTIONS_ZH_CN: &str = "\
- 当前界面是终端。格式规则要求可视化时，在最终答复中使用紧凑的 ASCII 图、树、时间线或表格。
- 精确映射或对比使用表格，不要把已知映射压缩成散文。
- 层级或一对多关系使用树；序列、变化或记录状态迁移使用图或时间线。
- 可视化中只使用 ASCII 字符。";

pub(crate) fn with_terminal_visualization_instructions(
    config: &Config,
    control_instructions: Option<String>,
) -> Option<String> {
    if !config
        .features
        .enabled(Feature::TerminalVisualizationInstructions)
    {
        return control_instructions;
    }

    let existing_instructions =
        control_instructions.or_else(|| config.developer_instructions.clone());
    let visualization_instructions = match config.tui_language.effective() {
        codex_config::types::TuiLanguage::Chinese => TERMINAL_VISUALIZATION_INSTRUCTIONS_ZH_CN,
        codex_config::types::TuiLanguage::English | codex_config::types::TuiLanguage::Auto => {
            TERMINAL_VISUALIZATION_INSTRUCTIONS
        }
    };
    Some(match existing_instructions.as_deref() {
        Some(existing) if !existing.trim().is_empty() => {
            format!("{existing}\n\n{visualization_instructions}")
        }
        _ => visualization_instructions.to_string(),
    })
}
