use a2a_rs_core::{
    JsonRpcRequest, JsonRpcResponse, Message, SendMessageConfiguration, SendMessageRequest,
    SendMessageResult, StreamingMessageResult,
};
use anyhow::{anyhow, Context, Result};
use futures_util::Stream;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::pin::Pin;

/// A2A client for WorkIQ using a2a-rs-core types with direct HTTP transport.
///
/// We use our own transport layer because a2a-rs-client's A2aClient requires
/// agent card discovery and doesn't support custom default headers.
pub struct WorkIQClient {
    http: reqwest::Client,
    endpoint: String,
}

impl WorkIQClient {
    pub fn new(endpoint: &str, token: &str, extra_headers: &[(&str, &str)]) -> Result<Self> {
        Ok(Self {
            http: build_http_client(token, extra_headers)?,
            endpoint: endpoint.to_string(),
        })
    }

    pub fn update_token(&mut self, token: &str, extra_headers: &[(&str, &str)]) -> Result<()> {
        self.http = build_http_client(token, extra_headers)?;
        Ok(())
    }

    /// Send a message (sync, blocking).
    pub async fn send_message(
        &self,
        message: Message,
        configuration: Option<SendMessageConfiguration>,
    ) -> Result<SendMessageResult> {
        let rpc = self.build_rpc("message/send", &message, configuration)?;

        let resp = self
            .http
            .post(&self.endpoint)
            .json(&rpc)
            .send()
            .await
            .context("Failed to send A2A request")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let rpc_resp: JsonRpcResponse = resp.json().await.context("Failed to parse JSON-RPC")?;
        if let Some(err) = rpc_resp.error {
            anyhow::bail!("JSON-RPC error {}: {}", err.code, err.message);
        }

        let val = rpc_resp.result.context("Server returned no result")?;
        Ok(serde_json::from_value(val).context("Failed to parse SendMessageResult")?)
    }

    /// Send a streaming message (SSE).
    pub async fn send_message_streaming(
        &self,
        message: Message,
        configuration: Option<SendMessageConfiguration>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamingMessageResult>> + Send>>> {
        let rpc = self.build_rpc("message/stream", &message, configuration)?;

        let resp = self
            .http
            .post(&self.endpoint)
            .json(&rpc)
            .send()
            .await
            .context("Failed to send streaming A2A request")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let stream = sse_stream(resp);
        Ok(Box::pin(stream))
    }

    fn build_rpc(
        &self,
        method: &str,
        message: &Message,
        configuration: Option<SendMessageConfiguration>,
    ) -> Result<JsonRpcRequest> {
        let params = SendMessageRequest {
            tenant: None,
            message: message.clone(),
            configuration,
            metadata: None,
        };
        Ok(JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params: Some(serde_json::to_value(&params)?),
            id: serde_json::json!(1),
        })
    }
}

/// Parse an SSE response into a stream of `StreamingMessageResult`.
fn sse_stream(
    resp: reqwest::Response,
) -> impl Stream<Item = Result<StreamingMessageResult>> + Send {
    async_stream::try_stream! {
        use tokio_stream::StreamExt;

        let mut bytes_stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = bytes_stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data.is_empty() {
                        continue;
                    }

                    let rpc_resp: JsonRpcResponse = serde_json::from_str(data)?;

                    if let Some(err) = rpc_resp.error {
                        Err(anyhow!("Server error {}: {}", err.code, err.message))?;
                    }

                    if let Some(result) = rpc_resp.result {
                        let event: StreamingMessageResult = serde_json::from_value(result)?;
                        yield event;
                    }
                }
            }
        }
    }
}

fn build_http_client(token: &str, extra_headers: &[(&str, &str)]) -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", token))?,
    );
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    for (k, v) in extra_headers {
        let name: HeaderName = k.parse()?;
        let val = HeaderValue::from_str(v)?;
        headers.insert(name, val);
    }
    Ok(reqwest::Client::builder()
        .default_headers(headers)
        .build()?)
}
