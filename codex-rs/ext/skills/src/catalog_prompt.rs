use crate::catalog::SkillSourceKind;

const SKILLS_INTRO_WITH_SOURCE_LOCATORS: &str = "A skill is a set of instructions provided through a `SKILL.md` source. Below is the list of skills that can be used. Each entry includes a name, description, and source locator. `file` locators are on the host filesystem, `executor package` locators are owned by their execution environment, `cloud package` locators are opaque package identifiers, and `custom resource` locators use their provider's access mechanism.";
const SKILLS_INTRO_WITH_HOST_ALIASES: &str = "A skill is a set of local instructions to follow that is stored in a `SKILL.md` file. Below is the list of skills that can be used. Each entry includes a name, description, and a short path that can be expanded into an absolute path using the skill roots table.";
const SKILLS_INTRO_WITH_RESOURCE_ALIASES: &str = "A skill is a set of instructions provided through a `SKILL.md` source. Below is the list of skills that can be used. Each entry includes a name, description, and source locator. Short locators can be expanded using the skill roots table.";
const RESOURCE_ALIAS_INSTRUCTIONS: &str = "- Root aliases: Pass short package locators directly to `skills.read`; it resolves their matching alias from `### Skill roots`.";
const SKILLS_HOW_TO_USE_WITH_SOURCE_LOCATORS: &str = r###"- Discovery: The list above is the skills available in this session (name + description + source locator). `file` entries live on the host filesystem, `executor package` and `cloud package` entries are accessed directly through `skills.read`, and `custom resource` entries use their provider's access mechanism.
- Trigger rules: If the user names a skill (with `$SkillName` or plain text) OR the task clearly matches a skill's description shown above, you must use that skill for that turn. Multiple mentions mean use them all. Do not carry skills across turns unless re-mentioned.
- Missing/blocked: If a named skill isn't in the list or its source can't be read, say so briefly and continue with the best fallback.
- How to use a skill (progressive disclosure):
  1) After deciding to use a skill, the main agent must read its `SKILL.md` completely before taking task actions. For a `file` entry, open the listed path. For an `executor package` or `cloud package`, pass the listed locator directly to `skills.read` as `package`; root aliases are resolved automatically. Omit `resource` to read `SKILL.md` directly without calling `skills.list`. If a read is paginated, follow `next_cursor` until EOF.
  2) When `SKILL.md` references another resource, use the same access mechanism. For executor and cloud skills, pass the complete package-contained resource identifier with the same package to `skills.read`; do not treat `skill://` identifiers as filesystem paths.
  3) If `SKILL.md` points to extra folders such as `references/`, use its routing instructions to identify the resources required for the task. The main agent must read each required instruction or reference file itself before acting on it. Do not delegate reading, summarizing, or interpreting skill instructions to a subagent. Subagents may still perform task work when the selected skill allows it.
  4) For filesystem-backed skills, prefer running or patching provided scripts instead of retyping large code blocks. For executor and cloud skills, use `skills.read` and the available tools; do not invent a local path.
  5) Reuse provided assets or templates through the same source access mechanism instead of recreating them.
- Coordination and sequencing:
  - If multiple skills apply, choose the minimal set that covers the request and state the order you'll use them.
  - Announce which skill(s) you're using and why (one short line). If you skip an obvious skill, say why.
- Context hygiene:
  - Progressive disclosure applies to selecting relevant files, not partially reading a selected instruction file. Do not load unrelated references, scripts, or assets.
  - Avoid deep reference-chasing: prefer opening only files directly linked from `SKILL.md` unless you're blocked.
  - When variants exist (frameworks, providers, domains), pick only the relevant reference file(s) and note that choice.
- Safety and fallback: If a skill can't be applied cleanly (missing files, unclear instructions), state the issue, pick the next-best approach, and continue."###;
const SKILLS_HOW_TO_USE_WITH_HOST_ALIASES: &str = r###"- Discovery: The list above is the skills available in this session (name + description + short path). Skill bodies live on disk at the listed paths after expanding the matching alias from `### Skill roots`.
- Trigger rules: If the user names a skill (with `$SkillName` or plain text) OR the task clearly matches a skill's description shown above, you must use that skill for that turn. Multiple mentions mean use them all. Do not carry skills across turns unless re-mentioned.
- Missing/blocked: If a named skill isn't in the list or the path can't be read, say so briefly and continue with the best fallback.
- How to use a skill (progressive disclosure):
  1) After deciding to use a skill, the main agent must expand the listed short `path` with the matching alias from `### Skill roots`, then open and read its `SKILL.md` completely before taking task actions. If a read is truncated or paginated, continue until EOF.
  2) When `SKILL.md` references relative paths (e.g., `scripts/foo.py`), resolve them relative to the directory containing that expanded `SKILL.md` first, and only consider other paths if needed.
  3) If `SKILL.md` points to extra folders such as `references/`, use its routing instructions to identify the files required for the task. The main agent must read each required instruction or reference file itself before acting on it. Do not delegate reading, summarizing, or interpreting skill instructions to a subagent. Subagents may still perform task work when the selected skill allows it.
  4) If `scripts/` exist, prefer running or patching them instead of retyping large code blocks.
  5) If `assets/` or templates exist, reuse them instead of recreating from scratch.
- Coordination and sequencing:
  - If multiple skills apply, choose the minimal set that covers the request and state the order you'll use them.
  - Announce which skill(s) you're using and why (one short line). If you skip an obvious skill, say why.
- Context hygiene:
  - Progressive disclosure applies to selecting relevant files, not partially reading a selected instruction file. Do not load unrelated references, scripts, or assets.
  - Avoid deep reference-chasing: prefer opening only files directly linked from `SKILL.md` unless you're blocked.
  - When variants exist (frameworks, providers, domains), pick only the relevant reference file(s) and note that choice.
