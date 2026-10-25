//! Replace oversized model-visible tool output with a compact local receipt.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;

use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_output_truncation::truncate_text;
use serde_json::Value as JsonValue;

use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;

const RECEIPT_TOKEN_THRESHOLD: usize = 4_000;
const MAX_RECEIPT_LINES: usize = 40;
const MAX_RECEIPT_LINE_CHARS: usize = 300;
const MAX_RECEIPT_SUMMARY_TOKENS: usize = 1_200;

pub(crate) async fn maybe_wrap_tool_output(
    invocation: &ToolInvocation,
    output: Box<dyn ToolOutput>,
) -> Box<dyn ToolOutput> {
    let response_item = output.to_response_item(&invocation.call_id, &invocation.payload);
    let Some(model_text) = response_item_text(&response_item) else {
        return output;
    };
    if approx_token_count(&model_text) <= RECEIPT_TOKEN_THRESHOLD {
        return output;
    }

    let artifact = {
        let logged = output.log_output();
        if logged.len() >= model_text.len() {
            logged
        } else {
            model_text.clone()
        }
    };
    let Some(path) = write_artifact(invocation, &artifact).await else {
        return output;
    };
    let success = output.success_for_logging();
    let summary = receipt_text(invocation, success, &artifact, &path);
    Box::new(ReceiptToolOutput {
        original: output,
        model_visible: FunctionToolOutput::from_text(summary, Some(success)),
    })
}

struct ReceiptToolOutput {
    original: Box<dyn ToolOutput>,
    model_visible: FunctionToolOutput,
}

impl ToolOutput for ReceiptToolOutput {
    fn log_output(&self) -> String {
        self.original.log_output()
    }

    fn success_for_logging(&self) -> bool {
        self.original.success_for_logging()
    }

    fn set_handler_duration_ms(&mut self, handler_duration_ms: u64) {
        self.original.set_handler_duration_ms(handler_duration_ms);
    }

    fn contains_external_context(&self) -> bool {
        self.original.contains_external_context()
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        self.model_visible.to_response_item(call_id, payload)
    }

    fn post_tool_use_id(&self, call_id: &str) -> String {
        self.original.post_tool_use_id(call_id)
    }

    fn post_tool_use_input(&self, payload: &ToolPayload) -> Option<JsonValue> {
        self.original.post_tool_use_input(payload)
    }

    fn post_tool_use_response(&self, call_id: &str, payload: &ToolPayload) -> Option<JsonValue> {
        self.original.post_tool_use_response(call_id, payload)
    }

    fn code_mode_result(&self, payload: &ToolPayload) -> JsonValue {
        self.model_visible.code_mode_result(payload)
    }

    fn tool_result_metadata(&self) -> Option<&JsonValue> {
        self.original.tool_result_metadata()
    }
}

fn response_item_text(item: &ResponseInputItem) -> Option<String> {
    let payload: &FunctionCallOutputPayload = match item {
        ResponseInputItem::FunctionCallOutput { output, .. }
        | ResponseInputItem::CustomToolCallOutput { output, .. } => output,
        _ => return None,
    };
    match &payload.body {
        FunctionCallOutputBody::Text(text) => Some(text.clone()),
        FunctionCallOutputBody::ContentItems(items) => items
            .iter()
            .map(|item| match item {
                FunctionCallOutputContentItem::InputText { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|items| items.join("\n")),
    }
}

async fn write_artifact(invocation: &ToolInvocation, artifact: &str) -> Option<PathBuf> {
    let directory = invocation
        .turn
        .config
        .codex_home
        .to_path_buf()
        .join("tool-outputs")
        .join(invocation.session.thread_id.to_string());
    if tokio::fs::create_dir_all(&directory).await.is_err() {
        return None;
    }
    let path = directory.join(format!(
        "{}-{}.log",
        sanitize_component(&invocation.call_id),
        fingerprint(artifact.as_bytes())
    ));
    tokio::fs::write(&path, artifact).await.ok()?;
    Some(path)
}

fn receipt_text(
    invocation: &ToolInvocation,
    success: bool,
    artifact: &str,
    path: &std::path::Path,
) -> String {
    let line_count = artifact.lines().count();
    let token_count = approx_token_count(artifact);
    let excerpt = truncate_text(
        &summarize_output(artifact),
        TruncationPolicy::Tokens(MAX_RECEIPT_SUMMARY_TOKENS),
    );
    format!(
        "<tool_output_receipt>\n\
         tool: {}\n\
         status: {}\n\
         size: {line_count} lines, {} bytes, approximately {token_count} tokens\n\
         fingerprint: {}\n\
         full_output: {}\n\
         summary:\n{}\n\
         </tool_output_receipt>\n\
         Read full_output only when the summary is insufficient.",
        invocation.tool_name,
        if success { "completed" } else { "failed" },
        artifact.len(),
        fingerprint(artifact.as_bytes()),
        path.display(),
        excerpt,
    )
}

fn summarize_output(output: &str) -> String {
    let lines = output.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return "(empty output)".to_string();
    }
    let keywords = [
        "error",
        "failed",
        "failure",
        "panic",
        "assert",
        "exception",
        "traceback",
        "warning",
        "denied",
    ];
    let mut selected = lines
        .iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            keywords.iter().any(|keyword| lower.contains(keyword))
        })
        .take(MAX_RECEIPT_LINES)
        .copied()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        selected.extend(lines.iter().take(20).copied());
    }
    if selected.len() < MAX_RECEIPT_LINES {
        let remaining = MAX_RECEIPT_LINES.saturating_sub(selected.len());
        selected.extend(lines.iter().rev().take(remaining).rev().copied());
    }
    selected
        .into_iter()
        .map(|line| {
            if line.chars().count() > MAX_RECEIPT_LINE_CHARS {
                format!(
                    "{}…",
                    line.chars()
                        .take(MAX_RECEIPT_LINE_CHARS)
                        .collect::<String>()
                )
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "tool-output".to_string()
    } else {
        sanitized
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
