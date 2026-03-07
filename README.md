# BRIM

<a href="https://www.buymeacoffee.com/alexandrughinea" title="BRIM (Brew Remote Install Manager)">
  <img src=".fixtures/logo.svg" alt="BRIM (Brew Remote Install Manager)" width="256px">
</a>

**BRIM** (Brew Remote Install Manager) - A modern CLI tool for managing Homebrew packages with beautiful TUI progress tracking.

## Features

- **Beautiful terminal UI** with real-time progress bars (powered by ratatui)
- **Recipe chaining** - combine multiple recipe files (local + remote)
- **Sync analysis** - compare installed packages with recipes
- **Dry-run mode** - preview changes before applying
- **Smart installation** - parallel downloads, sequential installs
- **Clean removal** - auto-cleanup of dependencies
- **Cask support** - install both formulae and applications
- **Color-coded UI** - clear visual distinction between package types
- **Fast and safe** - respects Homebrew's locking mechanism

## Installation

### Quick Install (Recommended)

**macOS and Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/alexandrughinea/brim/main/install.sh | bash
```

This will:
- Detect your platform automatically (x86_64/ARM64, macOS/Linux)
- Download the latest release binary
- Install `brim` (and `brim-mcp` for MCP) to `~/.local/bin`
- Set up PATH if needed

### Manual Installation

Download pre-built binaries from the [releases page](https://github.com/alexandrughinea/brim/releases):

- **macOS Intel:** `brim-x86_64-macos.tar.gz`
- **macOS Apple Silicon:** `brim-aarch64-macos.tar.gz`
- **Linux x86_64:** `brim-x86_64-linux.tar.gz`
- **Linux ARM64:** `brim-aarch64-linux.tar.gz`

```bash
# Extract and install
tar -xzf brim-*.tar.gz
sudo mv brim /usr/local/bin/
# Newer releases also include brim-mcp for MCP; if present:
# sudo mv brim-mcp /usr/local/bin/
```

### Build from Source

```bash
# Clone the repository
git clone https://github.com/alexandrughinea/brim
cd brim

# Build release binary
cargo build --release

# Binary location
./target/release/brim

# Optional: Install to system
sudo cp target/release/brim /usr/local/bin/
```

## API Reference

### Command Line Interface

```bash
brim [OPTIONS]
```

### Options

| Option | Type | Description | Example |
|--------|------|-------------|---------|
| `--url <URL>` | String | Recipe file(s) - comma-separated or repeat flag | `--url="base.json,extras.json"` or `--url="base.json" --url="extras.json"` |
| `--list` | Flag | List all installed Homebrew packages | `--list` |
| `--remove` | Flag | Interactive removal of installed packages | `--remove` |
| `--sync` | Flag | Compare installed packages with recipe and show diff | `--sync --url="packages.json"` |
| `--parallel` | Flag | Enable parallel downloads (sequential install) | `--parallel` |
| `--dry-run` | Flag | Preview changes without installing or removing packages | `--dry-run` |
| `--webhook <URL>` | String | Webhook URL to POST installation summary (optional) | `--webhook="https://example.com/hook"` |
| `--autoquit <SECONDS>` | Number | Auto-quit summary screen after N seconds (after install) | `--autoquit 10` |
| `-h, --help` | Flag | Print help information | `--help` |

### Usage Examples

```bash
# Install packages from remote URL
brim --url="https://raw.githubusercontent.com/user/repo/main/packages.json"

# Install from local file
brim --url="packages.json"

# Chain multiple recipe files (later files override earlier ones)
brim --url="base.json" --url="dev-tools.json" --url="personal.json"

# Or use comma-separated syntax
brim --url="base.json,dev-tools.json,personal.json"

# Mix remote and local files
brim --url="https://example.com/base.json,local-overrides.json"

# Install with parallel downloads (faster)
brim --url="packages.json" --parallel

# Preview changes without installing (dry-run mode)
brim --url="packages.json" --dry-run

# Chain files with dry-run preview
brim --url="base.json" --url="extras.json" --dry-run

# Install with webhook notification
brim --url="packages.json" --webhook="https://hooks.example.com/notify"