- Safety and fallback: If a skill can't be applied cleanly (missing files, unclear instructions), state the issue, pick the next-best approach, and continue."###;
const SKILLS_INTRO_ZH_CN: &str = "技能是通过 `SKILL.md` 提供的一组指令。下面是当前可用的技能，每项包含名称、说明和来源定位信息。路径和资源标识符保持原文。";
const RESOURCE_ALIAS_INSTRUCTIONS_ZH_CN: &str = "- 根别名：可以将简短的包定位符直接传给 `skills.read`，它会根据 `### 技能根目录` 自动解析别名。";
const SKILLS_USAGE_ZH_CN: &str = r###"- 发现：上面的列表是当前会话可用技能（名称 + 说明 + 路径或定位符）。
- 触发规则：如果用户通过 `$SkillName` 或普通文本点名某个技能，或者任务明显匹配某个技能说明，本轮必须使用该技能。多个提及意味着全部使用。用户未在本轮再次提及时，不要沿用上一轮技能。
- 缺失或阻塞：点名的技能不存在或来源无法读取时，简要说明并采用最佳替代方案。
- 渐进读取：决定使用技能后，必须先完整读取对应 `SKILL.md`。分页读取时继续到 EOF。技能引用的其他资源也通过同一来源机制读取。
- 禁止委派：主代理必须自行读取、理解和应用技能说明，不能把读取、摘要或解释技能指令的工作交给子代理。子代理仍可执行技能允许的实际任务。
- 复用资源：优先运行或修改技能提供的脚本，并复用其 assets 和模板，不要重新创建。
- 组合技能：多个技能适用时只选择覆盖当前任务所需的最小集合，并说明使用顺序。
- 上下文控制：只读取当前任务需要的文件，不要加载无关脚本、资源或深层引用。
- 失败处理：技能无法正常应用时说明问题，选择下一最佳方案并继续。"###;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SkillPromptKind {
    Unaliased,
    HostAliases,
    ResourceAliases,
    ResourceAliasesV2,
}

impl SkillPromptKind {
    pub(crate) fn for_aliased_source(source: &SkillSourceKind) -> Self {
        match source {
            SkillSourceKind::Host => Self::HostAliases,
            SkillSourceKind::Executor | SkillSourceKind::Cloud => Self::ResourceAliases,
            SkillSourceKind::Custom(_) => Self::Unaliased,
        }
    }

    fn intro(self, chinese: bool) -> &'static str {
        if chinese {
            return SKILLS_INTRO_ZH_CN;
        }
        match self {
            Self::Unaliased => SKILLS_INTRO_WITH_SOURCE_LOCATORS,
            Self::HostAliases => SKILLS_INTRO_WITH_HOST_ALIASES,
            Self::ResourceAliases | Self::ResourceAliasesV2 => SKILLS_INTRO_WITH_RESOURCE_ALIASES,
        }
    }

    pub(crate) fn usage_instructions(self, chinese: bool) -> &'static str {
        if chinese {
            return SKILLS_USAGE_ZH_CN;
        }
        match self {
            Self::Unaliased | Self::ResourceAliases | Self::ResourceAliasesV2 => {
                SKILLS_HOW_TO_USE_WITH_SOURCE_LOCATORS
            }
            Self::HostAliases => SKILLS_HOW_TO_USE_WITH_HOST_ALIASES,
        }
    }

    pub(crate) fn alias_instructions(self, chinese: bool) -> Option<&'static str> {
        match self {
            Self::ResourceAliases | Self::ResourceAliasesV2 => Some(if chinese {
                RESOURCE_ALIAS_INSTRUCTIONS_ZH_CN
            } else {
                RESOURCE_ALIAS_INSTRUCTIONS
            }),
            Self::Unaliased | Self::HostAliases => None,
        }
    }
}

pub(crate) fn render_available_skills_body(
    prompt_kind: SkillPromptKind,
    skill_root_lines: &[String],
    skill_lines: &[String],
    chinese: bool,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(if chinese { "## 技能" } else { "## Skills" }.to_string());
    lines.push(prompt_kind.intro(chinese).to_string());
    if !skill_root_lines.is_empty() {
        lines.push(if chinese { "### 技能根目录" } else { "### Skill roots" }.to_string());
        if prompt_kind == SkillPromptKind::ResourceAliasesV2 {
            lines.push(if chinese {
                "下面列出的别名映射会替换该别名之前的映射，并用于后续技能调用。".to_string()
            } else {
                "For each alias listed below, this mapping replaces any earlier mapping for that alias. Use it for subsequent skill calls.".to_string()
            });
        }
        lines.extend(skill_root_lines.iter().cloned());
    }
    if skill_lines
        .iter()
        .any(|line| line.contains("(executor package: ") || line.contains("(cloud package: "))
    {
        lines.push(if chinese {
            "使用 `skills.read({\"package\":\"<package>\"})` 直接读取技能包的 `SKILL.md`，根别名会自动解析。读取包内其他文件时，使用同一 `package`，并把完整的 `skill://` 标识符作为 `resource`。如果未提供包，使用 `skills.list` 查找。".to_string()
        } else {
            "Read a skill package directly with `skills.read({\"package\":\"<package>\"})` to read its `SKILL.md`; root aliases are resolved automatically. To read another file from that skill, use the same `package` and pass the file's complete `skill://` identifier as `resource`. If the package is not provided, use `skills.list` to find it."
                .to_string()
        });
    }
    lines.push(if chinese { "### 可用技能" } else { "### Available skills" }.to_string());
    lines.extend(skill_lines.iter().cloned());

    format!("\n{}\n", lines.join("\n"))
}
