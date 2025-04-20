use std::{array::from_fn, sync::LazyLock};

use alloy_primitives::{aliases::B32, fixed_bytes};
use ream_consensus::fork::Fork;

pub const TOTAL_FORKS: usize = 6;
pub const UNSCHEDULED_FORK_EPOCH: u64 = u64::MAX;

const MAINNET_SPECS: [(B32, u64); TOTAL_FORKS] = [
    (fixed_bytes!("0x00000000"), 0),                      // Phase0
    (fixed_bytes!("0x01000000"), 74_240),                 // Altair
    (fixed_bytes!("0x02000000"), 144_896),                // Bellatrix
    (fixed_bytes!("0x03000000"), 194_048),                // Capella
    (fixed_bytes!("0x04000000"), 269_568),                // Deneb
    (fixed_bytes!("0x05000000"), UNSCHEDULED_FORK_EPOCH), // Electra
];
const HOLESKY_SPECS: [(B32, u64); TOTAL_FORKS] = [
    (fixed_bytes!("0x01017000"), 0),       // Phase0
    (fixed_bytes!("0x02017000"), 0),       // Altair
    (fixed_bytes!("0x03017000"), 0),       // Bellatrix
    (fixed_bytes!("0x04017000"), 256),     // Capella
    (fixed_bytes!("0x05017000"), 29_696),  // Deneb
    (fixed_bytes!("0x06017000"), 115_968), // Electra
];
const SEPOLIA_SPECS: [(B32, u64); TOTAL_FORKS] = [
    (fixed_bytes!("0x90000069"), 0),       // Phase0
    (fixed_bytes!("0x90000070"), 50),      // Altair
    (fixed_bytes!("0x90000071"), 100),     // Bellatrix
    (fixed_bytes!("0x90000072"), 56_832),  // Capella
    (fixed_bytes!("0x90000073"), 132_608), // Deneb
    (fixed_bytes!("0x90000074"), 222_464), // Electra
];
const HOODI_SPECS: [(B32, u64); TOTAL_FORKS] = [
    (fixed_bytes!("0x10000910"), 0),     // Phase0
    (fixed_bytes!("0x20000910"), 0),     // Altair
    (fixed_bytes!("0x30000910"), 0),     // Bellatrix
    (fixed_bytes!("0x40000910"), 0),     // Capella
    (fixed_bytes!("0x50000910"), 0),     // Deneb
    (fixed_bytes!("0x60000910"), 2_048), // Electra
];

fn build_fork_array<const N: usize>(specs: &[(B32, u64); N]) -> [Fork; N] {
    from_fn(|i| {
        let (current, epoch) = specs[i];
        let previous = if i == 0 { current } else { specs[i - 1].0 };
        Fork {
            previous_version: previous,
            current_version: current,
            epoch,
        }
    })
}

pub static MAINNET_FORKS: LazyLock<[Fork; TOTAL_FORKS]> =
    LazyLock::new(|| build_fork_array(&MAINNET_SPECS));
pub static HOLESKY_FORKS: LazyLock<[Fork; TOTAL_FORKS]> =
    LazyLock::new(|| build_fork_array(&HOLESKY_SPECS));
pub static SEPOLIA_FORKS: LazyLock<[Fork; TOTAL_FORKS]> =
    LazyLock::new(|| build_fork_array(&SEPOLIA_SPECS));
pub static HOODI_FORKS: LazyLock<[Fork; TOTAL_FORKS]> =
    LazyLock::new(|| build_fork_array(&HOODI_SPECS));
pub static DEV_FORKS: LazyLock<[Fork; TOTAL_FORKS]> =
    LazyLock::new(|| build_fork_array(&MAINNET_SPECS));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_fork_array() {
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
            Fork {
                previous_version: fixed_bytes!("0x90000071"),
                current_version: fixed_bytes!("0x90000072"),
                epoch: 56_832,
            },
            Fork {
                previous_version: fixed_bytes!("0x90000072"),
                current_version: fixed_bytes!("0x90000073"),
                epoch: 132_608,
            },
            Fork {
                previous_version: fixed_bytes!("0x90000073"),
                current_version: fixed_bytes!("0x90000074"),
                epoch: 222_464,
            },
        ];

        assert_eq!(
            build_fork_array(&[
                (fixed_bytes!("0x90000069"), 0),       // Phase0
                (fixed_bytes!("0x90000070"), 50),      // Altair
                (fixed_bytes!("0x90000071"), 100),     // Bellatrix
                (fixed_bytes!("0x90000072"), 56_832),  // Capella
                (fixed_bytes!("0x90000073"), 132_608), // Deneb
                (fixed_bytes!("0x90000074"), 222_464), // Electra
            ]),
            expected
        );
    }
}
