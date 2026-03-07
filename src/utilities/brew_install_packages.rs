use crate::constants::PROGRAM;
use crate::models::BrewPackage;
use crate::models::BrewPackageResult;
use crate::tui::{ProgressState, ProgressTracker};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn install_packages(
    packages: &[BrewPackage],
    parallel: bool,
    autoquit_secs: Option<u64>,
) -> Vec<BrewPackageResult> {
    let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();

    let mut tracker = match ProgressTracker::new(package_names, autoquit_secs) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to initialize TUI: {}", e);
            return vec![];
        }
    };

    let packages_arc = Arc::new(Mutex::new(packages.to_owned()));
    let tracker_packages = tracker.get_packages();
    let tracker_packages_for_result = Arc::clone(&tracker_packages);

    if parallel {
        return parallel_download_sequential_install(packages_arc, tracker_packages, &mut tracker);
    } else if false {
        let cancelled = Arc::new(AtomicBool::new(false));

        let install_threads: Vec<_> = {
            let packages = packages_arc.lock().unwrap();

            packages
                .iter()
                .enumerate()
                .map(|(index, package)| {
                    let package = package.clone();
                    let tracker_packages = Arc::clone(&tracker_packages);
                    let cancelled = Arc::clone(&cancelled);

                    thread::spawn(move || {
                        install_single_package(index, &package, &tracker_packages, &cancelled);
                    })
                })
                .collect()
        };

        let cancelled_clone = Arc::clone(&cancelled);
        let install_completed =
            tracker.run_with_updates(|| install_threads.iter().all(|t| t.is_finished()));

        if install_completed.is_err() || !install_completed.unwrap_or(true) {
            cancelled_clone.store(true, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(200));
        }

        for thread in install_threads {
            let _ = thread.join();
        }
    } else {
        let cancelled = Arc::new(AtomicBool::new(false));

        let install_thread = {
            let packages_arc = Arc::clone(&packages_arc);
            let tracker_packages = Arc::clone(&tracker_packages);
            let cancelled = Arc::clone(&cancelled);

            thread::spawn(move || {
                let packages = packages_arc.lock().unwrap();

                for (index, package) in packages.iter().enumerate() {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    install_single_package(index, package, &tracker_packages, &cancelled);
                }
            })
        };

        let cancelled_clone = Arc::clone(&cancelled);
        let install_completed = tracker.run_with_updates(|| install_thread.is_finished());

        if install_completed.is_err() || !install_completed.unwrap_or(true) {
            cancelled_clone.store(true, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(200));
        }

        let _ = install_thread.join();
    }

    collect_results(&tracker_packages_for_result)
}

/// Headless install: no TUI. Runs the same brew commands and returns results.
/// Use from scripts or other processes.
pub fn install_packages_headless(
    packages: &[BrewPackage],
    parallel: bool,
) -> Vec<BrewPackageResult> {
    if parallel {
        headless_parallel_fetch_then_sequential_install(packages)
    } else {
        packages.iter().map(run_one_install_headless).collect()
    }
}

