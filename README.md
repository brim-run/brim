<p align="center">
  <a href="https://www.buymeacoffee.com/alexandrughinea" title="BRIM (Brew Recipe Install Manager)">
    <img src=".fixtures/logo.svg" alt="BRIM (Brew Recipe Install Manager)" width="256px">
  </a>
</p>

# BRIM
The declarative package layer for Homebrew. Define your packages in a recipe, host it anywhere, and let brim handle sync, integrity, and installation with a TUI or headless, in a single binary or an MCP server.
[Website](https://brim.run)

[![Rust](https://github.com/brim-run/brim/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/brim-run/brim/actions/workflows/rust.yml)

## Features

- TUI with real-time progress (ratatui)
- Recipe chaining (multiple local/remote recipe files)
- Sync analysis (compare installed vs recipe)
- Dry-run mode
- Parallel downloads, sequential installs
- Clean removal with dependency cleanup
- Cask and formula support
- Recipe lockfile (integrity check; update when recipe changes)
- MCP server (stdio) for Cursor, Claude, and other clients — `brim-mcp`
- Headless subcommands for scripts and CI: `brim install`, `brim sync`, `brim remove`, `brim update-lock`
- Webhook payload with optional machine ID

## Installation

**Quick (macOS/Linux):**
```bash
curl -fsSL https://raw.githubusercontent.com/brim-run/brim/main/install.sh | bash
```
Installs `brim` and `brim-mcp` to `~/.local/bin`. Add that directory to PATH if needed.

**Manual:** Download binaries from [releases](https://github.com/brim-run/brim/releases). Extract and move `brim` and `brim-mcp` to a directory on PATH.

**From source:**
```bash
git clone https://github.com/brim-run/brim && cd brim
cargo build --release
# MCP binary (optional)
cargo build --release --features mcp --bin brim-mcp
```
Binaries: `target/release/brim`, `target/release/brim-mcp`.

## CLI (brim)

### TUI mode

```bash
brim [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--url <URL>` | Recipe file(s); comma-separated or repeated |
| `--list` | List installed Homebrew packages |
| `--remove` | Interactive removal (TUI) |
| `--sync` | Compare installed vs recipe; show to_install / to_remove / in_sync |
| `--parallel` | Parallel downloads, sequential installs |
| `--dry-run` | Preview only |
| `--webhook <URL>` | POST JSON summary to URL after install |
| `--webhook-machine-id <ID>` | Machine ID in webhook payload (default: hostname or "unknown") |
| `--autoquit <SECONDS>` | Auto-quit summary after N seconds |

Examples:
```bash
brim --url="packages.json"
brim --url="base.json,extras.json" --parallel
brim --sync --url="packages.json"
brim --remove --dry-run
brim --url="packages.json" --webhook="https://example.com/hook"
```

### Headless subcommands (scripts and CI)

Same logic as the MCP tools — no TUI, plain output, exits when done:

```bash
brim install --urls recipe.json
brim install --urls base.json,extras.json --parallel
brim install --urls recipe.json --webhook="https://example.com/hook"
brim sync    --urls recipe.json
brim remove  --packages wget,jq
brim remove  --urls recipe.json
brim trim    --urls recipe.json
brim update-lock --urls recipe.json
```

| Subcommand | Required | Optional | Description |
|------------|----------|----------|-------------|
| `install` | `--urls` | `--parallel`, `--webhook`, `--webhook-machine-id` | Install packages from recipe |
| `sync` | `--urls` | — | Show to_install / to_remove / in_sync |
| `remove` | `--packages` OR `--urls` | — | Remove by name, or remove all packages listed in the recipe (opposite of install) |
| `trim` | `--urls` | — | Remove installed packages not in the recipe (clean up extras) |
| `update-lock` | `--urls` | — | Accept recipe changes and update lockfile |

## MCP server (brim-mcp)

Run with no arguments to start the stdio MCP server. Configure your client with `"command": "brim-mcp"` (or full path, e.g. `~/.local/bin/brim-mcp`). For scripted/CI use, prefer the `brim` headless subcommands above.

**Tools:**

| Tool | Description |
|------|-------------|
| update_recipe_lock | Accept current recipe and update lockfile; retry after recipe change |
| validate_recipe | Validate recipe JSON string |
| list_recipe_packages | Fetch and merge recipe URL(s); return merged package list |
| sync_analysis_tool | Compare recipe vs installed; return to_install, to_remove, in_sync |
| install | Install from recipe URL(s); optional webhook_url, webhook_machine_id, parallel |
| remove | Remove packages by name (JSON array) |
| remove_recipe | Remove all packages listed in the recipe (opposite of install) |
| trim | Remove installed packages not in the recipe (clean up extras) |

**Config (Cursor):** `~/.cursor/mcp.json` or Settings > Tools & MCP. **Claude Desktop:** e.g. `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS). Add:
```json
{
  "mcpServers": {
    "brim": {
      "command": "brim-mcp"
    }
  }
}
```
Restart the app after editing.

## Recipe format

JSON array of packages. Example:
```json
[
  { "name": "postgresql", "category": "Database", "url": "https://formulae.brew.sh/formula/postgresql" },
  { "name": "visual-studio-code", "category": "Development", "cask": true }
]
```

Fields: `name` (required), `category`, `url`, `cask`, `version`. Names: alphanumeric, dots, hyphens, underscores. URLs: must start with http:// or https://.

Chaining: pass multiple files with `--url="a.json,b.json"` or repeated `--url`. Later files override earlier by package name.

## Recipe lockfile

Operations that use a recipe (list_recipe_packages, sync_analysis_tool, install) verify content against a lockfile. If the recipe changed, run `update_recipe_lock` (MCP) or `brim update-lock --urls <same urls>` then retry.

## Webhook payload

When a webhook URL is set (CLI or MCP install), BRIM POSTs JSON after install:

```json
{
  "status": "success",
  "total": 10,
  "completed": 10,
  "failed": 0,
  "packages": [{"name": "wget", "status": "completed"}, ...],
  "elapsed_seconds": 45,
  "machine_id": "my-hostname"
}
```

`status`: "success" | "partial" | "failed". `machine_id`: from `--webhook-machine-id` or MCP param, or default (HOSTNAME/COMPUTERNAME env or "unknown").

## Library (Rust)

Add to `Cargo.toml`:
```toml
[dependencies]
brim = { path = "/path/to/brim" }
```

Single URL, headless install:
```rust
use brim::{fetch_packages, install_packages_headless, FetchError};

#[tokio::main]
async fn main() -> Result<(), FetchError> {
    let packages = fetch_packages("https://example.com/recipe.json").await?;
    let results = install_packages_headless(&packages, true);
    for r in &results { println!("{}: {}", r.name, r.status); }
    Ok(())
}
```

Multiple URLs: use `fetch_and_merge_packages(&urls).await` (returns `Result<Recipe, String>`). Other APIs: `validate_recipe_json`, `list_installed_packages`, `sync_analysis`, `remove_packages_headless`, `update_lock`, `verify_or_update_lock`.

## Terminal UI

Progress bars and package states (downloading, installing, completed, failed). Color: formulae vs casks, state. Keys: Space (toggle), Enter (confirm), q (quit), ESC (force quit). `--autoquit N` exits summary after N seconds.

## Troubleshooting

- Stuck on "Fetching": ESC to quit; retry without `--parallel` if needed.
- Brew lock errors: `rm -rf /usr/local/var/homebrew/locks/*` (path may vary).
- Recipe lock error: run update_recipe_lock or `brim-mcp update-lock --urls <recipe>` then retry.

## Contributing

Issues and pull requests: [GitHub](https://github.com/brim-run/brim).

## License

Apache 2.0
