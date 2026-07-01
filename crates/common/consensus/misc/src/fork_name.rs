use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

#[derive(
    Debug, Clone, Copy, Decode, Encode, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String")]
#[serde(into = "String")]
#[ssz(enum_behaviour = "tag")]
pub enum ForkName {
    Base,
    Altair,
    Bellatrix,
    Capella,
    Deneb,
    Electra,
    Fulu,
}

impl ForkName {
    pub fn list_all() -> Vec<ForkName> {
        vec![
            ForkName::Base,
            ForkName::Altair,
            ForkName::Bellatrix,
            ForkName::Capella,
            ForkName::Deneb,
            ForkName::Electra,
            ForkName::Fulu,
        ]
    }

    /// Return the name of the fork immediately prior to the current one.
    pub fn previous_fork(self) -> Option<ForkName> {
        match self {
            ForkName::Base => None,
            ForkName::Altair => Some(ForkName::Base),
            ForkName::Bellatrix => Some(ForkName::Altair),
            ForkName::Capella => Some(ForkName::Bellatrix),
            ForkName::Deneb => Some(ForkName::Capella),
            ForkName::Electra => Some(ForkName::Deneb),
            ForkName::Fulu => Some(ForkName::Electra),
        }
    }

    /// Return the name of the fork immediately after the current one.
    pub fn next_fork(self) -> Option<ForkName> {
        match self {
            ForkName::Base => Some(ForkName::Altair),
            ForkName::Altair => Some(ForkName::Bellatrix),
            ForkName::Bellatrix => Some(ForkName::Capella),
            ForkName::Capella => Some(ForkName::Deneb),
            ForkName::Deneb => Some(ForkName::Electra),
            ForkName::Electra => Some(ForkName::Fulu),
            ForkName::Fulu => None,
        }
    }

    pub fn altair_enabled(self) -> bool {
        self >= ForkName::Altair
    }

    pub fn bellatrix_enabled(self) -> bool {
        self >= ForkName::Bellatrix
    }

    pub fn capella_enabled(self) -> bool {
        self >= ForkName::Capella
    }

    pub fn deneb_enabled(self) -> bool {
        self >= ForkName::Deneb
    }

    pub fn electra_enabled(self) -> bool {
        self >= ForkName::Electra
    }

    pub fn fulu_enabled(self) -> bool {
        self >= ForkName::Fulu
    }

    pub fn is_enabled_at(&self, fork: ForkName) -> bool {
        *self >= fork
    }
}

impl FromStr for ForkName {
    type Err = String;

    fn from_str(fork_name: &str) -> Result<Self, String> {
        Ok(match fork_name.to_lowercase().as_ref() {
            "phase0" | "base" => ForkName::Base,
            "altair" => ForkName::Altair,
            "bellatrix" | "merge" => ForkName::Bellatrix,
            "capella" => ForkName::Capella,
            "deneb" => ForkName::Deneb,
            "electra" => ForkName::Electra,
            "fulu" => ForkName::Fulu,
            _ => return Err(format!("unknown fork name: {fork_name}")),
        })
    }
}

impl Display for ForkName {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            ForkName::Base => "phase0".fmt(f),
            ForkName::Altair => "altair".fmt(f),
            ForkName::Bellatrix => "bellatrix".fmt(f),
            ForkName::Capella => "capella".fmt(f),
            ForkName::Deneb => "deneb".fmt(f),
            ForkName::Electra => "electra".fmt(f),
            ForkName::Fulu => "fulu".fmt(f),
        }
    }
}

impl From<ForkName> for String {
    fn from(fork: ForkName) -> String {
        fork.to_string()
    }
}

impl TryFrom<String> for ForkName {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}
