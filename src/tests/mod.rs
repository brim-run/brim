#[cfg(test)]
mod tests {
    use brim::models::BrewPackage;
    use brim::models::BrewPackageResult;
    use brim::webhook::WebhookPayload;

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
}
