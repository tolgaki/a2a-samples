use a2a_rs_client::{A2aClient, ClientConfig, ProtocolVersion};
use a2a_rs_core::{Message, SendMessageConfiguration, SendMessageResult, StreamingMessageResult};
use anyhow::Result;
use futures_util::Stream;
use std::pin::Pin;
use std::time::Duration;

/// Thin wrapper around `A2aClient` configured for A2A v0.3.
///
/// Uses `endpoint_url` to bypass agent card discovery. Auth token is passed per-request.
pub struct A2ASessionClient {
    client: A2aClient,
    token: String,
}

impl A2ASessionClient {
    pub fn new(endpoint: &str, token: &str) -> Result<Self> {
        let http_client = build_http_client()?;
        let client = A2aClient::new(ClientConfig {
            server_url: endpoint.to_string(),
            endpoint_url: Some(endpoint.to_string()),
            http_client: Some(http_client),
            protocol_version: ProtocolVersion::V0_3,
            ..Default::default()
        })?;
        Ok(Self {
            client,
            token: token.to_string(),
        })
    }

    /// Update the bearer token. Cheap — no client rebuild needed since
    /// `A2aClient` accepts the token per-request.
    pub fn update_token(&mut self, token: &str) {
        self.token = token.to_string();
    }

    /// Send a message (sync, blocking).
    pub async fn send_message(
        &self,
        message: Message,
        configuration: Option<SendMessageConfiguration>,
    ) -> Result<SendMessageResult> {
        self.client
            .send_message(message, Some(&self.token), configuration)
            .await
    }

    /// Send a streaming message (SSE).
    pub async fn send_message_streaming(
        &self,
        message: Message,
        configuration: Option<SendMessageConfiguration>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamingMessageResult>> + Send>>> {
        self.client
            .send_message_streaming(message, Some(&self.token), configuration)
            .await
    }
}

fn build_http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?)
}
