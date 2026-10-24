use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::StreamResponse;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

pub(crate) fn spawn_chat_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    _turn_state: Option<Arc<OnceLock<String>>>,
) -> ResponseStream {
    let upstream_request_id = stream_response
        .headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(async move {
        process_chat_sse(stream_response.bytes, tx_event, idle_timeout, telemetry).await;
    });
    ResponseStream {
        rx_event,
        upstream_request_id,
        interrupt: None,
    }
}

/// Converts Chat Completions SSE deltas into Codex response events.
pub async fn process_chat_sse<S>(
    stream: S,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
) where
    S: Stream<Item = Result<bytes::Bytes, codex_client::TransportError>> + Unpin,
{
    let mut stream = stream.eventsource();

    #[derive(Default, Debug)]
    struct ToolCallState {
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    }

    let mut tool_calls: HashMap<usize, ToolCallState> = HashMap::new();
    let mut tool_call_order = Vec::new();
    let mut tool_call_order_seen = HashSet::new();
    let mut tool_call_index_by_id = HashMap::new();
    let mut next_tool_call_index = 0usize;
    let mut last_tool_call_index = None;
    let mut assistant_item: Option<ResponseItem> = None;
    let mut reasoning_item: Option<ResponseItem> = None;
    let mut completed_sent = false;

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(telemetry) = telemetry.as_ref() {
            telemetry.on_sse_poll(&response, start.elapsed());
        }
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(error))) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(error.to_string())))
                    .await;
                return;
            }
            Ok(None) => {
                if !completed_sent {
                    flush_and_complete(
                        &tx_event,
                        &mut reasoning_item,
                        &mut assistant_item,
                        /*end_turn*/ true,
                    )
                    .await;
                }
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(
                        "idle timeout waiting for Chat Completions SSE".into(),
                    )))
                    .await;
                return;
            }
        };

        trace!("Chat Completions SSE event: {}", sse.data);
        let data = sse.data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" || data == "DONE" {
            if !completed_sent {
                flush_and_complete(
                    &tx_event,
                    &mut reasoning_item,
                    &mut assistant_item,
                    /*end_turn*/ true,
                )
                .await;
            }
            return;
        }

        let value: serde_json::Value = match serde_json::from_str(data) {
            Ok(value) => value,
            Err(error) => {
                debug!(%error, data, "failed to parse Chat Completions SSE event");
                continue;
            }
        };
        let Some(choices) = value.get("choices").and_then(|choices| choices.as_array()) else {
            continue;
        };

        for choice in choices {
            if let Some(delta) = choice.get("delta") {
                collect_reasoning(&tx_event, &mut reasoning_item, delta.get("reasoning")).await;
                if let Some(content) = delta.get("content") {
                    collect_content(&tx_event, &mut assistant_item, content).await;
                }
                if let Some(tool_call_values) =
                    delta.get("tool_calls").and_then(|value| value.as_array())
                {
                    for tool_call in tool_call_values {
                        let mut index = tool_call
                            .get("index")
                            .and_then(serde_json::Value::as_u64)
                            .map(|index| index as usize);
                        let call_id = tool_call.get("id").and_then(|value| value.as_str());
                        if let Some(existing) = call_id.and_then(|id| tool_call_index_by_id.get(id))
                        {
                            index = Some(*existing);
                        }
                        if index.is_none() && call_id.is_none() {
                            index = last_tool_call_index;
                        }
                        let index = index.unwrap_or_else(|| {
                            while tool_calls.contains_key(&next_tool_call_index) {
                                next_tool_call_index += 1;
                            }
                            let index = next_tool_call_index;
                            next_tool_call_index += 1;
                            index
                        });
                        let state = tool_calls.entry(index).or_default();
                        if tool_call_order_seen.insert(index) {
                            tool_call_order.push(index);
                        }
                        if let Some(id) = call_id {
                            state.id.get_or_insert_with(|| id.to_string());
                            tool_call_index_by_id.entry(id.to_string()).or_insert(index);
                        }
                        if let Some(function) = tool_call.get("function") {
                            if let Some(name) = function.get("name").and_then(|name| name.as_str())
                                && !name.is_empty()
                            {
                                state.name.get_or_insert_with(|| name.to_string());
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(|args| args.as_str())
                            {
                                state.arguments.push_str(arguments);
                            }
                        }
                        last_tool_call_index = Some(index);
                    }
                }
            }

            if let Some(message) = choice.get("message") {
                collect_reasoning(&tx_event, &mut reasoning_item, message.get("reasoning")).await;
            }

            match choice.get("finish_reason").and_then(|value| value.as_str()) {
                Some("stop") => {
                    flush_and_complete(
                        &tx_event,
                        &mut reasoning_item,
                        &mut assistant_item,
                        /*end_turn*/ true,
                    )
                    .await;
                    completed_sent = true;
                }
                Some("length") => {
                    let _ = tx_event.send(Err(ApiError::ContextWindowExceeded)).await;
                    return;
                }
                Some("tool_calls") => {
                    if let Some(reasoning) = reasoning_item.take() {
                        let _ = tx_event
                            .send(Ok(ResponseEvent::OutputItemDone(reasoning)))
                            .await;
                    }
                    for index in tool_call_order.drain(..) {
                        let Some(state) = tool_calls.remove(&index) else {
                            continue;
                        };
                        tool_call_order_seen.remove(&index);
                        let Some(name) = state.name else {
                            debug!(index, "skipping Chat Completions tool call without a name");
                            continue;
                        };
                        let _ = tx_event
                            .send(Ok(ResponseEvent::OutputItemDone(
                                ResponseItem::FunctionCall {
                                    encrypted_content: None,
                                    status: None,
                                    id: None,
                                    name,
                                    namespace: None,
                                    arguments: state.arguments,
                                    encrypted_function_args: None,
                                    call_id: state
                                        .id
                                        .unwrap_or_else(|| format!("tool-call-{index}")),
                                    internal_chat_message_metadata_passthrough: None,
                                },
                            )))
                            .await;
                    }
                    if let Some(assistant) = assistant_item.take() {
                        let _ = tx_event
                            .send(Ok(ResponseEvent::OutputItemDone(assistant)))
                            .await;
                    }
                    let _ = tx_event
                        .send(Ok(ResponseEvent::Completed {
                            response_id: String::new(),
                            token_usage: None,
                            usage_metadata: None,
                            end_turn: Some(false),
                        }))
                        .await;
                    completed_sent = true;
                }
                _ => {}
            }
        }
    }
}

