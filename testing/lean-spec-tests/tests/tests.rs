use std::{env, fs, path::PathBuf};

use lean_spec_tests::fork_choice::{load_fork_choice_test, run_fork_choice_test};
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
async fn test_all_fork_choice_fixtures() {
    // Initialize tracing subscriber for test output
    let env_filter = match env::var(EnvFilter::DEFAULT_ENV) {
        Ok(filter) => EnvFilter::builder().parse_lossy(filter),
        Err(_) => EnvFilter::new("info"),
    };
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let fixtures = find_json_files("fixtures/consensus/fork_choice");

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
