use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use colored::Colorize;
use msal::broker::BrokerTokenRequest;
use msal::request::{DeviceCodeRequest, RefreshTokenRequest};
use msal::{AuthenticationResult, Configuration, PublicClientApplication};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Session Persistence ────────────────────────────────────────────────

/// On-disk session state for cross-invocation token persistence.
/// When brokered auth is active, the OS manages tokens; this stores
/// the last-known access token and account info for status display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStore {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix timestamp when the access token expires.
    pub expires_at: i64,
    pub account: Option<String>,
}

fn store_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".a2a-cli")
}

fn store_path() -> PathBuf {
    store_dir().join("token_cache.json")
}

impl SessionStore {
    pub fn load() -> Option<Self> {
        let data = std::fs::read_to_string(store_path()).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(&self) -> Result<()> {
        let dir = store_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create {}", dir.display()))?;
        let path = store_path();
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, &data)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
        }
        Ok(())
    }

    pub fn clear() -> Result<()> {
        let path = store_path();
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
        }
        Ok(())
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() >= self.expires_at
    }

    pub fn expires_within(&self, secs: i64) -> bool {
        (self.expires_at - chrono::Utc::now().timestamp()) < secs
    }
}

// ── AuthManager ────────────────────────────────────────────────────────

/// Manages the full token lifecycle using MSAL: cache → broker → refresh → device code.
pub struct AuthManager {
    app: PublicClientApplication,
    client_id: String,
    scopes: Vec<String>,
    session: Option<SessionStore>,
}

