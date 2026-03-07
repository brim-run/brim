//! BRIM MCP server and one-shot CLI.
//!
//! **Exposed to MCP clients (Cursor, Claude, etc.):**
//! - Run with **no arguments** to start the stdio MCP server. Clients discover and call tools
//!   (`install`, `list_recipe_packages`, `sync_analysis_tool`, `validate_recipe`, `update_recipe_lock`, `remove`)
//!   via JSON-RPC on stdin/stdout. Configure your MCP client with `"command": "brim-mcp"`.
//!
//! **Exposed to other tools / scripts:**
//! - Run with the **`install` subcommand** for a one-shot install (same logic as the MCP `install` tool).
//!   Use this when another MCP server or script invokes brim-mcp as a command, e.g.:
//!   `brim-mcp install --urls recipe.json`

use brim::{
    fetch_and_merge_packages, install_packages_headless, list_installed_packages, remove_packages_headless,
    sync_analysis, update_lock, validate_recipe_json, verify_or_update_lock, webhook, BrewPackage,
    BrewPackageResult, Recipe,
};
use clap::{Arg, ArgMatches, Command};
use model_context_protocol::macros::mcp_server;
use model_context_protocol::server::stdio::McpStdioServer;
use model_context_protocol::{MacroServerAdapter, McpServerConfig};
use std::time::Instant;

#[mcp_server(name = "brim", version = env!("CARGO_PKG_VERSION"))]
pub struct BrimMcpServer;

#[mcp_server]
impl BrimMcpServer {
    #[mcp_tool(
        description = "Accept current recipe content and update the lockfile. Call this when recipe has changed (e.g. added a package) and you want to proceed. Pass the same JSON array of recipe URLs/paths you use for list_recipe_packages or install. Then retry the original tool."
    )]
    pub fn update_recipe_lock(
        &self,
        #[param("JSON array of recipe URLs or file paths (same as other recipe tools)")]
        urls_json: String,
    ) -> Result<String, String> {
        let urls = parse_urls_json(&urls_json)?;
        let packages = fetch_recipe(&urls)?;
        update_lock(&packages, &urls).map_err(|e| e.to_string())?;
        Ok("Recipe lock updated. You can retry the previous operation.".to_string())
    }

    #[mcp_tool(
        description = "Validate a recipe JSON string. Pass the raw JSON content of the recipe file."
    )]
    pub fn validate_recipe(
        &self,
        #[param("Recipe JSON string (array of packages)")] json: String,
    ) -> Result<String, String> {
        validate_recipe_json(json.trim())
            .map(|()| "Validation passed.".to_string())
            .map_err(|e| e.to_string())
    }

    #[mcp_tool(
        description = "Fetch and merge recipe files from URLs or local paths. Pass a JSON array of strings, e.g. [\"https://example.com/recipe.json\", \"local.json\"]. Fails if recipe content changed since last run (integrity lock); use update_recipe_lock to accept the change and retry."
    )]
    pub fn list_recipe_packages(
        &self,
        #[param("JSON array of recipe URLs or file paths")] urls_json: String,
    ) -> Result<String, String> {
        let urls = parse_urls_json(&urls_json)?;
        let packages = fetch_recipe_verified(&urls)?;
        serde_json::to_string(&packages).map_err(|e| e.to_string())
    }

    #[mcp_tool(
        description = "Run sync analysis: compare recipe packages (from URLs) with installed packages. Pass a JSON array of recipe URLs. Returns JSON with to_install, to_remove, in_sync. Fails if recipe changed since last run; use update_recipe_lock to accept and retry."
    )]
    pub fn sync_analysis_tool(
        &self,
        #[param("JSON array of recipe URLs or file paths")] urls_json: String,
    ) -> Result<String, String> {
        let urls = parse_urls_json(&urls_json)?;
        let recipe = fetch_recipe_verified(&urls)?;
        let installed = list_installed_packages();
        let report = sync_analysis(&recipe, &installed);
        serde_json::to_string(&report).map_err(|e| e.to_string())
    }

    #[mcp_tool(
        description = "Install packages from recipe file(s) without interactive UI. Pass urls_json (JSON array of recipe URLs), parallel (true/false), and optionally webhook_url to POST a JSON summary after install. Fails if recipe changed since last run; use update_recipe_lock to accept and retry."
    )]
    pub fn install(
        &self,
        #[param("JSON array of recipe URLs or file paths")] urls_json: String,
        #[param("Use parallel fetch then sequential install")] parallel: bool,
        #[param("Optional webhook URL to POST install summary (empty to skip)")]
        webhook_url: String,
    ) -> Result<String, String> {
        let urls = parse_urls_json(&urls_json)?;
        let start = Instant::now();
        let results = do_install(&urls, parallel)?;
        let response_json = serde_json::to_string(&results).map_err(|e| e.to_string())?;

        let webhook_url = webhook_url.trim();
        if !webhook_url.is_empty() {
            let completed = results.iter().filter(|r| r.status == "completed").count();
            let failed = results.iter().filter(|r| r.status == "failed").count();
            let payload = webhook::WebhookPayload {
                status: if failed > 0 {
                    "partial".to_string()
                } else {
                    "success".to_string()
                },
                total: results.len(),
                completed,
                failed,
                packages: results,
                elapsed_seconds: start.elapsed().as_secs(),
            };
            if let Err(e) = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(webhook::post_webhook(webhook_url, payload))
            }) {
                return Ok(format!(
                    "{}\nWebhook notification failed: {}",
                    response_json, e
                ));
            }
        }

        Ok(response_json)
    }

    #[mcp_tool(
        description = "Remove Homebrew packages by name. Pass a JSON array of package names (e.g. from sync_analysis_tool's to_remove). Returns JSON array of { name, status } per package."
    )]
    pub fn remove(
        &self,
        #[param("JSON array of package names to remove")] names_json: String,
    ) -> Result<String, String> {
        let names: Vec<String> = serde_json::from_str(names_json.trim())
            .map_err(|e| format!("Invalid JSON array of names: {}", e))?;
        let packages: Vec<BrewPackage> = names
            .into_iter()
            .map(|name| BrewPackage {
                name: name.clone(),
                category: None,
                url: None,
                cask: None,
                version: None,
            })
            .collect();
        let results = remove_packages_headless(&packages);
        serde_json::to_string(&results).map_err(|e| e.to_string())
    }
}

