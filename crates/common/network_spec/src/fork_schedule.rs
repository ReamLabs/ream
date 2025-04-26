use std::slice::Iter;

use alloy_primitives::fixed_bytes;
use ream_consensus::fork::Fork;

macro_rules! fork_array {
    // Entry
    (
        ( $first_ver:literal , $first_epoch:expr )
        $( , ( $rest_ver:literal , $rest_epoch:expr ) )* $(,)?
    ) => {
        fork_array!(@internal (
            Fork {
                previous_version: fixed_bytes!($first_ver),
                current_version:  fixed_bytes!($first_ver),
                epoch:            $first_epoch,
            }
        ), $first_ver $( , $rest_ver , $rest_epoch )* )
    };

    // Recursive case
    (@internal (
        $( $forks:expr ),*
    ), $prev_ver:literal , $curr_ver:literal , $curr_epoch:expr
       $( , $tail_ver:literal , $tail_epoch:expr )* ) => {
        fork_array!(@internal (
            $( $forks ),* ,
            Fork {
                previous_version: fixed_bytes!($prev_ver),
                current_version:  fixed_bytes!($curr_ver),
                epoch:            $curr_epoch,
            }
        ), $curr_ver $( , $tail_ver , $tail_epoch )* )
    };

    // Final case
    (@internal (
        $( $forks:expr ),*
    ), $last_ver:literal ) => {
        [ $( $forks ),* ]
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkSchedule(pub [Fork; ForkSchedule::TOTAL]);

impl ForkSchedule {
    pub const TOTAL: usize = 6;

    pub const fn new(forks: [Fork; ForkSchedule::TOTAL]) -> Self {
        Self(forks)
    }

    pub fn iter(&self) -> Iter<'_, Fork> {
        self.0.iter()
    }

    pub fn scheduled(&self) -> impl Iterator<Item = &Fork> {
        self.iter()
            .filter(|fork| fork.epoch != Fork::UNSCHEDULED_EPOCH)
    }
}

pub const MAINNET_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x00000000", 0),       // Phase0
    ("0x01000000", 74_240),  // Altair
    ("0x02000000", 144_896), // Bellatrix
    ("0x03000000", 194_048), // Capella
    ("0x04000000", 269_568), // Deneb
    ("0x05000000", 364_032), // Electra
));

pub const HOLESKY_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x01017000", 0),       // Phase0
    ("0x02017000", 0),       // Altair
    ("0x03017000", 0),       // Bellatrix
    ("0x04017000", 256),     // Capella
    ("0x05017000", 29_696),  // Deneb
    ("0x06017000", 115_968), // Electra
));

pub const SEPOLIA_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x90000069", 0),       // Phase0
    ("0x90000070", 50),      // Altair
    ("0x90000071", 100),     // Bellatrix
    ("0x90000072", 56_832),  // Capella
    ("0x90000073", 132_608), // Deneb
    ("0x90000074", 222_464), // Electra
));

pub const HOODI_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x10000910", 0),     // Phase0
    ("0x20000910", 0),     // Altair
    ("0x30000910", 0),     // Bellatrix
    ("0x40000910", 0),     // Capella
    ("0x50000910", 0),     // Deneb
    ("0x60000910", 2_048), // Electra
));

pub const DEV_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x00000000", 0),                       // Phase0
    ("0x01000000", 74_240),                  // Altair
    ("0x02000000", 144_896),                 // Bellatrix
    ("0x03000000", 194_048),                 // Capella
    ("0x04000000", 269_568),                 // Deneb
    ("0x05000000", Fork::UNSCHEDULED_EPOCH), // Electra
));

#[cfg(test)]
mod tests {
    use alloy_primitives::fixed_bytes;
    use ream_consensus::fork::Fork;

    #[test]
    fn test_fork_array() {
        let expected = [
            Fork {
                previous_version: fixed_bytes!("0x90000069"),
                current_version: fixed_bytes!("0x90000069"),
                epoch: 0,
            },
            Fork {
                previous_version: fixed_bytes!("0x90000069"),
                current_version: fixed_bytes!("0x90000070"),
                epoch: 50,
            },
            Fork {
                previous_version: fixed_bytes!("0x90000070"),
                current_version: fixed_bytes!("0x90000071"),
                epoch: 100,
            },
        ];

        assert_eq!(
            fork_array!(("0x90000069", 0), ("0x90000070", 50), ("0x90000071", 100)),
            expected
        );
    }
}
