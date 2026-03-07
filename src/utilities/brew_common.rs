use crate::models::BrewPackage;
use std::collections::HashMap;

pub(crate) fn brew_package_spec(package: &BrewPackage) -> String {
    match &package.version {
        Some(v) if !v.trim().is_empty() => format!("{}@{}", package.name, v.trim()),
        _ => package.name.clone(),
    }
}

pub fn header_lines(title: &str) -> (String, String, String) {
    let version = env!("CARGO_PKG_VERSION");
    let title_width = 49usize.saturating_sub(version.len());
    let top = "╔═══════════════════════════════════════════════════════════════════╗".to_string();
    let middle = format!("║         BRIM v{} - {:<title_width$}║", version, title);
    let bottom =
        "╚═══════════════════════════════════════════════════════════════════╝".to_string();
    (top, middle, bottom)
}

pub async fn fetch_and_merge_packages(urls: &[String]) -> Result<Vec<BrewPackage>, String> {
    if urls.is_empty() {
        return Err("No URLs provided".to_string());
    }

    let mut all_packages: HashMap<String, BrewPackage> = HashMap::new();

    for url in urls {
        match super::brew_fetch_packages::fetch_packages(url).await {
            Ok(packages) => {
                for package in packages {
                    all_packages.insert(package.name.clone(), package);
                }
            }
            Err(e) => return Err(format!("Failed to fetch from {}: {}", url, e)),
        }
    }

    Ok(all_packages.into_values().collect())
}