/// Parse JSON array of recipe URLs/paths. Shared by all tools that take urls_json.
fn parse_urls_json(urls_json: &str) -> Result<Vec<String>, String> {
    serde_json::from_str(urls_json.trim())
        .map_err(|e| format!("Invalid JSON array of URLs: {}", e))
}

/// Shared: fetch recipe only (no lock check). Used by fetch_recipe_verified and update_recipe_lock.
fn fetch_recipe(urls: &[String]) -> Result<Recipe, String> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(fetch_and_merge_packages(urls))
    })
    .map_err(|e| e.to_string())
}

/// Shared: fetch recipe and verify lock. Used by list_recipe_packages, sync_analysis_tool, and do_install.
fn fetch_recipe_verified(urls: &[String]) -> Result<Recipe, String> {
    let packages = fetch_recipe(urls)?;
    verify_or_update_lock(&packages, urls).map_err(|e| {
        format!(
            "{} Use update_recipe_lock to accept the new recipe and retry.",
            e
        )
    })?;
    Ok(packages)
}

/// Shared install path: fetch recipe, verify lock, run headless install. Used by MCP tool and CLI.
fn do_install(urls: &[String], parallel: bool) -> Result<Vec<BrewPackageResult>, String> {
    let packages = fetch_recipe_verified(urls)?;
    Ok(install_packages_headless(&packages, parallel))
}

fn run_install(urls: &[String], parallel: bool) -> Result<(), String> {
    let start = Instant::now();
    let results = do_install(urls, parallel)?;
    for r in &results {
        println!("  {}: {}", r.name, r.status);
    }
    println!(
        "Done in {}s ({} completed, {} failed).",
        start.elapsed().as_secs(),
        results.iter().filter(|r| r.status == "completed").count(),
        results.iter().filter(|r| r.status == "failed").count()
    );
    Ok(())
}

