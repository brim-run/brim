use std::time::Instant;

use clap::{Arg, ArgMatches, Command};
use console::{style, Color, StyledObject};
use dialoguer::theme::Theme;
use dialoguer::{Confirm, MultiSelect};
use std::fmt;

use brim::fetch_and_merge_packages;
use brim::models::BrewPackage;
use brim::sync_analysis;
use brim::utilities::brew_common;
use brim::utilities::{
    brew_install_packages::{install_packages, install_packages_headless},
    brew_list_installed_packages::list_installed_packages,
    brew_recipe_lock,
    brew_remove_packages::{remove_packages, remove_packages_headless},
};
use brim::webhook::{default_machine_id, post_webhook, WebhookPayload};
use brim::LockError;

#[cfg(test)]
mod tests;

struct MultiSelectThemeNoColon;

impl Theme for MultiSelectThemeNoColon {
    fn format_prompt(&self, f: &mut dyn fmt::Write, prompt: &str) -> fmt::Result {
        write!(f, "{}", prompt)
    }
}

fn print_header(title: &str, color: Color) {
    let (top, middle, bottom) = brew_common::header_lines(title);
    println!("\n{}", style(top).fg(color).bold());
    println!("{}", style(middle).fg(color).bold());
    println!("{}", style(bottom).fg(color).bold());
}

/// Collect comma-separated or repeated `--urls` values from a subcommand match.
fn collect_urls(sub: &ArgMatches) -> Vec<String> {
    sub.get_many::<String>("urls")
        .unwrap_or_default()
        .cloned()
        .collect()
}

