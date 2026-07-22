// Shadow-simulator sim-cost backend, gated on `shadow-integration`.
//
// The real recursive-XMSS prover/verifier is too slow to run per block under the
// fuzzer. `type_2` swaps it for husk-prototype stub proofs; this module supplies
// the shared knobs — the `fake_xmss` toggle and the modeled sim-cost sleeps on
// Shadow's virtual clock. Uses a similiar approach to ethlambda and zeam.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

// Process-global config: set once via `init`, then read lock-free at every call
// site. Rates are stored as `f64` bits (no `AtomicF64`); `0` means unset/disabled.
static FAKE_ENABLED: AtomicBool = AtomicBool::new(false);
static AGG_RATE: AtomicU64 = AtomicU64::new(0);
static VERIFY_RATE: AtomicU64 = AtomicU64::new(0);
static MERGE_RATE: AtomicU64 = AtomicU64::new(0);

/// Keep only finite, strictly-positive rates; everything else collapses to `0`
/// (disabled).
fn rate_bits(v: Option<f64>) -> u64 {
    match v {
        Some(v) if v.is_finite() && v > 0.0 => v.to_bits(),
        _ => 0,
    }
}

/// Configure the backend. Call once at node startup, before any aggregation.
/// A `None`/non-positive rate disables the sleep for that operation.
pub fn init(fake: bool, agg: Option<f64>, verify: Option<f64>, merge: Option<f64>) {
    FAKE_ENABLED.store(fake, Ordering::Relaxed);
    AGG_RATE.store(rate_bits(agg), Ordering::Relaxed);
    VERIFY_RATE.store(rate_bits(verify), Ordering::Relaxed);
    MERGE_RATE.store(rate_bits(merge), Ordering::Relaxed);
}

/// Whether the fake-XMSS stub backend is active.
pub fn fake_xmss() -> bool {
    FAKE_ENABLED.load(Ordering::Relaxed)
}

/// Delay for processing `n` units at `rate` (units/sec): `n / rate` seconds, or
/// zero if the rate is disabled or `n == 0`.
fn compute_delay(rate: &AtomicU64, n: usize) -> Duration {
    let r = f64::from_bits(rate.load(Ordering::Relaxed));
    if r <= 0.0 || n == 0 {
        return Duration::ZERO;
    }

    let ns = (n as f64 / r) * 1e9;
    if !ns.is_finite() || ns <= 0.0 {
        return Duration::ZERO;
    }

    // Clamp before the f64 -> u64 cast to avoid saturating-cast surprises.
    Duration::from_nanos(ns.min(u64::MAX as f64) as u64)
}

/// Delay to model aggregating `n` raw signatures / child proofs.
pub fn aggregate_delay(n: usize) -> Duration {
    compute_delay(&AGG_RATE, n)
}

/// Delay to model verifying `n` signatures/proofs.
pub fn verify_delay(n: usize) -> Duration {
    compute_delay(&VERIFY_RATE, n)
}

/// Delay to model merging `n` proofs.
pub fn merge_delay(n: usize) -> Duration {
    compute_delay(&MERGE_RATE, n)
}

/// Advance Shadow's virtual clock by `delay`; a zero delay is a no-op (no
/// `nanosleep(0)` event).
pub fn sleep(delay: Duration) {
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
}