# Install and auto-quit summary screen after 10 seconds
brim --url="packages.json" --autoquit 10

# List installed packages
brim --list

# Sync analysis - compare installed vs recipe
brim --sync --url="packages.json"

# Sync with multiple files
brim --sync --url="base.json,dev-tools.json"

# Remove packages (with preview option)
brim --remove --dry-run
```

### Library API (headless / programmatic use)

BRIM exposes a Rust library so other applications and scripts can call the same logic without the interactive TUI. Add `brim` as a dependency and use the API below. All functions that run `brew` require a normal terminal environment (e.g. `brew` on `PATH`).

**Add to `Cargo.toml`:**
```toml
[dependencies]
brim = { path = "/path/to/brim" }
# or from crates.io when published:
# brim = "0.2"
```

**MCP (Model Context Protocol)**

BRIM runs as an [MCP](https://modelcontextprotocol.io/) server so AI assistants and other MCP clients can drive it via tools. The server binary is **`brim-mcp`** and uses **stdio** (JSON-RPC on stdin/stdout).

**Build and run:**
```bash
cargo build --features mcp --bin brim-mcp
# or install
cargo install --path . --features mcp --bin brim-mcp
```

**Tools:**

| Tool | Description |
|------|-------------|
| **validate_recipe** | Validate a recipe JSON string. Argument: raw JSON array of packages. |
| **list_recipe_packages** | Fetch and merge recipe file(s) from URLs or paths. Argument: JSON array of URL/path strings. Returns merged package list as JSON. |
| **sync_analysis_tool** | Compare recipe (from URLs) with installed packages. Argument: JSON array of recipe URLs. Returns JSON with `to_install`, `to_remove`, `in_sync`. |
| **install** | Install packages from recipe file(s) without UI. Arguments: `urls_json` (JSON array of recipe URLs), `parallel` (bool), optional `webhook_url` (POST install summary JSON, same as CLI `--webhook`). Returns JSON array of `{ name, status }` per package. |

These tools are exposed to MCP clients when you run `brim-mcp` with **no arguments** (stdio server). The same install logic is also available as a one-shot CLI for scripts or hosts that invoke by command: `brim-mcp install --urls recipe.json` (see **One-shot CLI** below).

**How Cursor or Claude discover brim-mcp**

MCP servers are not discovered automatically. You add `brim-mcp` in your client config and set `command` to the full path of the `brim-mcp` binary. Restart the app after changing config.

Example MCP config (paste into your client's config file):

```json
{
  "mcpServers": {
    "brim": {
      "command": "brim-mcp"
    }
  }
}
```

Use `"command": "/absolute/path/to/brim-mcp"` if the binary is not on PATH (e.g. `~/.local/bin/brim-mcp` or `~/.cargo/bin/brim-mcp`).

- **If you used the install script:** `brim-mcp` is in the same directory as `brim`, e.g. `~/.local/bin/brim-mcp`. Use that path if it’s not on PATH.
- **If you used `cargo install ... --bin brim-mcp`:** the binary is usually `~/.cargo/bin/brim-mcp`. Ensure that directory is on your PATH so the client can run it.
- **If you built locally:** use the path to the binary, e.g. `./target/release/brim-mcp`.

**Cursor** — add the server in one of these ways:
- **Settings → Tools & MCP → Add new MCP server:** set type to **command** and command to the path above (e.g. `brim-mcp` if it’s on PATH, or `/absolute/path/to/brim-mcp`).
- **Config file:** create or edit `~/.cursor/mcp.json` (global) or `.cursor/mcp.json` (project) and paste the JSON above.

**Claude Desktop** — edit the MCP config (e.g. **Settings → Developer → Edit Config**). Config file location:
- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Linux:** `~/.config/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

Add the same `mcpServers` block as in the example above (or merge it into your existing config). Restart Claude Desktop after saving.

**One-shot CLI (for scripts and other MCP servers)**

To run the same install logic without starting the stdio server (e.g. from another MCP server or a script), use the `install` subcommand:

```bash
brim-mcp install --urls /path/to/recipe.json
# or multiple recipes
brim-mcp install -u recipe1.json -u recipe2.json
# optional: --parallel false
```