#[tokio::main]
async fn main() {
    let start_time = Instant::now();

    let matches = Command::new("BRIM")
        .version(env!("CARGO_PKG_VERSION"))
        .disable_version_flag(true)
        .subcommand_required(false)
        .arg_required_else_help(false)
        // ── TUI mode flags (no subcommand) ──────────────────────────────────
        .arg(
            Arg::new("version")
                .short('v')
                .long("version")
                .action(clap::ArgAction::Version)
                .help("Print version information"),
        )
        .arg(
            Arg::new("url")
                .long("url")
                .value_name("URL")
                .action(clap::ArgAction::Append)
                .help("Recipe file(s): separate multiple with commas or repeat flag"),
        )
        .arg(
            Arg::new("list")
                .long("list")
                .action(clap::ArgAction::SetTrue)
                .help("List preinstalled Homebrew packages."),
        )
        .arg(
            Arg::new("remove")
                .long("remove")
                .action(clap::ArgAction::SetTrue)
                .help("Remove Homebrew packages (forced)."),
        )
        .arg(
            Arg::new("sync")
                .long("sync")
                .action(clap::ArgAction::SetTrue)
                .help("Sync installed packages with recipe file(s)"),
        )
        .arg(
            Arg::new("parallel")
                .long("parallel")
                .action(clap::ArgAction::SetTrue)
                .help("Parallel download + sequential install (faster, safe)"),
        )
        .arg(
            Arg::new("webhook")
                .long("webhook")
                .value_name("URL")
                .help("Webhook URL to post installation summary (optional)"),
        )
        .arg(
            Arg::new("webhook-machine-id")
                .long("webhook-machine-id")
                .value_name("ID")
                .help("Machine ID sent in webhook payload (default: hostname from env or \"unknown\")"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(clap::ArgAction::SetTrue)
                .help("Preview changes without installing or removing packages"),
        )
        .arg(
            Arg::new("autoquit")
                .long("autoquit")
                .value_name("SECONDS")
                .num_args(1)
                .help("Auto-quit summary screen after N seconds (after install)"),
        )
        // ── Headless subcommands ─────────────────────────────────────────────
        .subcommand(
            Command::new("install")
                .about("Install packages from recipe URL(s) without TUI. Lock is verified; use update-lock to accept changes.")
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
                        .action(clap::ArgAction::SetTrue)
                        .help("Parallel fetch, sequential install"),
                )
                .arg(
                    Arg::new("webhook")
                        .long("webhook")
                        .value_name("URL")
                        .help("Webhook URL to POST install summary"),
                )
                .arg(
                    Arg::new("webhook-machine-id")
                        .long("webhook-machine-id")
                        .value_name("ID")
                        .help("Machine ID in webhook payload (default: hostname or \"unknown\")"),
                ),
        )
        .subcommand(
            Command::new("sync")
                .about("Compare recipe with installed packages; show to_install / to_remove / in_sync.")
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
                .about("Remove packages: by name (--packages) or by recipe (--urls removes extras not in recipe).")
                .arg(
                    Arg::new("urls")
                        .short('u')
                        .long("urls")
                        .value_delimiter(',')
                        .num_args(1..)
                        .help("Recipe URL(s); remove packages not in recipe (extras)"),
                )
                .arg(
                    Arg::new("packages")
                        .short('n')
                        .long("packages")
                        .value_delimiter(',')
                        .num_args(1..)
                        .help("Package name(s) to remove (e.g. wget,jq)"),
                )
                .group(
                    clap::ArgGroup::new("remove_mode")
                        .args(["urls", "packages"])
                        .required(true)
                        .multiple(false),
                ),
        )
        .subcommand(
            Command::new("update-lock")
                .about("Accept current recipe and update lockfile. Use after changing recipe so sync/install can proceed.")
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

    match matches.subcommand() {
        Some(("install", sub)) => {
            run_install_headless(sub, start_time).await;
        }
        Some(("sync", sub)) => {
            run_sync_headless(sub).await;
        }
        Some(("remove", sub)) => {
            run_remove_headless(sub).await;
        }
        Some(("update-lock", sub)) => {
            run_update_lock_headless(sub).await;
        }
        _ => {
            run_tui(&matches, start_time).await;
        }
    }
}

// ── Headless subcommand handlers ──────────────────────────────────────────────

async fn run_install_headless(sub: &ArgMatches, start_time: Instant) {
    let urls = collect_urls(sub);
    let parallel = sub.get_flag("parallel");
    let webhook_url = sub.get_one::<String>("webhook").cloned();
    let machine_id_opt = sub.get_one::<String>("webhook-machine-id").cloned();

    println!("\n{} Fetching recipe...", style("→").cyan().bold());
    let packages = match fetch_and_merge_packages(&urls).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("\n{} {}", style("✗").red().bold(), style("Error fetching recipe").red().bold());
            eprintln!("  {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = brew_recipe_lock::verify_or_update_lock(&packages, &urls) {
        eprintln!("\n{} {}", style("✗").red().bold(), style(e.to_string()).red());
        eprintln!("  Run: brim update-lock --urls {}", urls.join(","));
        std::process::exit(1);
    }

    println!("{} Installing {} package(s)...", style("→").cyan().bold(), packages.len());
    let results = install_packages_headless(&packages, parallel);

    let completed = results.iter().filter(|r| r.status == "completed").count();
    let failed = results.iter().filter(|r| r.status == "failed").count();

    for r in &results {
        let icon = if r.status == "completed" {
            style("✓").green().bold()
        } else {
            style("✗").red().bold()
        };
        println!("  {} {}", icon, r.name);
    }

    let summary_icon = if failed == 0 {
        style("✓").green().bold()
    } else {
        style("⚠").yellow().bold()
    };
    println!(
        "\n{} Done in {}s — {} completed, {} failed.",
        summary_icon,
        start_time.elapsed().as_secs(),
        completed,
        failed
    );

    if let Some(url) = webhook_url {
        let machine_id = machine_id_opt.unwrap_or_else(default_machine_id);
        let payload = WebhookPayload {
            status: if failed > 0 { "partial".to_string() } else { "success".to_string() },
            total: results.len(),
            completed,
            failed,
            packages: results,
            elapsed_seconds: start_time.elapsed().as_secs(),
            machine_id,
        };
        match post_webhook(&url, payload).await {
            Ok(_) => println!("{} Webhook sent.", style("✓").green()),
            Err(e) => eprintln!("{} Webhook failed: {}", style("⚠").yellow(), e),
        }
    }
}

async fn run_sync_headless(sub: &ArgMatches) {
    let urls = collect_urls(sub);

    println!("\n{} Fetching recipe...", style("→").cyan().bold());
    let recipe = match fetch_and_merge_packages(&urls).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("\n{} {}", style("✗").red().bold(), style("Error fetching recipe").red().bold());
            eprintln!("  {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = brew_recipe_lock::verify_or_update_lock(&recipe, &urls) {
        eprintln!("\n{} {}", style("✗").red().bold(), style(e.to_string()).red());
        eprintln!("  Run: brim update-lock --urls {}", urls.join(","));
        std::process::exit(1);
    }

    let installed = list_installed_packages();
    let report = sync_analysis(&recipe, &installed);

    println!("\n{} Sync analysis (recipe vs installed):", style("→").cyan().bold());
    println!("  {} In sync:     {}", style("✓").green(), style(report.in_sync.len()).cyan().bold());
    println!("  {} To install:  {}", style("+").green(), style(report.to_install.len()).cyan().bold());
    println!("  {} To remove:   {}", style("-").red(), style(report.to_remove.len()).cyan().bold());

    if !report.to_install.is_empty() {
        println!("\n{}", style("Packages to install:").green().bold());
        for p in &report.to_install {
            println!("  {} {}", style("+").green().bold(), p.name);
        }
    }
    if !report.to_remove.is_empty() {
        println!("\n{}", style("Extra packages (not in recipe):").yellow().bold());
        for p in &report.to_remove {
            println!("  {} {}", style("-").yellow().bold(), p.name);
        }
    }
    if report.to_install.is_empty() && report.to_remove.is_empty() {
        println!("\n{} All packages are in sync!", style("✓").green().bold());
    }
}

async fn run_remove_headless(sub: &ArgMatches) {
    let names: Vec<String> = sub
        .get_many::<String>("packages")
        .unwrap_or_default()
        .cloned()
        .collect();

    let packages_to_remove: Vec<BrewPackage> = if !names.is_empty() {
        names
            .into_iter()
            .map(|name| BrewPackage { name, category: None, url: None, cask: None, version: None })
            .collect()
    } else {
        let urls = collect_urls(sub);
        println!("\n{} Fetching recipe...", style("→").cyan().bold());
        let recipe = match fetch_and_merge_packages(&urls).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("\n{} {}", style("✗").red().bold(), style("Error fetching recipe").red().bold());
                eprintln!("  {}", e);
                std::process::exit(1);
            }
        };
        if let Err(e) = brew_recipe_lock::verify_or_update_lock(&recipe, &urls) {
            eprintln!("\n{} {}", style("✗").red().bold(), style(e.to_string()).red());
            eprintln!("  Run: brim update-lock --urls {}", urls.join(","));
            std::process::exit(1);
        }
        let installed = list_installed_packages();
        let report = sync_analysis(&recipe, &installed);
        if report.to_remove.is_empty() {
            println!("{} Nothing to remove — all installed packages are in the recipe.", style("✓").green().bold());
            return;
        }
        println!(
            "{} Removing {} package(s) not in recipe: {}",
            style("→").cyan().bold(),
            report.to_remove.len(),
            report.to_remove.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
        );
        report.to_remove
    };

    let start = Instant::now();
    let results = remove_packages_headless(&packages_to_remove);
    let completed = results.iter().filter(|r| r.status == "completed").count();
    let failed = results.iter().filter(|r| r.status == "failed").count();

    for r in &results {
        let icon = if r.status == "completed" {
            style("✓").green().bold()
        } else {
            style("✗").red().bold()
        };
        println!("  {} {}", icon, r.name);
    }

    let summary_icon = if failed == 0 {
        style("✓").green().bold()
    } else {
        style("⚠").yellow().bold()
    };
    println!(
        "\n{} Done in {}s — {} removed, {} failed.",
        summary_icon,
        start.elapsed().as_secs(),
        completed,
        failed
    );
}

