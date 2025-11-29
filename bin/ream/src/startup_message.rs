use ream_node::version::{
    BUILD_ARCHITECTURE, BUILD_OPERATING_SYSTEM, PROGRAMMING_LANGUAGE_VERSION, REAM_FULL_COMMIT,
    VERGEN_GIT_DESCRIBE,
};

pub fn startup_message() -> String {
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
"
    )
}
