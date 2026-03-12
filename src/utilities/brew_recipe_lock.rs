use crate::models::BrewPackage;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const LOCK_PREFIX: &str = "sha256:";
const LOCK_FILE_PREFIX: &str = "recipe-";

#[derive(Debug)]
pub enum LockError {
    Io(std::io::Error),
    Parse(String),
    IntegrityMismatch { expected: String, got: String },
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LockError::Io(e) => write!(f, "{}", e),
            LockError::Parse(s) => write!(f, "{}", s),
            LockError::IntegrityMismatch { expected, got } => write!(
                f,
                "Recipe integrity check failed: recipe content has changed since the last run (expected {}, got {}). \
                If you intended to change the recipe, remove the lockfile and run again.",
                expected, got
            ),
        }
    }
}

impl std::error::Error for LockError {}

impl From<std::io::Error> for LockError {
    fn from(err: std::io::Error) -> LockError {
        LockError::Io(err)
    }
}

/// Returns a canonical JSON string for the merged recipe (sorted keys) for stable hashing.
fn canonical_recipe_json(packages: &[BrewPackage]) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(packages)?;
    Ok(canonical_json_string(&value))
}

fn canonical_json_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let entries: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    let v = map.get(k).unwrap();
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap(),
                        canonical_json_string(v)
                    )
                })
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        serde_json::Value::Array(arr) => {
            let entries: Vec<String> = arr.iter().map(canonical_json_string).collect();
            format!("[{}]", entries.join(","))
        }
        other => serde_json::to_string(other).unwrap(),
    }
}

/// Builds a stable identity string for the recipe source(s): location (file vs url), path/URL, and order.
/// Used as part of the integrity hash so the same content from a different source yields a different hash.
fn recipe_identity(recipe_sources: &[String]) -> Result<String, LockError> {
    let mut parts = Vec::with_capacity(recipe_sources.len());
    for src in recipe_sources {
        let normalized = if src.starts_with("http://") || src.starts_with("https://") {
            format!("url:{}", src)
        } else {
            let path = Path::new(src);
            let abs = if path.is_absolute() {
                path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
                    .canonicalize()
                    .unwrap_or_else(|_| path.to_path_buf())
            };
            format!("file:{}", abs.display())
        };
        parts.push(normalized);
    }
    Ok(parts.join("\n"))
}

/// SHA-256 hash of (recipe identity + content). Identity is location (file/url), path/URL, and order.
/// Same content from a different file or URL produces a different hash.
/// Packages are sorted by name so hash is stable regardless of fetch order.
pub fn recipe_content_hash(
    packages: &[BrewPackage],
    recipe_sources: &[String],
) -> Result<String, LockError> {
    let identity = recipe_identity(recipe_sources)?;
    let mut sorted: Vec<BrewPackage> = packages.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let canonical = canonical_recipe_json(&sorted).map_err(|e| LockError::Parse(e.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(identity.as_bytes());
    hasher.update(b"\n");
    hasher.update(canonical.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

/// Directory containing the current executable (brim binary). Falls back to current dir if unavailable.
fn binary_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Stable filename for a recipe: SHA-256 of its identity (location + path/URL), so each recipe has one lockfile.
fn lock_filename(recipe_sources: &[String]) -> Result<String, LockError> {
    let identity = recipe_identity(recipe_sources)?;
    let mut hasher = Sha256::new();
    hasher.update(identity.as_bytes());
    Ok(format!("{}{:x}.lock", LOCK_FILE_PREFIX, hasher.finalize()))
}

/// Lockfile path: next to the brim binary, one file per recipe (filename = hash of recipe identity).
pub fn lockfile_path(recipe_sources: &[String]) -> Result<PathBuf, LockError> {
    let dir = binary_dir();
    let name = lock_filename(recipe_sources)?;
    Ok(dir.join(name))
}

/// Read the stored hash from the lockfile, if it exists.
pub fn read_lock(lock_path: &Path) -> Result<Option<String>, LockError> {
    let content = match std::fs::read_to_string(lock_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(LockError::Io(e)),
    };
    let line = content.lines().find(|l| l.starts_with(LOCK_PREFIX));
    match line {
        Some(l) => Ok(Some(l.trim_start_matches(LOCK_PREFIX).trim().to_string())),
        None => Err(LockError::Parse(format!(
            "Invalid lockfile format: {}",
            lock_path.display()
        ))),
    }
}

/// Write the hash to the lockfile.
pub fn write_lock(lock_path: &Path, hash: &str) -> Result<(), LockError> {
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(lock_path, format!("{}{}\n", LOCK_PREFIX, hash))?;
    Ok(())
}

/// Verify current recipe hash against the lockfile, or create/update the lockfile.
/// Hash is computed from recipe identity (location + path/URL) and content, so the same
/// content from a different source is considered a different recipe.
pub fn verify_or_update_lock(
    packages: &[BrewPackage],
    recipe_sources: &[String],
) -> Result<(), LockError> {
    let hash = recipe_content_hash(packages, recipe_sources)?;
    let path = lockfile_path(recipe_sources)?;

    if let Some(stored) = read_lock(&path)? {
        if stored != hash {
            return Err(LockError::IntegrityMismatch {
                expected: stored,
                got: hash,
            });
        }
    }
    write_lock(&path, &hash)?;
    Ok(())
}

/// Writes the current recipe hash to the lockfile without verifying. Use when the user
/// explicitly accepts a recipe change (e.g. added a package) so the next run sees the new content as locked.
pub fn update_lock(packages: &[BrewPackage], recipe_sources: &[String]) -> Result<(), LockError> {
    let hash = recipe_content_hash(packages, recipe_sources)?;
    let path = lockfile_path(recipe_sources)?;
    write_lock(&path, &hash)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_is_deterministic() {
        let a = BrewPackage {
            name: "foo".to_string(),
            category: Some("dev".to_string()),
            url: None,
            cask: Some(false),
            version: None,
        };
        let b = BrewPackage {
            name: "bar".to_string(),
            category: None,
            url: None,
            cask: None,
            version: Some("1.0".to_string()),
        };
        let packages = vec![a, b];
        let j1 = canonical_recipe_json(&packages).unwrap();
        let j2 = canonical_recipe_json(&packages).unwrap();
        assert_eq!(j1, j2);
    }

    #[test]
    fn hash_is_stable() {
        let packages = vec![BrewPackage {
            name: "test".to_string(),
            category: None,
            url: None,
            cask: None,
            version: None,
        }];
        let sources = vec!["https://example.com/recipe.json".to_string()];
        let h1 = recipe_content_hash(&packages, &sources).unwrap();
        let h2 = recipe_content_hash(&packages, &sources).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_differs_for_same_content_different_source() {
        let packages = vec![BrewPackage {
            name: "test".to_string(),
            category: None,
            url: None,
            cask: None,
            version: None,
        }];
        let h_file = recipe_content_hash(&packages, &["/path/to/recipe.json".to_string()]).unwrap();
        let h_url =
            recipe_content_hash(&packages, &["https://example.com/recipe.json".to_string()])
                .unwrap();
        assert_ne!(h_file, h_url, "file vs url should produce different hashes");
    }

    #[test]
    fn write_and_read_lock_roundtrip() {
        let dir = std::env::temp_dir();
        let path = dir.join("brim_test_lock_roundtrip.lock");
        let hash = "deadbeefcafe1234";
        write_lock(&path, hash).unwrap();
        let stored = read_lock(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(stored, Some(hash.to_string()));
    }
}
