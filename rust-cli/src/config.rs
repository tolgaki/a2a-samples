use clap::{Parser, Subcommand};

/// A2A CLI — Interactive A2A session
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// A2A endpoint URL
    #[arg(long, global = true, env = "A2A_ENDPOINT")]
    pub endpoint: Option<String>,

    /// Auth token (JWT). Omit to use cached login or device code flow.
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// Azure AD application (client) ID
    #[arg(long, global = true, env = "A2A_APP_ID")]
    pub appid: Option<String>,

    /// Azure AD tenant ID (e.g. "common", "organizations", or a specific tenant GUID)
    #[arg(long, global = true, env = "A2A_TENANT_ID", default_value = "common")]
    pub tenant_id: String,

    /// Azure AD authority URL — overrides --tenant-id
    /// (e.g. https://login.microsoftonline.com/<tenant-id>)
    #[arg(long, global = true, env = "A2A_AUTHORITY")]
    pub authority: Option<String>,

    /// OAuth scopes (comma-separated)
    #[arg(long, global = true, env = "A2A_SCOPES", value_delimiter = ',',
           default_values_t = vec!["https://graph.microsoft.com/.default".to_string()])]
    pub scopes: Vec<String>,

    /// Redirect URI for brokered auth (default: msauth://com.microsoft.CompanyPortal)
    #[arg(long, global = true, env = "A2A_REDIRECT_URI")]
    pub redirect_uri: Option<String>,

    /// M365 account hint (e.g. user@contoso.com)
    #[arg(long, global = true)]
    pub account: Option<String>,

    /// Enable streaming mode (SSE)
    #[arg(long, global = true, default_value_t = false)]
    pub stream: bool,

    /// Verbosity level (0=quiet, 1=normal, 2=wire)
    #[arg(short, long, global = true, default_value_t = 1)]
    pub verbosity: u8,

    /// Show raw token in output
    #[arg(long, global = true, default_value_t = false)]
    pub show_token: bool,
}

impl Cli {
    /// Resolve the authority URL from --authority or --tenant-id.
    pub fn authority(&self) -> String {
        self.authority.clone().unwrap_or_else(|| {
            format!("https://login.microsoftonline.com/{}", self.tenant_id)
        })
    }

}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Log in to M365 via device code flow and cache the token
    Login,
    /// Clear cached M365 tokens
    Logout,
    /// Show current auth status
    Status,
}
