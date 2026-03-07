use crate::constants::PROGRAM;
use crate::models::BrewPackage;
use crate::models::BrewPackageResult;
use crate::tui::{ProgressState, ProgressTracker};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Shared per-package removal: brew remove -f then autoremove on success. Used by headless and TUI.
fn remove_one_package(package: &BrewPackage) -> BrewPackageResult {
    let spec = super::brew_common::brew_package_spec(package);
    let status = match Command::new(PROGRAM)
        .arg("remove")
        .arg("-f")
        .arg(&spec)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            return BrewPackageResult {
                name: package.name.clone(),
                status: format!("failed: {}", e),
            };
        }
    };
    if status.success() {
        let _ = Command::new(PROGRAM)
            .arg("autoremove")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        BrewPackageResult {
            name: package.name.clone(),
            status: "completed".to_string(),
        }
    } else {
        BrewPackageResult {
            name: package.name.clone(),
            status: "failed".to_string(),
        }
    }
}

/// Removes packages without TUI (headless). Suitable for MCP and scripts.
pub fn remove_packages_headless(packages: &[BrewPackage]) -> Vec<BrewPackageResult> {
    packages.iter().map(remove_one_package).collect()
}

pub fn remove_packages(packages: &[BrewPackage], _parallel: bool) -> Vec<BrewPackageResult> {
    let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();

    let mut tracker = match ProgressTracker::new(package_names, None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to initialize TUI: {}", e);
            return vec![];
        }
    };

    let packages_arc = Arc::new(Mutex::new(packages.to_owned()));
    let tracker_packages = tracker.get_packages();
    let tracker_packages_for_result = Arc::clone(&tracker_packages);
    let cancelled = Arc::new(AtomicBool::new(false));

    let remove_threads: Vec<_> = {
        let packages = packages_arc.lock().unwrap();

        packages
            .iter()
            .enumerate()
            .map(|(index, package)| {
                let package = package.clone();
                let tracker_packages = Arc::clone(&tracker_packages);

                thread::spawn(move || {
                    if let Ok(mut tracked) = tracker_packages.lock() {
                        if let Some(p) = tracked.get_mut(index) {
                            p.state = ProgressState::Removing;
                            p.progress = 10;
                            p.message = "Removing...".to_string();
                        }
                    }

                    thread::sleep(Duration::from_millis(200));

                    let result = remove_one_package(&package);

                    if let Ok(mut tracked) = tracker_packages.lock() {
                        if let Some(p) = tracked.get_mut(index) {
                            if result.status == "completed" {
                                p.state = ProgressState::Completed;
                                p.progress = 100;
                                p.message = "Removed!".to_string();
                            } else {
                                p.state = ProgressState::Failed;
                                p.progress = 0;
                                p.message = result.status.clone();
                            }
                        }
                    }

                    thread::sleep(Duration::from_millis(100));
                })
            })
            .collect()
    };

    let cancelled_clone = Arc::clone(&cancelled);
    let removal_completed =
        tracker.run_with_updates(|| remove_threads.iter().all(|t| t.is_finished()));

    if removal_completed.is_err() || !removal_completed.unwrap_or(true) {
        cancelled_clone.store(true, Ordering::Relaxed);
        thread::sleep(Duration::from_millis(200));
    }

    for thread in remove_threads {
        let _ = thread.join();
    }

    let guard = tracker_packages_for_result.lock();
    if let Ok(packages) = guard {
        packages
            .iter()
            .map(|p| BrewPackageResult {
                name: p.name.clone(),
                status: p.state_label().to_string(),
            })
            .collect()
    } else {
        vec![]
    }
}