async fn run_update_lock_headless(sub: &ArgMatches) {
    let urls = collect_urls(sub);

    println!("\n{} Fetching recipe...", style("→").cyan().bold());
    let packages = match fetch_and_merge_packages(&urls).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("\n{} {}", style("✗").red().bold(), style("Error fetching recipe").red().bold());
            eprintln!("  {}", e);
            std::process::exit(1);
        }
    };

    match brew_recipe_lock::update_lock(&packages, &urls) {
        Ok(()) => println!(
            "{} Recipe lock updated for {} package(s).",
            style("✓").green().bold(),
            packages.len()
        ),
        Err(e) => {
            eprintln!("\n{} {}", style("✗").red().bold(), style("Failed to update lock").red().bold());
            eprintln!("  {}", e);
            std::process::exit(1);
        }
    }
}

// ── TUI mode (no subcommand) ──────────────────────────────────────────────────

async fn run_tui(matches: &ArgMatches, start_time: Instant) {
    let installed_packages = list_installed_packages();

    if let Some(urls) = matches.get_many::<String>("url") {
        let mut url_list: Vec<String> = Vec::new();
        for url_arg in urls {
            for url in url_arg.split(',') {
                let trimmed = url.trim();
                if !trimmed.is_empty() {
                    url_list.push(trimmed.to_string());
                }
            }
        }

        println!("\n{} Fetching recipe files...", style("→").cyan().bold());
        for (i, u) in url_list.iter().enumerate() {
            println!(
                "  {} {}",
                style(format!("{}/{}:", i + 1, url_list.len())).dim(),
                style(u).cyan()
            );
        }
        match fetch_and_merge_packages(&url_list).await {
            Ok(packages) => {
                let packages: Vec<BrewPackage> = packages;
                if let Err(e) = brew_recipe_lock::verify_or_update_lock(&packages, &url_list) {
                    if let LockError::IntegrityMismatch { .. } = &e {
                        eprintln!(
                            "\n{} {}",
                            style("⚠").yellow().bold(),
                            style("Recipe has changed since last run.").yellow().bold()
                        );
                        let update = Confirm::with_theme(&MultiSelectThemeNoColon)
                            .with_prompt("Update lock and continue?")
                            .default(false)
                            .interact()
                            .unwrap_or(false);
                        if update {
                            if let Err(we) = brew_recipe_lock::update_lock(&packages, &url_list) {
                                eprintln!(
                                    "\n{} {}",
                                    style("✗").red().bold(),
                                    style("Failed to update lock").red().bold()
                                );
                                eprintln!("  {}", we);
                                std::process::exit(1);
                            }
                        } else {
                            eprintln!(
                                "\n{} {}",
                                style("✗").red().bold(),
                                style("Recipe integrity check failed").red().bold()
                            );
                            eprintln!("  {}", e);
                            std::process::exit(1);
                        }
                    } else {
                        eprintln!(
                            "\n{} {}",
                            style("✗").red().bold(),
                            style("Recipe integrity check failed").red().bold()
                        );
                        eprintln!("  {}", e);
                        std::process::exit(1);
                    }
                }
                println!(
                    "\n{} Merged {} unique packages from recipe file(s)",
                    style("✓").green().bold(),
                    style(packages.len()).cyan().bold()
                );
                print_header("Brew Remote Install Manager", Color::Cyan);

                println!("\n{}", style("Legend:").yellow().bold());
                println!("  {} Regular package (not installed)", style("◯").green());
                println!("  {} Regular package (installed)", style("●").green().dim());
                println!(
                    "  {} Cask application (not installed)",
                    style("◯").magenta()
                );
                println!(
                    "  {} Cask application (installed)",
                    style("●").magenta().dim()
                );

                let installed_count = packages
                    .iter()
                    .filter(|p| {
                        installed_packages
                            .iter()
                            .any(|ip| ip.name.to_string().contains(&p.name))
                    })
                    .count();
                let cask_count = packages.iter().filter(|p| p.cask.is_some()).count();
                let recipe_label = if url_list.len() == 1 {
                    "Summary for current recipe file:"
                } else {
                    "Summary for current recipe files (merged):"
                };
                println!("\n{}", style(recipe_label).yellow().bold());
                println!("  Total packages: {}", style(packages.len()).cyan().bold());
                println!("  Already installed: {}", style(installed_count).green());
                println!("  Casks: {}", style(cask_count).magenta());
                println!("  Formulae: {}", style(packages.len() - cask_count).green());

                let prompt: String = format!(
                    "\n{}\nSpace to toggle, Enter to confirm and install\n",
                    style("Select packages:").yellow().bold()
                );

                let package_option: Vec<_> = packages
                    .iter()
                    .map(|package| {
                        let is_installed = installed_packages
                            .iter()
                            .any(|p| p.name.to_string().contains(&package.name));

                        let is_cask = package.cask.is_some();

                        let icon = if is_installed {
                            style("●").dim()
                        } else {
                            style("◯")
                        };

                        let status = if is_installed {
                            style("[installed]").dim()
                        } else {
                            style("")
                        };

                        let category = if let Some(ref cat) = package.category {
                            style(format!(" [{}]", cat)).dim()
                        } else {
                            style("".to_string())
                        };

                        let formatted = format!("{} {} {}{}", icon, package.name, status, category);

                        if is_cask {
                            if is_installed {
                                style(formatted).magenta().dim()
                            } else {
                                style(formatted).magenta()
                            }
                        } else if is_installed {
                            style(formatted).green().dim()
                        } else {
                            style(formatted).green()
                        }
                    })
                    .collect();
                let defaults: Vec<bool> = packages
                    .iter()
                    .map(|package| {
                        !installed_packages
                            .iter()
                            .any(|p| p.name.to_string().contains(&package.name))
                    })
                    .collect();
                let package_selections: Vec<usize> =
                    MultiSelect::with_theme(&MultiSelectThemeNoColon)
                        .with_prompt(prompt)
                        .items(&package_option)
                        .defaults(&defaults)
                        .interact()
                        .unwrap();

                let mut selected_packages: Vec<BrewPackage> = vec![];

                for index in &package_selections {
                    let package_clone: BrewPackage = packages[*index].clone();
                    selected_packages.push(package_clone);
                }

                if !selected_packages.is_empty() {
                    let parallel = matches.get_flag("parallel");
                    let dry_run = matches.get_flag("dry-run");
                    let webhook_url = matches.get_one::<String>("webhook").cloned();
                    let autoquit_secs = matches
                        .get_one::<String>("autoquit")
                        .and_then(|s| s.parse::<u64>().ok());

                    if dry_run {
                        print_dry_run_preview(&selected_packages, "install");
                        return;
                    }

                    let results = install_packages(&selected_packages, parallel, autoquit_secs);

                    if results.is_empty() && !selected_packages.is_empty() {
                        eprintln!(
                            "\n{} Operation cancelled by user",
                            style("✗").yellow().bold()
                        );
                        std::process::exit(130);
                    }

                    if let Some(url) = webhook_url {
                        let completed = results.iter().filter(|r| r.status == "completed").count();
                        let failed = results.iter().filter(|r| r.status == "failed").count();
                        let machine_id = matches
                            .get_one::<String>("webhook-machine-id")
                            .cloned()
                            .unwrap_or_else(default_machine_id);

                        let payload = WebhookPayload {
                            status: if failed > 0 {
                                "partial".to_string()
                            } else {
                                "success".to_string()
                            },
                            total: results.len(),
                            completed,
                            failed,
                            packages: results,
                            elapsed_seconds: start_time.elapsed().as_secs(),
                            machine_id,
                        };

                        match post_webhook(&url, payload).await {
                            Ok(_) => eprintln!("Webhook notification sent successfully"),
                            Err(e) => eprintln!("Warning: Failed to send webhook: {}", e),
                        }
                    }
                }
            }
            Err(err) => {
                eprintln!(
                    "\n{} {}",
                    style("✗").red().bold(),
                    style("Error fetching packages").red().bold()
                );
                eprintln!("  {}", err);
                eprintln!(
                    "\n{} Make sure your URL or file path is correct.",
                    style("→").yellow()
                );
            }
        }
    }

    if matches.get_flag("list") {
        print_header("Installed Packages", Color::Cyan);

        println!(
            "\n{}",
            style(format!("Total: {} packages", installed_packages.len()))
                .yellow()
                .bold()
        );
        println!();

        for (i, package) in installed_packages.iter().enumerate() {
            println!(
                "  {} {}",
                style(format!("{:3}.", i + 1)).dim(),
                style(&package.name).green()
            );
        }
        println!();
    }

    if matches.get_flag("remove") {
        print_header("Package Removal", Color::Red);

        println!(
            "\n{}",
            style("⚠ Warning: This will remove selected packages and their dependencies!")
                .yellow()
                .bold()
        );

        println!("\n{}", style("Summary:").yellow().bold());
        println!(
            "  Total installed packages: {}",
            style(installed_packages.len()).cyan().bold()
        );

        let prompt: String = format!(
            "\n{} Select packages to remove (Space to toggle, Enter to confirm):",
            style("→").red().bold()
        );
        let package_option: Vec<_> = installed_packages
            .iter()
            .map(|package| -> StyledObject<String> { style(format!("✗ {}", package.name)).red() })
            .collect();
        let package_selections: Vec<usize> = MultiSelect::new()
            .with_prompt(prompt)
            .items(&package_option)
            .interact()
            .unwrap();

        let mut selected_packages: Vec<BrewPackage> = vec![];

        for index in &package_selections {
            let package_clone: BrewPackage = installed_packages[*index].clone();
            selected_packages.push(package_clone);
        }

        if !selected_packages.is_empty() {
            let parallel = matches.get_flag("parallel");
            let dry_run = matches.get_flag("dry-run");
            let webhook_url = matches.get_one::<String>("webhook").cloned();

            if dry_run {
                print_dry_run_preview(&selected_packages, "remove");
                return;
            }

            let results = remove_packages(&selected_packages, parallel);

            if results.is_empty() && !selected_packages.is_empty() {
                eprintln!(
                    "\n{} Operation cancelled by user",
                    style("✗").yellow().bold()
                );
                std::process::exit(130);
            }

            if let Some(url) = webhook_url {
                let completed = results.iter().filter(|r| r.status == "completed").count();
                let failed = results.iter().filter(|r| r.status == "failed").count();
                let machine_id = matches
                    .get_one::<String>("webhook-machine-id")
                    .cloned()
                    .unwrap_or_else(default_machine_id);

                let payload = WebhookPayload {
                    status: if failed > 0 {
                        "partial".to_string()
                    } else {
                        "success".to_string()
                    },
                    total: results.len(),
                    completed,
                    failed,
                    packages: results,
                    elapsed_seconds: start_time.elapsed().as_secs(),
                    machine_id,
                };

                match post_webhook(&url, payload).await {
                    Ok(_) => eprintln!("Webhook notification sent successfully"),
                    Err(e) => eprintln!("Warning: Failed to send webhook: {}", e),
                }
            }
        }
    }

    if matches.get_flag("sync") {
        if let Some(urls) = matches.get_many::<String>("url") {
            let mut url_list: Vec<String> = Vec::new();
            for url_arg in urls {
                for url in url_arg.split(',') {
                    let trimmed = url.trim();
                    if !trimmed.is_empty() {
                        url_list.push(trimmed.to_string());
                    }
                }
            }

            println!("\n{} Fetching recipe files...", style("→").cyan().bold());
            match fetch_and_merge_packages(&url_list).await {
                Ok(recipe_packages) => {
                    if let Err(e) =
                        brew_recipe_lock::verify_or_update_lock(&recipe_packages, &url_list)
                    {
                        if let LockError::IntegrityMismatch { .. } = &e {
                            eprintln!(
                                "\n{} {}",
                                style("⚠").yellow().bold(),
                                style("Recipe has changed since last run.").yellow().bold()
                            );
                            let update = Confirm::with_theme(&MultiSelectThemeNoColon)
                                .with_prompt("Update lock and continue?")
                                .default(false)
                                .interact()
                                .unwrap_or(false);
                            if update {
                                if let Err(we) =
                                    brew_recipe_lock::update_lock(&recipe_packages, &url_list)
                                {
                                    eprintln!(
                                        "\n{} {}",
                                        style("✗").red().bold(),
                                        style("Failed to update lock").red().bold()
                                    );
                                    eprintln!("  {}", we);
                                    std::process::exit(1);
                                }
                            } else {
                                eprintln!(
                                    "\n{} {}",
                                    style("✗").red().bold(),
                                    style("Recipe integrity check failed").red().bold()
                                );
                                eprintln!("  {}", e);
                                std::process::exit(1);
                            }
                        } else {
                            eprintln!(
                                "\n{} {}",
                                style("✗").red().bold(),
                                style("Recipe integrity check failed").red().bold()
                            );
                            eprintln!("  {}", e);
                            std::process::exit(1);
                        }
                    }
                    let dry_run = matches.get_flag("dry-run");
                    sync_packages(&installed_packages, &recipe_packages, dry_run);
                }
                Err(err) => {
                    eprintln!(
                        "\n{} {}",
                        style("✗").red().bold(),
                        style("Error fetching packages").red().bold()
                    );
                    eprintln!("  {}", err);
                }
            }
        } else {
            eprintln!(
                "\n{} {}",
                style("✗").red().bold(),
                style("Sync requires --url flag").red().bold()
            );
            eprintln!("  Example: brim --sync --url=\"packages.json\"");
        }
    }

    eprintln!("Elapsed time: {:?} seconds", start_time.elapsed().as_secs());
}

