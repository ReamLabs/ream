#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Verbosity {
    pub fn directive(&self) -> String {
        const FILTERS: &str =
            ",actix_server=warn,discv5=error,air=error,rec_aggregation=error,sub_protocols=error";

        match self {
            Verbosity::Error => format!("error{FILTERS}"),
            Verbosity::Warn => format!("warn{FILTERS}"),
            Verbosity::Info => format!("info{FILTERS}"),
            Verbosity::Debug => "debug".to_string(),
            Verbosity::Trace => "trace".to_string(),
        }
    }
}

pub fn verbosity_parser(s: &str) -> Result<Verbosity, String> {
    let level = s.parse::<u8>().map_err(|err| err.to_string())?;

    match level {
        1 => Ok(Verbosity::Error),
        2 => Ok(Verbosity::Warn),
        3 => Ok(Verbosity::Info),
        4 => Ok(Verbosity::Debug),
        5 => Ok(Verbosity::Trace),
        _ => Err(format!("verbosity must be between 1 and 5, got {level}")),
    }
}