This uses the same code path as the MCP `install` tool and is exposed to MCP clients both via the stdio server (tools) and via command invocation.

**Library usage (Rust)**

You can also use the crate directly. Example (headless install from a recipe URL):
```rust
use brim::{fetch_packages, install_packages_headless, FetchError};

#[tokio::main]
async fn main() -> Result<(), FetchError> {
    let packages = fetch_packages("https://example.com/recipe.json").await?;
    let results = install_packages_headless(&packages, true);
    for r in &results {
        println!("{}: {}", r.name, r.status);
    }
    Ok(())
}
```

**Example (validate + sync analysis):**
```rust
use brim::{fetch_and_merge_packages, list_installed_packages, sync_analysis};

#[tokio::main]
async fn main() {
    let urls = vec!["packages.json".to_string()];
    match fetch_and_merge_packages(&urls).await {
        Ok(recipe) => {
            let installed = list_installed_packages();
            let report = sync_analysis(&recipe, &installed);
            println!("To install: {}, to remove: {}, in sync: {}",
                report.to_install.len(), report.to_remove.len(), report.in_sync.len());
        }
        Err(e) => eprintln!("Fetch failed: {}", e),
    }
}
```

The CLI binary is built from the same library; interactive installs use the TUI, while the library and MCP server expose headless operations for other processes.

## Recipe Chaining

BRIM supports chaining multiple recipe files together, allowing you to compose your package lists from multiple sources:

```bash
# Multiple --url flags
brim --url="base.json" --url="python-dev.json" --url="my-tools.json"

# Or comma-separated (both work!)
brim --url="base.json,python-dev.json,my-tools.json"
```

### How Chaining Works

1. **Sequential Loading**: Recipe files are loaded in the order specified
2. **Deduplication**: If a package appears in multiple files, the **later file takes precedence**
3. **Merge Strategy**: Package definitions are merged by package name
4. **Mix Sources**: You can freely mix local and remote files in one command

### Common Use Cases

**Modular Setup:**
```bash
# Using multiple flags
brim --url="base-tools.json" --url="media-tools.json" --url="dev-tools.json"

# Or comma-separated
brim --url="base-tools.json,media-tools.json,dev-tools.json"
```

**Environment-Specific:**
```bash
# Company standard + team-specific + personal
brim --url="https://company.com/standard.json,team-shared.json,personal.json"
```

**Override Pattern:**
```bash
# Use defaults but override with local customizations
brim --url="https://example.com/defaults.json,local-overrides.json"
```

## Recipe File Format

BRIM uses JSON recipe files to define packages. The format is simple and flexible:

```json
[
  {
    "name": "postgresql",
    "category": "Database",
    "url": "https://formulae.brew.sh/formula/postgresql"
  },
  {
    "name": "visual-studio-code",
    "category": "Development",
    "url": "https://formulae.brew.sh/cask/visual-studio-code",
    "cask": true
  }
]
```

### Recipe Schema

| Field | Type | Required | Description | Validation |
|-------|------|----------|-------------|------------|
| `name` | String | ✓ | Package name as it appears in Homebrew | Alphanumeric, dots, hyphens, underscores only |
| `category` | String | ✗ | Organization category (displayed in UI) | Any non-empty string |
| `url` | String | ✗ | Reference URL to Homebrew formulae page | Must start with http:// or https:// |
| `cask` | Boolean | ✗ | Set to `true` for cask applications | true or false |
| `version` | String | ✗ | Specific version (reserved for future use) | Semantic versioning (e.g., 1.0.0) |

### Recipe Validation

BRIM automatically validates recipe files and provides helpful error messages:

```bash
# Valid recipe
✓ Package names must be alphanumeric with dots, hyphens, or underscores
✓ URLs must start with http:// or https://
✓ Versions must follow semantic versioning (major.minor.patch)
✓ Recipe must contain at least one package

# Common validation errors and fixes:
✗ "Package has invalid name format"
  → Use only a-z, A-Z, 0-9, dots, hyphens, underscores

✗ "Package has invalid URL"
  → Ensure URL starts with http:// or https://

✗ "Package has invalid version format"
  → Use semantic versioning: 1.0.0 or 1.0

✗ "Recipe file contains no packages"
  → Add at least one package to your recipe
```

