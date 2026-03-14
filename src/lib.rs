pub mod constants;
pub mod models;
pub mod tui;
pub mod utilities;
pub mod webhook;
pub use models::{BrewPackage, BrewPackageResult, BrewSyncReport, Recipe};
pub use utilities::brew_fetch_packages::FetchError;
pub use utilities::brew_recipe_lock::{update_lock, verify_or_update_lock, LockError};

/// Validates recipe JSON string. Returns `Ok(())` if valid.
pub fn validate_recipe_json(json: &str) -> Result<(), FetchError> {
    let packages: Vec<BrewPackage> = serde_json::from_str(json).map_err(FetchError::ParseError)?;
    utilities::brew_fetch_packages::validate_packages(&packages)
}

/// Validates a slice of packages (e.g. after loading a recipe).
pub fn validate_packages(packages: &[BrewPackage]) -> Result<(), FetchError> {
    utilities::brew_fetch_packages::validate_packages(packages)
}

/// Fetches packages from a single URL or local file path.
pub async fn fetch_packages(url: &str) -> Result<Vec<BrewPackage>, FetchError> {
    utilities::brew_fetch_packages::fetch_packages(&url.to_string()).await
}

/// Fetches and merges multiple recipe URLs/paths. Later entries override earlier ones by package name.
pub async fn fetch_and_merge_packages(urls: &[String]) -> Result<Recipe, FetchError> {
    utilities::brew_common::fetch_and_merge_packages(urls).await
}

/// Returns the list of currently installed Homebrew packages (formulae and casks).
pub fn list_installed_packages() -> Vec<BrewPackage> {
    utilities::brew_list_installed_packages::list_installed_packages()
}

/// Compares recipe packages with installed packages. Returns what to install, remove, and what's in sync.
pub fn sync_analysis(
    recipe_packages: &[BrewPackage],
    installed_packages: &[BrewPackage],
) -> BrewSyncReport {
    let to_install: Vec<BrewPackage> = recipe_packages
        .iter()
        .filter(|p| {
            let spec = utilities::brew_common::brew_package_spec(p);
            !installed_packages.iter().any(|i| i.name == spec)
        })
        .cloned()
        .collect();
    let to_remove: Vec<BrewPackage> = installed_packages
        .iter()
        .filter(|i| {
            !recipe_packages
                .iter()
                .any(|p| utilities::brew_common::brew_package_spec(p) == i.name)
        })
        .cloned()
        .collect();
    let in_sync: Vec<BrewPackage> = recipe_packages
        .iter()
        .filter(|p| {
            let spec = utilities::brew_common::brew_package_spec(p);
            installed_packages.iter().any(|i| i.name == spec)
        })
        .cloned()
        .collect();
    BrewSyncReport {
        to_install,
        to_remove,
        in_sync,
    }
}

/// Installs packages without TUI (headless). Suitable for scripts and other processes.
pub fn install_packages_headless(
    packages: &[BrewPackage],
    parallel: bool,
) -> Vec<BrewPackageResult> {
    utilities::brew_install_packages::install_packages_headless(packages, parallel)
}

/// Removes packages without TUI (headless). Suitable for MCP and scripts.
pub fn remove_packages_headless(packages: &[BrewPackage]) -> Vec<BrewPackageResult> {
    utilities::brew_remove_packages::remove_packages_headless(packages)
}