impl AuthManager {
    pub async fn new(
        client_id: &str,
        scopes: &[&str],
        authority: &str,
        redirect_uri: Option<&str>,
        _account_hint: Option<&str>,
    ) -> Result<Self> {
        let config = Configuration::builder(client_id)
            .authority(authority)
            .build();
        let app =
            PublicClientApplication::new(config).map_err(|e| anyhow::anyhow!("{e}"))?;

        #[cfg(target_os = "macos")]
        {
            let broker_result = if let Some(uri) = redirect_uri {
                msal::broker::macos::MacOsBroker::new(uri, authority)
            } else {
                msal::broker::macos::MacOsBroker::new_for_cli(authority)
            };
            match broker_result {
                Ok(broker) => {
                    eprintln!("  {}", "macOS SSO broker detected".dimmed());
                    app.set_broker(Box::new(broker)).await;
                }
                Err(e) => {
                    eprintln!("  {} {}", "macOS SSO broker unavailable:".yellow(), e);
                }
            }
        }

        #[cfg(target_os = "windows")]
        if let Ok(broker) = msal::broker::wam::WamBroker::new() {
            app.set_broker(Box::new(broker)).await;
        }

        let session = SessionStore::load().filter(|s| s.client_id == client_id);

        Ok(Self {
            app,
            client_id: client_id.to_string(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            session,
        })
    }

    /// Get a valid access token. Tries (in order):
    /// 1. Cached token if still valid
    /// 2. Broker interactive (if broker available — OS decides whether to show UI)
    /// 3. Silent refresh via refresh_token
    /// 4. Device code flow (interactive)
    pub async fn get_token(&mut self, verbosity: u8) -> Result<String> {
        // 1. Cached and still fresh (>60s remaining)
        if let Some(ref session) = self.session {
            if !session.expires_within(60) {
                if verbosity >= 2 {
                    eprintln!("  {}", "Using cached access token".dimmed());
                }
                return Ok(session.access_token.clone());
            }
        }

        // 2. Try broker
        if self.app.is_broker_available().await {
            if verbosity >= 1 {
                eprintln!("  {}", "Acquiring token via broker...".dimmed());
            }
            match self.acquire_via_broker().await {
                Ok(token) => return Ok(token),
                Err(e) => {
                    if verbosity >= 1 {
                        eprintln!("  {} {}", "Broker failed:".yellow(), e);
                    }
                }
            }
        }

        // 3. Try refresh token
        if let Some(rt) = self.session.as_ref().and_then(|s| s.refresh_token.clone()) {
            if verbosity >= 1 {
                eprintln!("  {}", "Refreshing token...".dimmed());
            }
            match self.refresh(&rt).await {
                Ok(token) => return Ok(token),
                Err(e) => {
                    if verbosity >= 1 {
                        eprintln!("  {} {}", "Refresh failed:".yellow(), e);
                    }
                }
            }
        }

        // 4. Device code flow
        if verbosity >= 1 {
            eprintln!("  {}", "Starting device code login...".dimmed());
        }
        self.device_code_flow().await
    }

    /// Ensure the token is fresh before a request. Refreshes silently if needed.
    pub async fn ensure_fresh(&mut self, verbosity: u8) -> Result<String> {
        if let Some(ref session) = self.session {
            if !session.expires_within(300) {
                return Ok(session.access_token.clone());
            }
        }

        // Try broker silent
        if self.app.is_broker_available().await {
            if let Ok(token) = self.acquire_via_broker().await {
                return Ok(token);
            }
        }

        // Try refresh
        if let Some(rt) = self.session.as_ref().and_then(|s| s.refresh_token.clone()) {
            if verbosity >= 2 {
                eprintln!("  {}", "Silently refreshing token...".dimmed());
            }
            match self.refresh(&rt).await {
                Ok(token) => return Ok(token),
                Err(e) => {
                    if verbosity >= 1 {
                        eprintln!("  {} {}", "Silent refresh failed:".yellow(), e);
                    }
                }
            }
        }

        // Return whatever we have; caller will get a 401 if it's truly dead
        if let Some(ref session) = self.session {
            Ok(session.access_token.clone())
        } else {
            anyhow::bail!("No token available. Run `login` to authenticate.");
        }
    }

    pub fn cached_account(&self) -> Option<&str> {
        self.session.as_ref()?.account.as_deref()
    }

    // ── Broker ─────────────────────────────────────────────────────

    async fn acquire_via_broker(&mut self) -> Result<String> {
        let req = BrokerTokenRequest {
            scopes: self.scopes.clone(),
            account: None,
            claims: None,
            correlation_id: None,
            window_handle: None,
            authentication_scheme: Default::default(),
            pop_params: None,
        };
        let result = self
            .app
            .acquire_token_interactive(req)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.persist(&result, None)?;
        Ok(result.access_token)
    }

    // ── Refresh ────────────────────────────────────────────────────

    async fn refresh(&mut self, refresh_token: &str) -> Result<String> {
        let req = RefreshTokenRequest {
            refresh_token: refresh_token.to_string(),
            scopes: self.scopes_with_offline(),
            claims: None,
            correlation_id: None,
        };
        let result = self
            .app
            .acquire_token_by_refresh_token(req)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // Keep old refresh token if server didn't rotate
        self.persist(&result, Some(refresh_token))?;
        Ok(result.access_token)
    }

    // ── Device Code ────────────────────────────────────────────────

    async fn device_code_flow(&mut self) -> Result<String> {
        let req = DeviceCodeRequest {
            scopes: self.scopes_with_offline(),
            claims: None,
            correlation_id: None,
        };
        let result = self
            .app
            .acquire_token_by_device_code(req, |info| {
                println!("\n  {}", info.message.yellow().bold());
                println!(
                    "  Code: {}  URL: {}\n",
                    info.user_code.green().bold(),
                    info.verification_uri.underline()
                );
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.persist(&result, None)?;
        Ok(result.access_token)
    }

    // ── Helpers ─────────────────────────────────────────────────────

    fn scopes_with_offline(&self) -> Vec<String> {
        let mut scopes = self.scopes.clone();
        if !scopes.iter().any(|s| s == "offline_access") {
            scopes.push("offline_access".to_string());
        }
        scopes
    }

    fn persist(
        &mut self,
        result: &AuthenticationResult,
        fallback_rt: Option<&str>,
    ) -> Result<()> {
        let account = result
            .account
            .as_ref()
            .map(|a| a.username.clone())
            .or_else(|| self.session.as_ref().and_then(|s| s.account.clone()));
        let refresh_token = result
            .refresh_token
            .clone()
            .or_else(|| fallback_rt.map(|s| s.to_string()));
        let session = SessionStore {
            client_id: self.client_id.clone(),
            access_token: result.access_token.clone(),
            refresh_token,
            expires_at: result.expires_on,
            account,
        };
        session.save()?;
        self.session = Some(session);
        Ok(())
    }
}

// ── JWT Helpers ──────────────────────────────────────────────────────────

/// Decode and display JWT token claims (header + payload) without verification.
pub fn decode_token(token: &str) {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        eprintln!("{}", "  Token is not a valid JWT (expected 3 parts)".red());
        return;
    }

    if let Ok(header) = decode_jwt_part(parts[0]) {
        println!("  {}", "Header:".dimmed());
        print_json_pretty(&header, "    ");
    }

    if let Ok(payload) = decode_jwt_part(parts[1]) {
        println!("  {}", "Payload:".dimmed());
        print_json_pretty(&payload, "    ");

        if let Some(exp) = payload.get("exp").and_then(|v| v.as_i64()) {
            let now = chrono::Utc::now().timestamp();
            let remaining = exp - now;
            if remaining > 0 {
                println!(
                    "  {} {}m {}s",
                    "Expires in:".dimmed(),
                    remaining / 60,
                    remaining % 60
                );
            } else {
                println!("  {}", "TOKEN EXPIRED".red().bold());
            }
        }
    }
}

fn decode_jwt_part(part: &str) -> Result<serde_json::Value> {
    let decoded = URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .context("base64 decode")?;
    serde_json::from_slice(&decoded).context("JSON parse")
}

fn print_json_pretty(val: &serde_json::Value, indent: &str) {
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            let v_str = match v {
                serde_json::Value::String(s) => {
                    if s.len() > 80 {
                        let truncated: String = s.chars().take(80).collect();
                        format!("{truncated}...")
                    } else {
                        s.clone()
                    }
                }
                other => other.to_string(),
            };
            println!("{}{}: {}", indent, k.cyan(), v_str);
        }
    }
}