/// Sync flow: same as sync_analysis_tool, human-readable CLI output.
fn run_sync(urls: &[String]) -> Result<(), String> {
    let recipe = fetch_recipe_verified(urls)?;
    let installed = list_installed_packages();
    let report = sync_analysis(&recipe, &installed);
    println!("Sync analysis (recipe vs installed):");
    println!("  To install:  {} (not in system)", report.to_install.len());
    println!("  To remove:   {} (not in recipe)", report.to_remove.len());
    println!("  In sync:    {}", report.in_sync.len());
    if !report.to_install.is_empty() {
        println!("  Missing:    {}", report.to_install.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    if !report.to_remove.is_empty() {
        println!("  Extra:      {}", report.to_remove.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

/// Remove flow: remove packages not in recipe (to_remove from sync). Same list as sync_analysis_tool's to_remove.
fn run_remove(urls: &[String]) -> Result<(), String> {
    let recipe = fetch_recipe_verified(urls)?;
    let installed = list_installed_packages();
    let report = sync_analysis(&recipe, &installed);
    if report.to_remove.is_empty() {
        println!("Nothing to remove (all installed packages are in the recipe).");
        return Ok(());
    }
    println!("Removing {} package(s) not in recipe: {}", report.to_remove.len(), report.to_remove.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
    let start = Instant::now();
    let results = remove_packages_headless(&report.to_remove);
    for r in &results {
        println!("  {}: {}", r.name, r.status);
    }
    println!(
        "Done in {}s ({} completed, {} failed).",
        start.elapsed().as_secs(),
        results.iter().filter(|r| r.status == "completed").count(),
        results.iter().filter(|r| r.status == "failed").count()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("brim-mcp")
        .about("BRIM MCP server (stdio). Use 'install' subcommand for one-shot install from recipe.")
        .subcommand_required(false)
        .arg_required_else_help(false)
        .subcommand(
            Command::new("install")
                .about("Install packages from recipe URL(s) or path(s). Same logic as MCP install tool.")
                .arg(
                    Arg::new("urls")
                        .short('u')
                        .long("urls")
                        .value_delimiter(',')
                        .num_args(1..)
                        .required(true)
                        .help("Recipe URL(s) or file path(s)"),
                )
                .arg(
                    Arg::new("parallel")
                        .short('p')
                        .long("parallel")
                        .action(clap::ArgAction::Set)
                        .default_value("true")
                        .help("Use parallel fetch then sequential install (default: true)"),
                ),
        )
        .subcommand(
            Command::new("sync")
                .about("Sync analysis: compare recipe with installed packages. Same logic as MCP sync_analysis_tool.")
                .arg(
                    Arg::new("urls")
                        .short('u')
                        .long("urls")
                        .value_delimiter(',')
                        .num_args(1..)
                        .required(true)
                        .help("Recipe URL(s) or file path(s)"),
                ),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove packages not in recipe (extras). Same as MCP: sync_analysis then remove to_remove.")
                .arg(
                    Arg::new("urls")
                        .short('u')
                        .long("urls")
                        .value_delimiter(',')
                        .num_args(1..)
                        .required(true)
                        .help("Recipe URL(s) or file path(s)"),
                ),
        )
        .get_matches();

    fn urls_from_sub(sub: &ArgMatches) -> Vec<String> {
        sub.get_many::<String>("urls")
            .unwrap_or_default()
            .cloned()
            .collect()
    }

    match matches.subcommand() {
        Some(("install", sub)) => {
            let urls = urls_from_sub(sub);
            let parallel = sub
                .get_one::<String>("parallel")
                .map(|s| s != "false")
                .unwrap_or(true);
            if let Err(e) = run_install(&urls, parallel) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
            Ok(())
        }
        Some(("sync", sub)) => {
            if let Err(e) = run_sync(&urls_from_sub(sub)) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
            Ok(())
        }
        Some(("remove", sub)) => {
            if let Err(e) = run_remove(&urls_from_sub(sub)) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
            Ok(())
        }
        _ => {
            let config = McpServerConfig::builder()
                .name("brim")
                .version(env!("CARGO_PKG_VERSION"))
                .with_tools_from(MacroServerAdapter::new(BrimMcpServer))
                .build();

            McpStdioServer::run(config).await?;
            Ok(())
        }
    }
}
