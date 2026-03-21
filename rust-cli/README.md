# Work IQ A2A CLI

A Rust command-line tool for interactive [A2A (Agent-to-Agent)](https://google.github.io/A2A/) sessions with [Microsoft Work IQ](https://aka.ms/workiq) via the Microsoft Graph API.

## Features

- **Interactive REPL** with multi-turn conversation support
- **Streaming mode** (SSE) for real-time response display
- **OAuth 2.0 device code flow** with automatic token caching and silent refresh
- **Configurable verbosity** for debugging wire-level details

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.70+)
- A Microsoft 365 account with access to Work IQ
- An Azure AD app registration with the `https://graph.microsoft.com/.default` scope and device code flow enabled. A default app ID is provided for convenience.

## Quick Start

```bash
# Build
cargo build --release

# Login and start chatting
cargo run

# First run will prompt you with a device code to sign in:
#   To sign in, use a web browser to open https://microsoft.com/devicelogin
#   and enter the code XXXXXXXXX to authenticate.
```

After authentication, your token is cached at `~/.workiq/token_cache.json` and refreshed automatically.

## Usage

```
workiq-a2a [OPTIONS] [COMMAND]
```

### Commands

| Command  | Description                              |
|----------|------------------------------------------|
| `login`  | Authenticate via device code flow        |
| `logout` | Clear cached tokens                      |
| `status` | Show current auth status and token info  |

If no command is given, the CLI enters the interactive REPL.

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `--token <JWT>` | Provide a pre-authenticated JWT token | — |
| `--appid <ID>` | Azure AD application (client) ID | `a668445b-...` |
| `--account <EMAIL>` | M365 account hint (e.g. `user@contoso.com`) | — |
| `--stream` | Enable streaming mode (SSE) | `false` |
| `-v, --verbosity <N>` | Output detail: 0=quiet, 1=normal, 2=wire | `1` |
| `--show-token` | Include raw token in output | `false` |

The `--appid` flag can also be set via the `WORKIQ_APP_ID` environment variable.

### Examples

```bash
# Streaming mode
cargo run -- --stream

# Specify account for device code flow
cargo run -- --account user@contoso.com

# Use a pre-obtained token (useful in CI/automation)
cargo run -- --token "eyJ0eXAi..."

# Verbose wire-level output
cargo run -- -v 2 --show-token

# Check auth status
cargo run -- status
```

### REPL Commands

Once in the interactive session, type your message and press Enter. Special inputs:

- `quit` or `exit` — end the session
- `Ctrl+C` — interrupt

## Architecture

```
src/
├── main.rs      # CLI entry point, REPL loop, output formatting
├── config.rs    # Argument parsing (clap) and WorkIQ endpoint constants
├── auth.rs      # OAuth 2.0 device code flow, token caching, silent refresh
└── a2a.rs       # A2A HTTP client (sync and streaming via JSON-RPC 2.0)
```

### Authentication Flow

1. **Cached token** — use if still valid (>60s remaining)
2. **Silent refresh** — exchange refresh token for a new access token
3. **Device code flow** — interactive login as a fallback

Tokens are cached at `~/.workiq/token_cache.json` with `0600` permissions.

## License

[MIT](../LICENSE)
