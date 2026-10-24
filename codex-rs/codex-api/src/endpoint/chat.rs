use crate::auth::SharedAuthProvider;
use crate::common::ResponseStream;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::requests::ChatRequest;
use crate::requests::headers::build_session_headers;
use crate::requests::headers::insert_header;
use crate::sse::chat::spawn_chat_stream;
use crate::telemetry::SseTelemetry;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use codex_protocol::protocol::SessionSource;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use std::sync::Arc;
use std::sync::OnceLock;

pub struct ChatClient<T: HttpTransport> {
    session: EndpointSession<T>,
    sse_telemetry: Option<Arc<dyn SseTelemetry>>,
}

#[derive(Default)]
pub struct ChatOptions {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub session_source: Option<SessionSource>,
    pub extra_headers: HeaderMap,
    pub turn_state: Option<Arc<OnceLock<String>>>,
}

impl<T: HttpTransport> ChatClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
            sse_telemetry: None,
        }
    }

    pub fn with_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
        sse: Option<Arc<dyn SseTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
            sse_telemetry: sse,
        }
    }

    pub async fn stream_request(
        &self,
        request: ChatRequest,
        options: ChatOptions,
    ) -> Result<ResponseStream, ApiError> {
        let ChatOptions {
            session_id,
            thread_id,
            session_source,
            mut extra_headers,
            turn_state,
        } = options;
        extra_headers.extend(request.headers);
        if let Some(thread_id) = thread_id.as_ref() {
            insert_header(&mut extra_headers, "x-client-request-id", thread_id);
        }
        extra_headers.extend(build_session_headers(session_id, thread_id));
        if let Some(subagent) = crate::requests::headers::subagent_header(&session_source) {
            insert_header(&mut extra_headers, "x-openai-subagent", &subagent);
        }
        let body = EncodedJsonBody::encode(&request.body)
            .map_err(|error| ApiError::Stream(format!("failed to encode chat request: {error}")))?;
        let stream_response = self
            .session
            .stream_encoded_json_with(
                Method::POST,
                "/chat/completions",
                extra_headers,
                Some(body),
                |request| {
                    request.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                },
            )
            .await?;
        Ok(spawn_chat_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
            turn_state,
        ))
    }
}