async fn flush_and_complete(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    reasoning_item: &mut Option<ResponseItem>,
    assistant_item: &mut Option<ResponseItem>,
    end_turn: bool,
) {
    if let Some(reasoning) = reasoning_item.take() {
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemDone(reasoning)))
            .await;
    }
    if let Some(assistant) = assistant_item.take() {
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemDone(assistant)))
            .await;
    }
    let _ = tx_event
        .send(Ok(ResponseEvent::Completed {
            response_id: String::new(),
            token_usage: None,
            usage_metadata: None,
            end_turn: Some(end_turn),
        }))
        .await;
}

async fn collect_content(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    assistant_item: &mut Option<ResponseItem>,
    content: &serde_json::Value,
) {
    if let Some(parts) = content.as_array() {
        for part in parts {
            if let Some(text) = part.get("text").and_then(|text| text.as_str()) {
                append_assistant_text(tx_event, assistant_item, text.to_string()).await;
            }
        }
    } else if let Some(text) = content.as_str() {
        append_assistant_text(tx_event, assistant_item, text.to_string()).await;
    }
}

async fn collect_reasoning(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    reasoning_item: &mut Option<ResponseItem>,
    reasoning: Option<&serde_json::Value>,
) {
    let Some(reasoning) = reasoning else {
        return;
    };
    let text = reasoning.as_str().or_else(|| {
        reasoning
            .get("text")
            .and_then(|text| text.as_str())
            .or_else(|| reasoning.get("content").and_then(|text| text.as_str()))
    });
    if let Some(text) = text {
        append_reasoning_text(tx_event, reasoning_item, text.to_string()).await;
    }
}

async fn append_assistant_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    assistant_item: &mut Option<ResponseItem>,
    text: String,
) {
    if assistant_item.is_none() {
        let item = ResponseItem::Message {
            encrypted_content: None,
            status: None,
            id: None,
            role: "assistant".to_string(),
            content: vec![],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };
        *assistant_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }
    if let Some(ResponseItem::Message { content, .. }) = assistant_item {
        content.push(ContentItem::OutputText {
            text: text.clone(),
            annotations: None,
            logprobs: None,
        });
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputTextDelta(text)))
            .await;
    }
}

async fn append_reasoning_text(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    reasoning_item: &mut Option<ResponseItem>,
    text: String,
) {
    if reasoning_item.is_none() {
        let item = ResponseItem::Reasoning {
            id: None,
            summary: Vec::new(),
            content: Some(Vec::new()),
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        };
        *reasoning_item = Some(item.clone());
        let _ = tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await;
    }
    if let Some(ResponseItem::Reasoning {
        content: Some(content),
        ..
    }) = reasoning_item
    {
        let content_index = content.len() as i64;
        content.push(ReasoningItemContent::ReasoningText { text: text.clone() });
        let _ = tx_event
            .send(Ok(ResponseEvent::ReasoningContentDelta {
                delta: text,
                content_index,
            }))
            .await;
    }
}
