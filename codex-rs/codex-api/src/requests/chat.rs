use crate::error::ApiError;
use crate::requests::headers::build_session_headers;
use crate::requests::headers::insert_header;
use crate::requests::headers::subagent_header;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ImageReference;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::SessionSource;
use http::HeaderMap;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;

/// Assembled request body and headers for Chat Completions streaming calls.
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    #[serde(skip)]
    pub headers: HeaderMap,
    pub body: Value,
}

pub struct ChatRequestBuilder<'a> {
    model: &'a str,
    instructions: &'a str,
    input: &'a [ResponseItem],
    tools: &'a [Value],
    conversation_id: Option<String>,
    session_source: Option<SessionSource>,
}

impl<'a> ChatRequestBuilder<'a> {
    pub fn new(
        model: &'a str,
        instructions: &'a str,
        input: &'a [ResponseItem],
        tools: &'a [Value],
    ) -> Self {
        Self {
            model,
            instructions,
            input,
            tools,
            conversation_id: None,
            session_source: None,
        }
    }

    pub fn conversation_id(mut self, id: Option<String>) -> Self {
        self.conversation_id = id;
        self
    }

    pub fn session_source(mut self, source: Option<SessionSource>) -> Self {
        self.session_source = source;
        self
    }

    pub fn build(self) -> Result<ChatRequest, ApiError> {
        let mut messages = Vec::<Value>::new();
        messages.push(json!({"role": "system", "content": self.instructions}));

        let mut reasoning_by_anchor_index: HashMap<usize, String> = HashMap::new();
        let mut last_emitted_role: Option<&str> = None;
        for item in self.input {
            match item {
                ResponseItem::Message { role, .. } => last_emitted_role = Some(role.as_str()),
                ResponseItem::FunctionCall { .. } | ResponseItem::LocalShellCall { .. } => {
                    last_emitted_role = Some("assistant")
                }
                ResponseItem::FunctionCallOutput { .. } => last_emitted_role = Some("tool"),
                _ => {}
            }
        }

        let last_user_index = self
            .input
            .iter()
            .rposition(|item| matches!(item, ResponseItem::Message { role, .. } if role == "user"));
        if !matches!(last_emitted_role, Some("user")) {
            for (idx, item) in self.input.iter().enumerate() {
                if last_user_index.is_some_and(|user_idx| idx <= user_idx) {
                    continue;
                }
                let ResponseItem::Reasoning {
                    content: Some(items),
                    ..
                } = item
                else {
                    continue;
                };
                let text = items
                    .iter()
                    .map(|entry| match entry {
                        ReasoningItemContent::ReasoningText { text }
                        | ReasoningItemContent::Text { text } => text.as_str(),
                    })
                    .collect::<String>();
                if text.trim().is_empty() {
                    continue;
                }
                let anchor = idx
                    .checked_sub(1)
                    .filter(|prev| {
                        matches!(
                            &self.input[*prev],
                            ResponseItem::Message { role, .. } if role == "assistant"
                        )
                    })
                    .or_else(|| {
                        (idx + 1 < self.input.len()
                            && (matches!(
                                &self.input[idx + 1],
                                ResponseItem::FunctionCall { .. }
                                    | ResponseItem::LocalShellCall { .. }
                            ) || matches!(
                                &self.input[idx + 1],
                                ResponseItem::Message { role, .. } if role == "assistant"
                            )))
                        .then_some(idx + 1)
                    });
                if let Some(anchor) = anchor {
                    reasoning_by_anchor_index
                        .entry(anchor)
                        .and_modify(|existing| existing.push_str(&text))
                        .or_insert(text);
                }
            }
        }

        let mut last_assistant_text: Option<String> = None;
        for (idx, item) in self.input.iter().enumerate() {
            match item {
                ResponseItem::Message { role, content, .. } => {
                    let mut text = String::new();
                    let mut parts = Vec::new();
                    let mut has_image = false;
                    for part in content {
                        match part {
                            ContentItem::InputText { text: value }
                            | ContentItem::OutputText { text: value, .. } => {
                                text.push_str(value);
                                parts.push(json!({"type": "text", "text": value}));
                            }
                            ContentItem::InputImage { image, .. } => {
                                if let ImageReference::Inline { image_url } = image {
                                    has_image = true;
                                    parts.push(json!({
                                        "type": "image_url",
                                        "image_url": {"url": image_url}
                                    }));
                                }
                            }
                            ContentItem::InputAudio { .. } => {}
                        }
                    }
                    if role == "assistant" {
                        if last_assistant_text.as_ref() == Some(&text) {
                            continue;
                        }
                        last_assistant_text = Some(text.clone());
                    }
                    let content_value = if role == "assistant" || !has_image {
                        json!(text)
                    } else {
                        json!(parts)
                    };
                    let mut message = json!({"role": role, "content": content_value});
                    if role == "assistant"
                        && let Some(reasoning) = reasoning_by_anchor_index.get(&idx)
                    {
                        message["reasoning"] = json!(reasoning);
                    }
                    messages.push(message);
                }
                ResponseItem::FunctionCall {
                    name,
                    arguments,
                    call_id,
                    ..
                } => {
                    push_tool_call(
                        &mut messages,
                        json!({
                            "id": call_id,
                            "type": "function",
                            "function": {"name": name, "arguments": arguments}
                        }),
                        reasoning_by_anchor_index.get(&idx).map(String::as_str),
                    );
                }
                ResponseItem::LocalShellCall {
                    id,
                    call_id,
                    status,
                    action,
                    ..
                } => {
                    let call_id = call_id
                        .clone()
                        .or_else(|| id.as_ref().map(ToString::to_string))
                        .unwrap_or_else(|| format!("local-shell-{idx}"));
                    push_tool_call(
                        &mut messages,
                        json!({
                            "id": call_id,
                            "type": "local_shell_call",
                            "status": status,
                            "action": action
                        }),
                        reasoning_by_anchor_index.get(&idx).map(String::as_str),
                    );
                }
                ResponseItem::FunctionCallOutput {
                    call_id, output, ..
                } => {
                    let Some(call_id) = call_id else {
                        continue;
                    };
                    let content = chat_tool_output(output);
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": content
                    }));
                }
                ResponseItem::CustomToolCall {
                    call_id,
                    name,
                    input,
                    ..
                } => {
                    push_tool_call(
                        &mut messages,
                        json!({
                            "id": call_id,
                            "type": "custom",
                            "custom": {"name": name, "input": input}
                        }),
                        reasoning_by_anchor_index.get(&idx).map(String::as_str),
                    );
                }
                ResponseItem::CustomToolCallOutput {
                    call_id, output, ..
                } => {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": chat_tool_output(output)
                    }));
                }
                _ => {}
            }
        }

        let mut headers = build_session_headers(self.conversation_id.clone(), self.conversation_id);
        if let Some(subagent) = subagent_header(&self.session_source) {
            insert_header(&mut headers, "x-openai-subagent", &subagent);
        }
        Ok(ChatRequest {
            body: json!({
                "model": self.model,
                "messages": messages,
                "stream": true,
                "tools": self.tools,
            }),
            headers,
        })
    }
}