### JSON Schema

A JSON schema is available at `recipe-schema.json` for IDE validation and autocomplete. Configure your editor:

**VS Code** (`.vscode/settings.json`):
```json
{
  "json.schemas": [
    {
      "fileMatch": ["*recipe*.json", "*packages*.json"],
      "url": "./recipe-schema.json"
    }
  ]
}
```

## Terminal UI

### Installation Progress

```
┌─────────────────────────────────────────────────────────┐
│ BRIM - Brew Remote Install Manager                     │
└─────────────────────────────────────────────────────────┘

┌─ Progress ──────────────────────────────────────────────┐
│ ████████████████████░░░░░░░░░░░░ 5/10 packages         │
└─────────────────────────────────────────────────────────┘

┌─ postgresql [completed] ───────────────────────────────┐
│ ████████████████████████████████████████ Done!         │
└─────────────────────────────────────────────────────────┘

┌─ redis [installing] ───────────────────────────────────┐
│ ████████████████████░░░░░░░░░░░░ 60%                   │
└─────────────────────────────────────────────────────────┘
```

### Color Coding

- **Green** - Regular Homebrew formulae
- **Magenta** - Cask applications  
- **Yellow** - Downloading state
- **Blue** - Installing state
- **Red** - Failed/Error state
- **Gray** - Pending state

### Interactive Controls

| Key | Action |
|-----|--------|
| `Space` | Toggle package selection |
| `Enter` | Confirm selection and proceed |
| `q` | Quit (after completion) |
| `ESC` | Force quit immediately |

With `--autoquit <SECONDS>`, the summary screen shows a countdown and exits automatically after the given number of seconds (e.g. `--autoquit 10`).

## Performance Modes

### Sequential Mode (Default)

Installs packages one at a time. **Recommended** for stability.

```bash
brim --url="packages.json"
```

**Pros:** Stable, respects all Homebrew locks  
**Cons:** Slower for large lists

### Parallel Mode

Downloads all packages simultaneously, then installs sequentially.

```bash
brim --url="packages.json" --parallel
```

**Pros:** Faster downloads, still safe  
**Cons:** None - this is the recommended fast mode

## Technical Details

### Architecture

- **Language:** Rust 2021 edition
- **TUI Framework:** ratatui + crossterm
- **Async Runtime:** tokio
- **HTTP Client:** reqwest

### Timeouts

- Fetch operation: 2 minutes per package
- Install operation: 3 minutes per package
- Autoremove: 1 minute
- Webhook POST: 10 seconds

### Webhook Integration

When `--webhook` flag is provided, BRIM will POST a JSON summary after operations complete:

```json
{
  "status": "success",
  "total": 10,
  "completed": 9,
  "failed": 1,
  "packages": [
    {"name": "postgresql", "status": "completed"},
    {"name": "redis", "status": "failed"}
  ],
  "elapsed_seconds": 245
}
```

**Status values:**
- `success` - All packages completed
- `partial` - Some packages failed
- `failed` - All packages failed

### Thread Safety

- Uses `Arc<Mutex<T>>` for shared state
- Non-blocking `try_lock()` in output readers
- Proper cleanup on timeout/failure

## Troubleshooting

### Package Stuck on "Fetching"

Press `ESC` to force quit and retry. Or run without `--parallel` flag.

### Brew Lock Errors

```bash
# Clear Homebrew locks
rm -rf /usr/local/var/homebrew/locks/*
```

### Permission Errors

BRIM requires the same permissions as Homebrew. Run with appropriate user privileges.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features and future releases.

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests on the [GitHub repository](https://github.com/alexandrughinea/brim).

## License

Apache 2.0

## Donations

If you like **BRIM**, thanks for considering supporting its development!

<a href="https://www.buymeacoffee.com/alexandrughinea" title="Buy me a beer">
  <img src=".fixtures/bmc_qr.png" alt="Donate" width="128px">
</a>

## Author

Alex Ghinea - [alexandrughinea.dev+brim@gmail.com](mailto:alexandrughinea.dev+brim@gmail.com)
