use std::slice::Iter;

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
    ), $prev_ver:literal , $curr_ver:literal , $curr_epoch:expr $( , $tail_ver:literal , $tail_epoch:expr )* ) => {
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
            fork_array!(("0x90000069", 0), ("0x90000070", 50), ("0x90000071", 100),),
            expected
        );
    }
}

pub mod dev;
pub mod holesky;
pub mod hoodi;
pub mod mainnet;
pub mod sepolia;

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
