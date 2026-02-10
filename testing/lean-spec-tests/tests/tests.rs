use std::{env, fs, path::PathBuf};

#[cfg(feature = "devnet2")]
use lean_spec_tests::fork_choice::{load_fork_choice_test, run_fork_choice_test};
use lean_spec_tests::ssz_test::{load_ssz_test, run_ssz_test};
use lean_spec_tests::state_transition::{load_state_transition_test, run_state_transition_test};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

/// Helper to find all JSON files in a directory recursively
fn find_json_files(dir: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir);

    if !base_path.exists() {
        warn!("Directory does not exist: {}", base_path.display());
        return files;
    }

    fn visit_dirs(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit_dirs(&path, files);
                } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    files.push(path);
                }
            }
        }
    }

    visit_dirs(&base_path, &mut files);
    files.sort();
    files
}

#[tokio::test]
#[cfg(feature = "devnet2")]
async fn test_all_fork_choice_fixtures() {
    // Initialize tracing subscriber for test output
    let env_filter = match env::var(EnvFilter::DEFAULT_ENV) {
        Ok(filter) => EnvFilter::builder().parse_lossy(filter),
        Err(_) => EnvFilter::new("info"),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();

    let fixtures = find_json_files("fixtures/devnet2/fork_choice");

    if fixtures.is_empty() {
        info!(
            "No fork choice fixtures found. Skipping tests. Run 'make test' in lean-spec-tests to download fixtures."
        );
        return;
    }

    info!("Found {} fork choice test fixtures", fixtures.len());

    let mut total_tests = 0;
    let mut passed = 0;
    let mut failed = 0;

    for fixture_path in fixtures {
        debug!("\n=== Loading fixture: {:?} ===", fixture_path.file_name());

        match load_fork_choice_test(&fixture_path) {
            Ok(fixture) => {
                for (test_name, test) in fixture {
                    total_tests += 1;
                    info!("Starting test: {}", test_name);
                    match run_fork_choice_test(&test_name, test).await {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {}", test_name);
                        }
                        Err(err) => {
                            failed += 1;
                            error!("FAILED: {test_name} - {err:?}");
                        }
                    }
                }
            }
            Err(err) => {
                error!("Failed to load fixture {fixture_path:?}: {err:?}");
                failed += 1;
            }
        }
    }

    info!("\n=== Fork Choice Test Summary ===");
    info!("Total tests: {total_tests}");
    info!("Passed: {passed}");
    info!("Failed: {failed}");

    assert_eq!(failed, 0, "Some fork choice tests failed");
}

#[test]
fn test_all_state_transition_fixtures() {
    // Initialize tracing subscriber for test output
    let env_filter = match env::var(EnvFilter::DEFAULT_ENV) {
        Ok(filter) => EnvFilter::builder().parse_lossy(filter),
        Err(_) => EnvFilter::new("info"),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();

    let fixtures = find_json_files("fixtures/devnet2/state_transition");

    if fixtures.is_empty() {
        info!(
            "No state transition fixtures found. Skipping tests. Run 'make test' in lean-spec-tests to download fixtures."
        );
        return;
    }

    info!("Found {} state transition test fixtures", fixtures.len());

    let mut total_tests = 0;
    let mut passed = 0;
    let mut failed = 0;

    for fixture_path in fixtures {
        debug!("\n=== Loading fixture: {:?} ===", fixture_path.file_name());

        match load_state_transition_test(&fixture_path) {
            Ok(fixture) => {
                for (test_name, test) in &fixture {
                    total_tests += 1;
                    info!("Starting test: {}", test_name);
                    match run_state_transition_test(test_name, test) {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {}", test_name);
                        }
                        Err(err) => {
                            failed += 1;
                            error!("FAILED: {test_name} - {err:?}");
                        }
                    }
                }
            }
            Err(err) => {
                error!("Failed to load fixture {fixture_path:?}: {err:?}");
                failed += 1;
            }
        }
    }

    info!("\n=== State Transition Test Summary ===");
    info!("Total tests: {total_tests}");
    info!("Passed: {passed}");
    info!("Failed: {failed}");

    assert_eq!(failed, 0, "Some state transition tests failed");
}

#[test]
fn test_all_ssz_fixtures() {
    // Initialize tracing subscriber for test output
    let env_filter = match env::var(EnvFilter::DEFAULT_ENV) {
        Ok(filter) => EnvFilter::builder().parse_lossy(filter),
        Err(_) => EnvFilter::new("info"),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();

    #[cfg(feature = "devnet2")]
    let fixtures = find_json_files("fixtures/devnet2/ssz/devnet");
    #[cfg(feature = "devnet3")]
    let fixtures = find_json_files("fixtures/devnet3/ssz/devnet");

    if fixtures.is_empty() {
        info!(
            "No SSZ fixtures found. Skipping tests. Run 'make test' in lean-spec-tests to download fixtures."
        );
        return;
    }

    info!("Found {} SSZ test fixtures", fixtures.len());

    let mut total_tests = 0;
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for fixture_path in fixtures {
        debug!("\n=== Loading fixture: {:?} ===", fixture_path.file_name());

        match load_ssz_test(&fixture_path) {
            Ok(fixture) => {
                for (test_name, test) in &fixture {
                    total_tests += 1;
                    info!("Starting test: {}", test_name);
                    match run_ssz_test(test_name, test) {
                        Ok(_) => {
                            // Check if the test was skipped (logs contain "Skipping")
                            passed += 1;
                            info!("PASSED: {}", test_name);
                        }
                        Err(err) => {
                            failed += 1;
                            error!("FAILED: {test_name} - {err:?}");
                        }
                    }
                }
            }
            Err(err) => {
                error!("Failed to load fixture {fixture_path:?}: {err:?}");
                failed += 1;
            }
        }
    }

    info!("\n=== SSZ Test Summary ===");
    info!("Total tests: {total_tests}");
    info!("Passed: {passed}");
    info!("Skipped: {skipped}");
    info!("Failed: {failed}");

    assert_eq!(failed, 0, "Some SSZ tests failed");
}