fn run_one_install_headless(package: &BrewPackage) -> BrewPackageResult {
    let spec = super::brew_common::brew_package_spec(package);
    let mut command = Command::new(PROGRAM);
    command.arg("install").arg(&spec);
    if package.cask.is_some() {
        command.arg("--cask");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let status = match command.status() {
        Ok(s) => s,
        Err(e) => {
            return BrewPackageResult {
                name: package.name.clone(),
                status: format!("error: {}", e),
            };
        }
    };

    let status_str = if status.success() {
        "completed"
    } else {
        "failed"
    };
    BrewPackageResult {
        name: package.name.clone(),
        status: status_str.to_string(),
    }
}

fn headless_parallel_fetch_then_sequential_install(
    packages: &[BrewPackage],
) -> Vec<BrewPackageResult> {
    let fetch_handles: Vec<_> = packages
        .iter()
        .map(|package| {
            let spec = super::brew_common::brew_package_spec(package);
            let is_cask = package.cask.is_some();
            thread::spawn(move || {
                let mut cmd = Command::new(PROGRAM);
                cmd.arg("fetch").arg(&spec);
                if is_cask {
                    cmd.arg("--cask");
                }
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                cmd.status().map(|s| s.success()).unwrap_or(false)
            })
        })
        .collect();

    let fetch_ok: Vec<bool> = fetch_handles
        .into_iter()
        .map(|h| h.join().unwrap_or(false))
        .collect();

    packages
        .iter()
        .zip(fetch_ok.iter())
        .map(|(package, &ok)| {
            if ok {
                run_one_install_headless(package)
            } else {
                BrewPackageResult {
                    name: package.name.clone(),
                    status: "failed".to_string(),
                }
            }
        })
        .collect()
}

fn collect_results(
    tracker_packages: &Arc<Mutex<Vec<crate::tui::progress::PackageProgress>>>,
) -> Vec<BrewPackageResult> {
    if let Ok(packages) = tracker_packages.lock() {
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

fn parallel_download_sequential_install(
    packages_arc: Arc<Mutex<Vec<BrewPackage>>>,
    tracker_packages: Arc<Mutex<Vec<crate::tui::progress::PackageProgress>>>,
    tracker: &mut ProgressTracker,
) -> Vec<BrewPackageResult> {
    let packages = packages_arc.lock().unwrap().clone();
    let cancelled = Arc::new(AtomicBool::new(false));

    let download_threads: Vec<_> = packages
        .iter()
        .enumerate()
        .map(|(index, package)| {
            let package = package.clone();
            let tracker_packages = Arc::clone(&tracker_packages);
            let cancelled = Arc::clone(&cancelled);

            thread::spawn(move || {
                if let Ok(mut tracked) = tracker_packages.lock() {
                    if let Some(p) = tracked.get_mut(index) {
                        p.state = ProgressState::Downloading;
                        p.progress = 0;
                        p.message = "Fetching...".to_string();
                    }
                }

                let mut command = Command::new(PROGRAM);
                command.arg("fetch");

                if package.cask.is_some() {
                    command.arg("--cask");
                }

                let spec = super::brew_common::brew_package_spec(&package);
                command
                    .arg(&spec)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                let mut child = match command.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        if let Ok(mut tracked) = tracker_packages.lock() {
                            if let Some(p) = tracked.get_mut(index) {
                                p.state = ProgressState::Failed;
                                p.message = format!("Spawn error: {}", e);
                            }
                        }
                        return false;
                    }
                };

                let mut wait_count = 0;
                let fetch_result = loop {
                    if cancelled.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        break None;
                    }

                    match child.try_wait() {
                        Ok(Some(status)) => {
                            break Some(status.success());
                        }
                        Ok(None) => {
                            wait_count += 1;
                            if wait_count > 1200 {
                                let _ = child.kill();
                                if let Ok(mut tracked) = tracker_packages.lock() {
                                    if let Some(p) = tracked.get_mut(index) {
                                        p.state = ProgressState::Failed;
                                        p.message = "Fetch timeout".to_string();
                                    }
                                }
                                break None;
                            }

                            if wait_count % 10 == 0 {
                                let progress = ((wait_count as f32 / 1200.0) * 90.0) as u16;
                                if let Ok(mut tracked) = tracker_packages.try_lock() {
                                    if let Some(p) = tracked.get_mut(index) {
                                        p.progress = progress;
                                    }
                                }
                            }

                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(_) => break None,
                    }
                };

                match fetch_result {
                    Some(true) => {
                        if let Ok(mut tracked) = tracker_packages.lock() {
                            if let Some(p) = tracked.get_mut(index) {
                                p.progress = 100;
                                p.message = "Downloaded".to_string();
                            }
                        }
                        true
                    }
                    Some(false) => {
                        if let Ok(mut tracked) = tracker_packages.lock() {
                            if let Some(p) = tracked.get_mut(index) {
                                p.state = ProgressState::Failed;
                                p.message = "Download failed".to_string();
                            }
                        }
                        false
                    }
                    None => false,
                }
            })
        })
        .collect();

    let download_check_thread = Arc::new(Mutex::new(Some(thread::spawn(move || {
        for thread in download_threads {
            let _ = thread.join();
        }
    }))));

    let download_check_clone = Arc::clone(&download_check_thread);
    let cancelled_clone = Arc::clone(&cancelled);
    let download_completed = tracker.run_without_summary(|| {
        if let Ok(guard) = download_check_clone.lock() {
            if let Some(thread) = guard.as_ref() {
                return thread.is_finished();
            }
        }
        true
    });

    let user_cancelled = download_completed.is_err() || !download_completed.unwrap_or(true);
    if user_cancelled {
        cancelled_clone.store(true, Ordering::Relaxed);
        thread::sleep(Duration::from_millis(200));
    }

    if let Ok(mut guard) = download_check_thread.lock() {
        if let Some(thread) = guard.take() {
            let _ = thread.join();
        }
    }

    if user_cancelled {
        return vec![];
    }

    let install_thread = {
        let tracker_packages = Arc::clone(&tracker_packages);
        let cancelled = Arc::clone(&cancelled);

        thread::spawn(move || {
            for (index, package) in packages.iter().enumerate() {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let should_install = if let Ok(tracked) = tracker_packages.lock() {
                    if let Some(p) = tracked.get(index) {
                        p.state != ProgressState::Failed
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !should_install {
                    continue;
                }

                install_single_package(index, package, &tracker_packages, &cancelled);
            }
        })
    };

    let cancelled_clone = Arc::clone(&cancelled);
    let install_completed = tracker.run_with_updates(|| install_thread.is_finished());

    if install_completed.is_err() || !install_completed.unwrap_or(true) {
        cancelled_clone.store(true, Ordering::Relaxed);
        thread::sleep(Duration::from_millis(200));
    }

    let _ = install_thread.join();

    if let Ok(packages) = tracker_packages.lock() {
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

fn install_single_package(
    index: usize,
    package: &BrewPackage,
    tracker_packages: &Arc<Mutex<Vec<crate::tui::progress::PackageProgress>>>,
    cancelled: &Arc<AtomicBool>,
) {
    if let Ok(mut tracked) = tracker_packages.lock() {
        if let Some(p) = tracked.get_mut(index) {
            p.state = ProgressState::Downloading;
            p.progress = 0;
            p.message = "Starting...".to_string();
        }
    }

    thread::sleep(Duration::from_millis(200));

    let spec = super::brew_common::brew_package_spec(package);
    let mut command = Command::new(PROGRAM);
    command.arg("install").arg(&spec);

    if package.cask.is_some() {
        command.arg("--cask");
    }

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            if let Ok(mut tracked) = tracker_packages.lock() {
                if let Some(p) = tracked.get_mut(index) {
                    p.state = ProgressState::Failed;
                    p.progress = 0;
                    p.message = format!("Error: {}", e);
                }
            }
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let tracker_packages_clone = Arc::clone(tracker_packages);

    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let progress = estimate_progress(&line);
            let state = if line.contains("Downloading") || line.contains("download") {
                ProgressState::Downloading
            } else {
                ProgressState::Installing
            };

            if let Ok(mut tracked) = tracker_packages_clone.try_lock() {
                if let Some(p) = tracked.get_mut(index) {
                    p.state = state;
                    p.progress = progress;
                    if !line.trim().is_empty() && line.len() < 50 {
                        p.message = line.trim().to_string();
                    }
                }
            }
        }
    });

    let tracker_packages_clone = Arc::clone(tracker_packages);
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if !line.trim().is_empty() && line.len() < 50 {
                if let Ok(mut tracked) = tracker_packages_clone.try_lock() {
                    if let Some(p) = tracked.get_mut(index) {
                        if p.state != ProgressState::Failed {
                            p.message = line.trim().to_string();
                        }
                    }
                }
            }
        }
    });

    #[allow(unused_assignments)]
    let mut status = None;
    let mut wait_count = 0;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            status = Some(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Cancelled by user",
            )));
            break;
        }

        match child.try_wait() {
            Ok(Some(exit_status)) => {
                status = Some(Ok(exit_status));
                break;
            }
            Ok(None) => {
                wait_count += 1;
                if wait_count > 1800 {
                    let _ = child.kill();
                    status = Some(Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Installation timed out after 3 minutes",
                    )));
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                status = Some(Err(e));
                break;
            }
        }
    }

    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    let status = status.unwrap();

    if let Ok(mut tracked) = tracker_packages.lock() {
        if let Some(p) = tracked.get_mut(index) {
            match status {
                Ok(exit_status) if exit_status.success() => {
                    p.state = ProgressState::Completed;
                    p.progress = 100;
                    p.message = "Done!".to_string();
                }
                Ok(_) => {
                    p.state = ProgressState::Failed;
                    p.progress = 0;
                    p.message = "Installation failed".to_string();
                }
                Err(e) => {
                    p.state = ProgressState::Failed;
                    p.progress = 0;
                    p.message = format!("Error: {}", e);
                }
            }
        }
    }

    thread::sleep(Duration::from_millis(100));
}

fn estimate_progress(line: &str) -> u16 {
    if let Some(pos) = line.find('%') {
        let before = &line[..pos];
        if let Some(num_start) = before.rfind(|c: char| !c.is_ascii_digit()) {
            if let Ok(percent) = before[num_start + 1..].parse::<u16>() {
                return percent.min(100);
            }
        }
    }

    if line.contains("Fetch") || line.contains("fetch") {
        return 10;
    } else if line.contains("Download") || line.contains("download") {
        return 30;
    } else if line.contains("Installing") || line.contains("install") {
        return 60;
    } else if line.contains("Pouring") || line.contains("pour") {
        return 80;
    } else if line.contains("Complete") || line.contains("complete") {
        return 100;
    }

    50
}