fn chat_tool_output(output: &FunctionCallOutputPayload) -> Value {
    if let Some(items) = output.content_items() {
        return json!(
            items
                .iter()
                .filter_map(|item| match item {
                    FunctionCallOutputContentItem::InputText { text } => {
                        Some(json!({"type": "text", "text": text}))
                    }
                    FunctionCallOutputContentItem::InputImage {
                        image: ImageReference::Inline { image_url },
                        ..
                    } => Some(json!({"type": "image_url", "image_url": {"url": image_url}})),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
    }
    json!(output.text_content().unwrap_or_default())
}

fn push_tool_call(messages: &mut Vec<Value>, tool_call: Value, reasoning: Option<&str>) {
    if let Some(Value::Object(message)) = messages.last_mut()
        && message.get("role").and_then(Value::as_str) == Some("assistant")
        && message
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some()
    {
        message["tool_calls"]
            .as_array_mut()
            .expect("checked above")
            .push(tool_call);
        if let Some(reasoning) = reasoning {
            match message.get_mut("reasoning") {
                Some(Value::String(existing)) => {
                    if !existing.is_empty() {
                        existing.push('\n');
                    }
                    existing.push_str(reasoning);
                }
                _ => {
                    message.insert("reasoning".to_string(), json!(reasoning));
                }
            }
        }
        return;
    }

    let mut message = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [tool_call]
    });
    if let Some(reasoning) = reasoning {
        message["reasoning"] = json!(reasoning);
    }
    messages.push(message);
}
