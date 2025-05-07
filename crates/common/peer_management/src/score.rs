use std::time::Instant;
/// Reputation score for a peer.
#[derive(Debug, Clone, Copy)]
pub struct ReputationScore {
    /// Overall reputation score for the peer.
    score: i32,
    /// The time this score was last updated.
    last_updated: Instant,
}

impl Default for ReputationScore {
    fn default() -> Self {
        Self {
            score: 0,
            last_updated: Instant::now(),
        }
    }
}

impl ReputationScore {
    pub fn update(&mut self, new_score: i32) {
        self.score = new_score;
        self.last_updated = Instant::now();
    }

    pub fn get_score(&self) -> i32 {
        self.score
    }

    pub fn get_last_updated(&self) -> Instant {
        self.last_updated
    }
}
