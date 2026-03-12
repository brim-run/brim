#[cfg(test)]
mod tests {
    use brim::models::BrewPackage;
    use brim::models::BrewPackageResult;
    use brim::sync_analysis;
    use brim::webhook::{default_machine_id, WebhookPayload};

    #[test]
    fn test_brew_package_deserialization() {
        let json = r#"{
            "name": "postgresql",
            "category": "Database",
            "url": "https://formulae.brew.sh/formula/postgresql"
        }"#;

        let package: Result<BrewPackage, _> = serde_json::from_str(json);
        assert!(package.is_ok());

        let package = package.unwrap();
        assert_eq!(package.name, "postgresql");
        assert_eq!(package.category, Some("Database".to_string()));
        assert_eq!(package.cask, None);
    }

    #[test]
    fn test_brew_package_with_cask() {
        let json = r#"{
            "name": "visual-studio-code",
            "category": "Development",
            "cask": true
        }"#;

        let package: BrewPackage = serde_json::from_str(json).unwrap();
        assert_eq!(package.name, "visual-studio-code");
        assert_eq!(package.cask, Some(true));
    }

    #[test]
    fn test_webhook_payload_success() {
        let packages = vec![
            BrewPackageResult {
                name: "postgresql".to_string(),
                status: "completed".to_string(),
            },
            BrewPackageResult {
                name: "redis".to_string(),
                status: "completed".to_string(),
            },
        ];

        let payload = WebhookPayload {
            status: "success".to_string(),
            total: 2,
            completed: 2,
            failed: 0,
            packages,
            elapsed_seconds: 120,
            machine_id: "test-machine".to_string(),
        };

        assert_eq!(payload.status, "success");
        assert_eq!(payload.total, 2);
        assert_eq!(payload.completed, 2);
        assert_eq!(payload.failed, 0);
    }

    #[test]
    fn test_webhook_payload_partial() {
        let packages = vec![
            BrewPackageResult {
                name: "postgresql".to_string(),
                status: "completed".to_string(),
            },
            BrewPackageResult {
                name: "redis".to_string(),
                status: "failed".to_string(),
            },
        ];

        let payload = WebhookPayload {
            status: "partial".to_string(),
            total: 2,
            completed: 1,
            failed: 1,
            packages,
            elapsed_seconds: 120,
            machine_id: "test-machine".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("partial"));
        assert!(json.contains("postgresql"));
        assert!(json.contains("redis"));
    }

    #[test]
    fn test_package_result_creation() {
        let result = BrewPackageResult {
            name: "test-package".to_string(),
            status: "completed".to_string(),
        };

        assert_eq!(result.name, "test-package");
        assert_eq!(result.status, "completed");
    }

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
    fn test_sync_analysis_to_install() {
        let recipe = vec![pkg("wget"), pkg("jq"), pkg("curl")];
        let installed = vec![pkg("wget")];
        let report = sync_analysis(&recipe, &installed);
        assert_eq!(report.to_install.len(), 2);
        assert_eq!(report.in_sync.len(), 1);
        assert_eq!(report.to_remove.len(), 0);
    }

    #[test]
    fn test_dry_run_separates_casks_from_formulae() {
        let packages = vec![
            pkg("wget"),
            BrewPackage {
                name: "visual-studio-code".to_string(),
                cask: Some(true),
                category: None,
                url: None,
                version: None,
            },
            pkg("jq"),
        ];
        let formulae: Vec<_> = packages.iter().filter(|p| p.cask.is_none()).collect();
        let casks: Vec<_> = packages.iter().filter(|p| p.cask.is_some()).collect();
        assert_eq!(formulae.len(), 2);
        assert_eq!(casks.len(), 1);
        assert_eq!(casks[0].name, "visual-studio-code");
    }

    #[test]
    fn test_default_machine_id_returns_nonempty() {
        assert!(!default_machine_id().is_empty());
    }

    #[test]
    fn test_sync_analysis_to_remove_identifies_extras() {
        let recipe = vec![pkg("wget")];
        let installed = vec![pkg("wget"), pkg("curl"), pkg("jq")];
        let report = sync_analysis(&recipe, &installed);
        assert_eq!(report.to_remove.len(), 2);
        assert_eq!(report.in_sync.len(), 1);
        assert_eq!(report.to_install.len(), 0);
    }
}