fn sync_packages(installed: &[BrewPackage], recipe: &[BrewPackage], dry_run: bool) {
    println!(
        "\n{}",
        style("╔═══════════════════════════════════════════════════════════════════╗")
            .cyan()
            .bold()
    );
    println!(
        "{}",
        style("║         BRIM - Sync Analysis                                      ║")
            .cyan()
            .bold()
    );
    println!(
        "{}",
        style("╚═══════════════════════════════════════════════════════════════════╝")
            .cyan()
            .bold()
    );

    let to_install: Vec<&BrewPackage> = recipe
        .iter()
        .filter(|pkg| !installed.iter().any(|inst| inst.name == pkg.name))
        .collect();

    let to_remove: Vec<&BrewPackage> = installed
        .iter()
        .filter(|inst| !recipe.iter().any(|pkg| pkg.name == inst.name))
        .collect();

    let in_sync: Vec<&BrewPackage> = recipe
        .iter()
        .filter(|pkg| installed.iter().any(|inst| inst.name == pkg.name))
        .collect();

    println!("\n{}", style("═══ Summary ═══").yellow().bold());
    println!(
        "  {} In sync: {}",
        style("✓").green(),
        style(in_sync.len()).cyan().bold()
    );
    println!(
        "  {} To install: {}",
        style("+").green(),
        style(to_install.len()).cyan().bold()
    );
    println!(
        "  {} Extra (not in recipe): {}",
        style("-").red(),
        style(to_remove.len()).cyan().bold()
    );

    if !to_install.is_empty() {
        println!("\n{}", style("═══ Packages to Install ═══").green().bold());
        for (i, pkg) in to_install.iter().enumerate() {
            let category = if let Some(ref cat) = pkg.category {
                format!(" [{}]", cat)
            } else {
                String::new()
            };
            let cask_marker = if pkg.cask.is_some() { " [cask]" } else { "" };
            println!(
                "  {} {} {}{}{}",
                style(format!("{:2}.", i + 1)).dim(),
                style("+").green().bold(),
                style(&pkg.name).green(),
                style(category).dim(),
                style(cask_marker).magenta()
            );
        }
    }

    if !to_remove.is_empty() {
        println!(
            "\n{}",
            style("═══ Extra Packages (not in recipe) ═══")
                .yellow()
                .bold()
        );
        println!(
            "  {} These are installed but not in your recipe file:",
            style("ℹ").cyan()
        );
        for (i, pkg) in to_remove.iter().enumerate() {
            println!(
                "  {} {} {}",
                style(format!("{:2}.", i + 1)).dim(),
                style("-").yellow(),
                style(&pkg.name).dim()
            );
        }
    }

    if to_install.is_empty() && to_remove.is_empty() {
        println!("\n{} All packages are in sync!", style("✓").green().bold());
        println!("  {} packages match your recipe file.", in_sync.len());
    } else {
        println!();
        if dry_run {
            println!(
                "{} This is a dry-run. No changes were made.",
                style("ℹ").cyan().bold()
            );
            println!("\nTo apply changes:");
            println!(
                "  • Install missing: {} (without --sync)",
                style("brim --url=\"your-recipe.json\"").cyan()
            );
            println!(
                "  • Remove extras: {} (select manually)",
                style("brim --remove").cyan()
            );
        } else {
            println!("{} Sync analysis complete.", style("✓").green().bold());
            println!("\nTo apply changes:");
            println!(
                "  • Install missing: {} (without --sync)",
                style("brim --url=\"your-recipe.json\"").cyan()
            );
            println!(
                "  • Remove extras: {} (select manually)",
                style("brim --remove").cyan()
            );
        }
    }
    println!();
}

