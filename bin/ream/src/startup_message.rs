use ream_node::version::{
    BUILD_ARCHITECTURE, BUILD_OPERATING_SYSTEM, PROGRAMMING_LANGUAGE_VERSION, REAM_FULL_COMMIT,
    VERGEN_GIT_DESCRIBE,
};

const REAM_CARGO_FEATURES: &str = env!("REAM_CARGO_FEATURES");

pub fn startup_message() -> String {
    let cargo_features = if REAM_CARGO_FEATURES.is_empty() {
        "none"
    } else {
        REAM_CARGO_FEATURES
    };
    format!(
        "
 ███████████   ██████████   █████████   ██████   ██████
▒▒███▒▒▒▒▒███ ▒▒███▒▒▒▒▒█  ███▒▒▒▒▒███ ▒▒██████ ██████ 
 ▒███    ▒███  ▒███  █ ▒  ▒███    ▒███  ▒███▒█████▒███ 
 ▒██████████   ▒██████    ▒███████████  ▒███▒▒███ ▒███ 
 ▒███▒▒▒▒▒███  ▒███▒▒█    ▒███▒▒▒▒▒███  ▒███ ▒▒▒  ▒███ 
 ▒███    ▒███  ▒███ ▒   █ ▒███    ▒███  ▒███      ▒███ 
 █████   █████ ██████████ █████   █████ █████     █████
▒▒▒▒▒   ▒▒▒▒▒ ▒▒▒▒▒▒▒▒▒▒ ▒▒▒▒▒   ▒▒▒▒▒ ▒▒▒▒▒     ▒▒▒▒▒ 
                                                        
 GIT_DESCRIBE     : {VERGEN_GIT_DESCRIBE}
 Full Commit      : {REAM_FULL_COMMIT}
 Build Platform   : {BUILD_OPERATING_SYSTEM}-{BUILD_ARCHITECTURE}
 Compiler Version : rustc{PROGRAMMING_LANGUAGE_VERSION}
 Cargo Features   : {cargo_features}
"
    )
}
