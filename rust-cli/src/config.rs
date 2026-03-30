use clap::{Parser, Subcommand};

/// A2A endpoint configuration.
pub static A2A_ENDPOINT: &str = "https://insert-your-endpoint-url";
pub static A2A_SCOPES: &[&str] = &["https://graph.microsoft.com/.default"];
pub static A2A_AUTHORITY: &str = "https://login.microsoftonline.com/ca24a1b0-4df5-4b45-8126-22d617eb8f90";

/// A2A CLI — Interactive A2A session
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Auth token (JWT). Omit to use cached login or device code flow.
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// Azure AD application (client) ID
    #[arg(long, global = true, env = "A2A_APP_ID", default_value = "a668445b-6bb2-40f7-9aa6-87331e80db51")]
    pub appid: Option<String>,

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

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Log in to M365 via device code flow and cache the token
    Login,
    /// Clear cached M365 tokens
    Logout,
    /// Show current auth status
    Status,
}