fn print_dry_run_preview(packages: &[BrewPackage], operation: &str) {
    println!(
        "\n{}",
        style("╔═══════════════════════════════════════════════════════════════════╗")
            .yellow()
            .bold()
    );
    println!(
        "{}",
        style("║         DRY RUN - Preview Mode                                    ║")
            .yellow()
            .bold()
    );
    println!(
        "{}",
        style("╚═══════════════════════════════════════════════════════════════════╝")
            .yellow()
            .bold()
    );

    let action = if operation == "install" {
        "installed"
    } else {
        "removed"
    };

    println!(
        "\n{} The following {} packages would be {}:",
        style("ℹ").cyan().bold(),
        packages.len(),
        style(action).yellow().bold()
    );
    println!();

    let mut formulae = vec![];
    let mut casks = vec![];

    for package in packages {
        if package.cask.is_some() {
            casks.push(&package.name);
        } else {
            formulae.push(&package.name);
        }
    }

    if !formulae.is_empty() {
        println!("  {} Formulae:", style("→").green().bold());
        for (i, name) in formulae.iter().enumerate() {
            println!(
                "    {} {}",
                style(format!("{:2}.", i + 1)).dim(),
                style(name).green()
            );
        }
        println!();
    }

    if !casks.is_empty() {
        println!("  {} Casks:", style("→").magenta().bold());
        for (i, name) in casks.iter().enumerate() {
            println!(
                "    {} {}",
                style(format!("{:2}.", i + 1)).dim(),
                style(name).magenta()
            );
        }
        println!();
    }

    println!(
        "{} No changes were made. Run without {} to execute.",
        style("✓").green().bold(),
        style("--dry-run").yellow()
    );
    println!();
}
