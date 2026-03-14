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

pub async fn fetch_and_merge_packages(
    urls: &[String],
) -> Result<Vec<BrewPackage>, super::brew_fetch_packages::FetchError> {
    use super::brew_fetch_packages::FetchError;

    if urls.is_empty() {
        return Err(FetchError::InvalidUrl("No URLs provided".to_string()));
    }

    let mut all_packages: HashMap<String, BrewPackage> = HashMap::new();

    for url in urls {
        match super::brew_fetch_packages::fetch_packages(url).await {
            Ok(packages) => {
                for package in packages {
                    all_packages.insert(package.name.clone(), package);
                }
            }
            Err(e) => {
                return Err(FetchError::FetchFailed {
                    url: url.clone(),
                    source: Box::new(e),
                })
            }
        }
    }

    Ok(all_packages.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BrewPackage;

    fn pkg(name: &str) -> BrewPackage {
        BrewPackage {
            name: name.to_string(),
            category: None,
            url: None,
            cask: None,
            version: None,
        }
    }

    #[test]
    fn brew_package_spec_appends_version_when_set() {
        let p = BrewPackage {
            version: Some("14.0".to_string()),
            ..pkg("postgresql")
        };
        assert_eq!(brew_package_spec(&p), "postgresql@14.0");
    }

    #[tokio::test]
    async fn fetch_and_merge_deduplicates_by_name() {
        let dir = std::env::temp_dir();
        let p1 = dir.join("brim_test_merge_r1.json");
        let p2 = dir.join("brim_test_merge_r2.json");
        std::fs::write(&p1, r#"[{"name":"wget"},{"name":"curl"}]"#).unwrap();
        std::fs::write(&p2, r#"[{"name":"wget"},{"name":"jq"}]"#).unwrap();

        let urls = vec![
            p1.to_str().unwrap().to_string(),
            p2.to_str().unwrap().to_string(),
        ];
        let result = fetch_and_merge_packages(&urls).await.unwrap();

        let _ = std::fs::remove_file(p1);
        let _ = std::fs::remove_file(p2);

        // wget appears in both files; the merged set should have exactly 3 unique packages.
        assert_eq!(result.len(), 3);
    }
}
