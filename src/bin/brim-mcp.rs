//! BRIM MCP server — stdio.
//!
//! Run with no arguments to start the MCP server. Clients discover and call tools via JSON-RPC
//! on stdin/stdout. Configure your client with `"command": "brim-mcp"`.
//!
//! For scripted/CI use without an MCP client, use the `brim` headless subcommands instead:
//! `brim install`, `brim sync`, `brim remove`, `brim update-lock`.

use brim::{
    fetch_and_merge_packages, install_packages_headless, list_installed_packages,
    remove_packages_headless, sync_analysis, update_lock, validate_recipe_json,
    verify_or_update_lock, webhook, BrewPackage, BrewPackageResult, Recipe,
};
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
        do_update_lock(&urls)?;
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
        description = "Install packages from recipe file(s) without interactive UI. Pass urls_json (JSON array of recipe URLs), parallel (true/false), and optionally webhook_url to POST a JSON summary after install. Optional webhook_machine_id is sent in the payload (default: hostname or \"unknown\"). Fails if recipe changed since last run; use update_recipe_lock to accept and retry."
    )]
    pub fn install(
        &self,
        #[param("JSON array of recipe URLs or file paths")] urls_json: String,
        #[param("Use parallel fetch then sequential install")] parallel: bool,
        #[param("Optional webhook URL to POST install summary (empty to skip)")]
        webhook_url: String,
        #[param("Optional machine ID for webhook payload (empty = default: hostname or \"unknown\")")]
        webhook_machine_id: String,
    ) -> Result<String, String> {
        let urls = parse_urls_json(&urls_json)?;
        let start = Instant::now();
        let results = do_install(&urls, parallel)?;
        let response_json = serde_json::to_string(&results).map_err(|e| e.to_string())?;

        let webhook_url = webhook_url.trim();
        if !webhook_url.is_empty() {
            let completed = results.iter().filter(|r| r.status == "completed").count();
            let failed = results.iter().filter(|r| r.status == "failed").count();
            let machine_id = webhook_machine_id.trim();
            let machine_id = if machine_id.is_empty() {
                webhook::default_machine_id()
            } else {
                machine_id.to_string()
            };
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
                machine_id,
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

/// Fetch recipe (no lock check). Used by fetch_recipe_verified and update_recipe_lock.
fn fetch_recipe(urls: &[String]) -> Result<Recipe, String> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(fetch_and_merge_packages(urls))
    })
    .map_err(|e| e.to_string())
}

/// Fetch recipe and verify lock. Used by list_recipe_packages, sync_analysis_tool, and do_install.
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

/// Fetch recipe, verify lock, run headless install. Used by the install MCP tool.
fn do_install(urls: &[String], parallel: bool) -> Result<Vec<BrewPackageResult>, String> {
    let packages = fetch_recipe_verified(urls)?;
    Ok(install_packages_headless(&packages, parallel))
}

/// Fetch recipe and update lockfile. Used by the update_recipe_lock MCP tool.
fn do_update_lock(urls: &[String]) -> Result<usize, String> {
    let packages = fetch_recipe(urls)?;
    let n = packages.len();
    update_lock(&packages, urls).map_err(|e| e.to_string())?;
    Ok(n)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = McpServerConfig::builder()
        .name("brim")
        .version(env!("CARGO_PKG_VERSION"))
        .with_tools_from(MacroServerAdapter::new(BrimMcpServer))
        .build();

    McpStdioServer::run(config).await?;
    Ok(())
}
