use std::sync::Arc;

use ream_network_state_lean::NetworkState;
use tracing::info;

pub struct AggregatorController {
    network_state: Arc<NetworkState>,
}

impl AggregatorController {
    pub fn new(network_state: Arc<NetworkState>) -> Self {
        Self { network_state }
    }

    pub fn is_enabled(&self) -> bool {
        *self.network_state.is_aggregator.lock()
    }

    pub fn set_enabled(&self, enabled: bool) -> bool {
        let mut lock = self.network_state.is_aggregator.lock();
        let previous = *lock;

        if previous != enabled {
            *lock = enabled;
            info!(
                "Aggregator role {} via admin API",
                if enabled { "activated" } else { "deactivated" }
            );
        }
        previous
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ream_consensus_lean::checkpoint::Checkpoint;
    use ream_network_state_lean::NetworkState;

    use super::*;

    fn setup_controller(initial: bool) -> AggregatorController {
        let network_state = Arc::new(NetworkState::new(
            Checkpoint::default(),
            Checkpoint::default(),
            initial,
        ));
        AggregatorController::new(network_state)
    }

    #[test]
    fn test_is_enabled_reflects_sync_service_flag() {
        let controller = setup_controller(false);
        assert!(!controller.is_enabled());

        {
            let mut lock = controller.network_state.is_aggregator.lock();
            *lock = true;
        }
        assert!(controller.is_enabled());
    }

    #[test]
    fn test_set_enabled_activates_role() {
        let controller = setup_controller(false);

        let previous = controller.set_enabled(true);

        assert!(!previous);
        assert!(controller.is_enabled());
        assert!(*controller.network_state.is_aggregator.lock());
    }

    #[test]
    fn test_set_enabled_deactivates_role() {
        let controller = setup_controller(true);

        let previous = controller.set_enabled(false);

        assert!(previous);
        assert!(!controller.is_enabled());
        assert!(!*controller.network_state.is_aggregator.lock());
    }

    #[test]
    fn test_set_enabled_idempotent() {
        let controller = setup_controller(true);

        let previous = controller.set_enabled(true);

        assert!(previous);
        assert!(controller.is_enabled());
        assert!(*controller.network_state.is_aggregator.lock());
    }

    #[test]
    fn test_sequential_toggles_converge() {
        let controller = setup_controller(false);

        let r1 = controller.set_enabled(true);
        let r2 = controller.set_enabled(false);
        let r3 = controller.set_enabled(true);

        assert!(r1);
        assert!(r2);
        assert!(r3);

        assert!(controller.is_enabled());
        assert!(*controller.network_state.is_aggregator.lock());
    }
}
