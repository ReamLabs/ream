use std::{env, fs, path::PathBuf};

#[cfg(feature = "devnet5")]
use lean_spec_tests::fork_choice::{load_fork_choice_test, run_fork_choice_test};
use lean_spec_tests::{
    justifiability::{load_justifiability_test, run_justifiability_test},
    slot_clock::{load_slot_clock_test, run_slot_clock_test},
    state_transition::{load_state_transition_test, run_state_transition_test},
    sync::{load_sync_test, run_sync_test},
};
#[cfg(feature = "devnet5")]
use lean_spec_tests::{
    ssz::{load_ssz_test, run_ssz_test},
    verify_signatures::{load_verify_signatures_test, run_verify_signatures_test},
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

const FIXTURE_PROFILE: &str = "lstar";

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

fn find_fixture_files(suite: &str) -> Vec<PathBuf> {
    let network = "devnet5";
    find_json_files(&format!(
        "fixtures/{network}/consensus/{suite}/{FIXTURE_PROFILE}"
    ))
}

fn should_run_fixture_suite(suite: &str, fixtures: &[PathBuf]) -> bool {
    if !fixtures.is_empty() {
        return true;
    }

    let message = format!(
        "No {suite} fixtures found for {FIXTURE_PROFILE}. Run 'make test' in lean-spec-tests to download fixtures."
    );

    if cfg!(feature = "lean-spec-tests") {
        panic!("{message}");
    }

    info!("{message} Skipping tests.");
    false
}

fn init_tracing() {
    let env_filter = match env::var(EnvFilter::DEFAULT_ENV) {
        Ok(filter) => EnvFilter::builder().parse_lossy(filter),
        Err(_) => EnvFilter::new("info"),
    };
    if let Err(err) = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init()
    {
        warn!("Failed to initialize tracing subscriber: {err}");
    }
}

#[test]
fn test_all_state_transition_fixtures() {
    init_tracing();

    let fixtures = find_fixture_files("state_transition");

    if !should_run_fixture_suite("state_transition", &fixtures) {
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
                    info!("Starting test: {test_name}");
                    match run_state_transition_test(test_name, test) {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {test_name}");
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

#[cfg(feature = "devnet5")]
#[test]
fn test_all_ssz_fixtures() {
    init_tracing();

    let fixtures = find_fixture_files("ssz");

    if !should_run_fixture_suite("ssz", &fixtures) {
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
                        Ok(true) => {
                            passed += 1;
                            info!("PASSED: {test_name}");
                        }
                        Ok(false) => {
                            skipped += 1;
                            info!("SKIPPED: {test_name}");
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

#[cfg(feature = "devnet5")]
#[tokio::test]
async fn test_all_fork_choice_fixtures() {
    init_tracing();

    let fixtures = find_fixture_files("fork_choice");

    if !should_run_fixture_suite("fork_choice", &fixtures) {
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
                    info!("Starting test: {test_name}");
                    match run_fork_choice_test(&test_name, test).await {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {test_name}");
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
fn test_all_justifiability_fixtures() {
    init_tracing();

    let fixtures = find_fixture_files("justifiability");

    if !should_run_fixture_suite("justifiability", &fixtures) {
        return;
    }

    info!("Found {} justifiability test fixtures", fixtures.len());

    let mut total_tests = 0;
    let mut passed = 0;
    let mut failed = 0;

    for fixture_path in fixtures {
        debug!("\n=== Loading fixture: {:?} ===", fixture_path.file_name());

        match load_justifiability_test(&fixture_path) {
            Ok(fixture) => {
                for (test_name, test) in &fixture {
                    total_tests += 1;
                    info!("Starting test: {test_name}");
                    match run_justifiability_test(test_name, test) {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {test_name}");
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

    info!("\n=== Justifiability Test Summary ===");
    info!("Total tests: {total_tests}");
    info!("Passed: {passed}");
    info!("Failed: {failed}");

    assert_eq!(failed, 0, "Some justifiability tests failed");
}

#[test]
fn test_all_slot_clock_fixtures() {
    init_tracing();

    let fixtures = find_fixture_files("slot_clock");

    if !should_run_fixture_suite("slot_clock", &fixtures) {
        return;
    }

    info!("Found {} slot_clock test fixtures", fixtures.len());

    let mut total_tests = 0;
    let mut passed = 0;
    let mut failed = 0;

    for fixture_path in fixtures {
        debug!("\n=== Loading fixture: {:?} ===", fixture_path.file_name());

        match load_slot_clock_test(&fixture_path) {
            Ok(fixture) => {
                for (test_name, test) in &fixture {
                    total_tests += 1;
                    info!("Starting test: {test_name}");
                    match run_slot_clock_test(test_name, test) {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {test_name}");
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

    info!("\n=== Slot Clock Test Summary ===");
    info!("Total tests: {total_tests}");
    info!("Passed: {passed}");
    info!("Failed: {failed}");

    assert_eq!(failed, 0, "Some slot_clock tests failed");
}

#[cfg(feature = "devnet5")]
#[test]
fn test_all_verify_signatures_fixtures() {
    init_tracing();

    let fixtures = find_fixture_files("verify_signatures");

    if !should_run_fixture_suite("verify_signatures", &fixtures) {
        return;
    }

    info!("Found {} verify_signatures test fixtures", fixtures.len());

    let mut total_tests = 0;
    let mut passed = 0;
    let mut failed = 0;

    for fixture_path in fixtures {
        debug!("\n=== Loading fixture: {:?} ===", fixture_path.file_name());

        match load_verify_signatures_test(&fixture_path) {
            Ok(fixture) => {
                for (test_name, test) in &fixture {
                    total_tests += 1;
                    info!("Starting test: {test_name}");
                    match run_verify_signatures_test(test_name, test) {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {test_name}");
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

    info!("\n=== Verify Signatures Test Summary ===");
    info!("Total tests: {total_tests}");
    info!("Passed: {passed}");
    info!("Failed: {failed}");

    assert_eq!(failed, 0, "Some verify_signatures tests failed");
}

#[test]
fn test_all_sync_fixtures() {
    init_tracing();

    let fixtures = find_fixture_files("sync");

    if !should_run_fixture_suite("sync", &fixtures) {
        return;
    }

    info!("Found {} sync test fixtures", fixtures.len());

    let mut total_tests = 0;
    let mut passed = 0;
    let mut failed = 0;

    for fixture_path in fixtures {
        debug!("\n=== Loading fixture: {:?} ===", fixture_path.file_name());

        match load_sync_test(&fixture_path) {
            Ok(fixture) => {
                for (test_name, test) in &fixture {
                    total_tests += 1;
                    info!("Starting test: {test_name}");
                    match run_sync_test(test_name, test) {
                        Ok(_) => {
                            passed += 1;
                            info!("PASSED: {test_name}");
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

    info!("\n=== Sync Test Summary ===");
    info!("Total tests: {total_tests}");
    info!("Passed: {passed}");
    info!("Failed: {failed}");

    assert_eq!(failed, 0, "Some sync tests failed");
}
