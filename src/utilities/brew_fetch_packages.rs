use crate::models::BrewPackage;
use std::path::Path;

#[derive(Debug)]
pub enum FetchError {
    NetworkError(reqwest::Error),
    FileError(std::io::Error),
    ParseError(serde_json::Error),
    InvalidUrl(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FetchError::NetworkError(e) => write!(f, "Network error: {}", e),
            FetchError::FileError(e) => write!(f, "File error: {}", e),
            FetchError::ParseError(e) => write!(f, "JSON parse error: {}", e),
            FetchError::InvalidUrl(s) => write!(f, "Invalid URL or file path: {}", s),
        }
    }
}

impl From<reqwest::Error> for FetchError {
    fn from(err: reqwest::Error) -> FetchError {
        FetchError::NetworkError(err)
    }
}

impl From<std::io::Error> for FetchError {
    fn from(err: std::io::Error) -> FetchError {
        FetchError::FileError(err)
    }
}

impl From<serde_json::Error> for FetchError {
    fn from(err: serde_json::Error) -> FetchError {
        FetchError::ParseError(err)
    }
}

impl std::error::Error for FetchError {}

pub async fn fetch_packages(url: &String) -> Result<Vec<BrewPackage>, FetchError> {
    if is_local_path(url) {
        return fetch_local_file(url);
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(FetchError::InvalidUrl(format!(
            "'{}' is not a valid URL or file path. Use http://, https://, or a local file path.",
            url
        )));
    }

    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        return Err(FetchError::InvalidUrl(format!(
            "HTTP error {}: {}",
            response.status().as_u16(),
            response.status()
        )));
    }

    let text = response.text().await?;
    let packages: Vec<BrewPackage> = serde_json::from_str(&text)?;

    Ok(packages)
}

fn is_local_path(path: &str) -> bool {
    Path::new(path).exists() || !path.starts_with("http://") && !path.starts_with("https://")
}

fn fetch_local_file(path: &str) -> Result<Vec<BrewPackage>, FetchError> {
    let file_path = Path::new(path);

    if !file_path.exists() {
        return Err(FetchError::FileError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File not found: {}", path),
        )));
    }

    let content = std::fs::read_to_string(file_path)?;
    let packages: Vec<BrewPackage> = serde_json::from_str(&content)?;

    validate_packages(&packages)?;

    Ok(packages)
}

pub fn validate_packages(packages: &[BrewPackage]) -> Result<(), FetchError> {
    if packages.is_empty() {
        return Err(FetchError::InvalidUrl(
            "Recipe file contains no packages".to_string(),
        ));
    }

    for (index, package) in packages.iter().enumerate() {
        if package.name.trim().is_empty() {
            return Err(FetchError::InvalidUrl(format!(
                "Package at index {} has empty name",
                index
            )));
        }

        if !package
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(FetchError::InvalidUrl(
                format!("Package '{}' has invalid name format. Use only alphanumeric characters, dots, hyphens, and underscores.", package.name)
            ));
        }

        if let Some(ref url) = package.url {
            if !url.trim().is_empty() && !url.starts_with("http://") && !url.starts_with("https://")
            {
                return Err(FetchError::InvalidUrl(format!(
                    "Package '{}' has invalid URL: must start with http:// or https://",
                    package.name
                )));
            }
        }

        if let Some(ref version) = package.version {
            if !version.trim().is_empty() && !is_valid_version(version) {
                return Err(FetchError::InvalidUrl(
                    format!("Package '{}' has invalid version format: '{}'. Use semantic versioning (e.g., 1.0.0 or 20.0.0-alpha.1)", package.name, version)
                ));
            }
        }
    }

    Ok(())
}

fn is_valid_version(version: &str) -> bool {
    let (core, pre_release) = match version.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (version, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return false;
    }
    if !parts.iter().all(|part| part.parse::<u32>().is_ok()) {
        return false;
    }
    if let Some(pr) = pre_release {
        if pr.is_empty() {
            return false;
        }
        if !pr
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BrewPackage;

    fn validate_version(version: &str) -> Result<(), FetchError> {
        let packages = vec![BrewPackage {
            name: "test-pkg".to_string(),
            category: None,
            url: None,
            cask: None,
            version: Some(version.to_string()),
        }];
        validate_packages(&packages)
    }

    #[test]
    fn test_valid_version_plain() {
        assert!(validate_version("14.0").is_ok());
        assert!(validate_version("1.2.3").is_ok());
    }

    #[test]
    fn test_valid_version_prerelease() {
        assert!(validate_version("20.0.0-alpha.1").is_ok());
        assert!(validate_version("1.0.0-beta").is_ok());
        assert!(validate_version("2.0.0-rc.1").is_ok());
    }

    #[test]
    fn test_invalid_version() {
        assert!(validate_version("14").is_err()); // need major.minor
        assert!(validate_version("1.2.3.4").is_err()); // too many parts
        assert!(validate_version("1.0.0-").is_err()); // empty pre-release
        assert!(validate_version("a.b.c").is_err());
    }

    #[test]
    fn test_validate_accepts_cask_package() {
        let packages = vec![BrewPackage {
            name: "visual-studio-code".to_string(),
            category: None,
            url: None,
            cask: Some(true),
            version: None,
        }];
        assert!(validate_packages(&packages).is_ok());
    }
}
