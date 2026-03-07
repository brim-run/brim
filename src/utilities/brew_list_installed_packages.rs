use crate::constants::PROGRAM;
use crate::models::BrewPackage;
use std::process::{exit, Command, Stdio};

pub fn list_installed_packages() -> Vec<BrewPackage> {
    let output = Command::new(PROGRAM)
        .arg("list")
        .stdout(Stdio::piped())
        .output();

    match output {
        Ok(output) => {
            let stdout = &output.stdout.clone();
            let result: Vec<BrewPackage> = String::from_utf8_lossy(stdout)
                .lines()
                .map(|s| BrewPackage {
                    name: s.to_string(),
                    category: None,
                    url: None,
                    cask: None,
                    version: None,
                })
                .collect();

            result
        }
        Err(error) => {
            eprintln!("Failed to execute command: {}", error);
            exit(1);
        }
    }
}
