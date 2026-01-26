use std::{env, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    let mut cargo_features: Vec<String> = env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .map(|feature| feature.to_ascii_lowercase())
        .collect();
    cargo_features.sort();
    let cargo_features = cargo_features.join(",");

    println!("cargo:rustc-env=REAM_CARGO_FEATURES={cargo_features}");

    Ok(())
}
