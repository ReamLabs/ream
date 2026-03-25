pub mod forward_background_syncer;
pub mod job;
pub mod strategy;

use std::time::{Duration, Instant};

use alloy_primitives::B256;
use libp2p_identity::PeerId;
use ream_consensus_lean::checkpoint::Checkpoint;

use crate::sync::job::{queue::JobQueue, request::JobRequest};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SyncStatus {
    Synced,
    Syncing,
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct BackfillState {
    pub jobs: Vec<JobQueue>,
}

#[derive(Debug, Clone)]
pub struct QueueRecovery {
    pub starting_root: B256,
    pub starting_slot: u64,
    pub job_roots: Vec<B256>,
    pub peer_ids: Vec<PeerId>,
    pub restart_checkpoint: Option<Checkpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueAbsorption {
    pub absorbed_starting_root: B256,
    pub absorbed_starting_slot: u64,
    pub merged_job_count: usize,
}

impl BackfillState {
    pub fn remove_processed_queue(&mut self, starting_root: B256) {
        self.jobs
            .retain(|queue| queue.starting_root != starting_root);
    }

    pub fn get_ready_to_process_queue(&self) -> Option<JobQueue> {
        self.jobs
            .iter()
            .enumerate()
            .filter(|(_, queue)| queue.is_complete)
            .min_by_key(|(_, queue)| queue.starting_slot)
            .and_then(|(index, _)| self.jobs.get(index).cloned())
    }

    pub fn is_root_start_of_any_queue(&self, root: &B256) -> bool {
        if self.jobs.len() < 2 {
            return false;
        }

        self.jobs.iter().any(|queue| &queue.starting_root == root)
    }

    pub fn mark_job_queue_as_complete(&mut self, last_root: B256) {
        for queue in &mut self.jobs {
            if queue.jobs.contains_key(&last_root) {
                queue.is_complete = true;
                queue.touch_progress();
                queue.jobs.clear();
                break;
            }
        }
    }

    pub fn absorb_queue_frontier(
        &mut self,
        current_job_root: B256,
        current_slot: u64,
        absorbed_queue_root: B256,
    ) -> Option<QueueAbsorption> {
        let current_index = self
            .jobs
            .iter()
            .position(|queue| queue.jobs.contains_key(&current_job_root))?;
        let absorbed_index = self
            .jobs
            .iter()
            .position(|queue| queue.starting_root == absorbed_queue_root && !queue.is_complete)?;

        if current_index == absorbed_index {
            return None;
        }

        let absorption = {
            let (current_queue, absorbed_queue) = if current_index < absorbed_index {
                let (left, right) = self.jobs.split_at_mut(absorbed_index);
                (&mut left[current_index], &mut right[0])
            } else {
                let (left, right) = self.jobs.split_at_mut(current_index);
                (&mut right[0], &mut left[absorbed_index])
            };

            if current_queue.jobs.remove(&current_job_root).is_none()
                || absorbed_queue.jobs.is_empty()
            {
                return None;
            }

            let absorbed_starting_root = absorbed_queue.starting_root;
            let absorbed_starting_slot = absorbed_queue.starting_slot;
            let merged_job_count = absorbed_queue.jobs.len();
            current_queue.last_fetched_slot = current_slot.min(absorbed_queue.last_fetched_slot);
            current_queue.touch_progress();
            current_queue
                .jobs
                .extend(std::mem::take(&mut absorbed_queue.jobs));

            QueueAbsorption {
                absorbed_starting_root,
                absorbed_starting_slot,
                merged_job_count,
            }
        };

        self.jobs.remove(absorbed_index);
        Some(absorption)
    }

    pub fn add_new_job_queue(
        &mut self,
        checkpoint: Checkpoint,
        job: JobRequest,
        bypass_slot_check: bool,
    ) -> bool {
        if !bypass_slot_check
            && self
                .jobs
                .iter()
                .any(|queue| checkpoint.slot <= queue.starting_slot)
        {
            return false;
        }

        if self
            .jobs
            .iter()
            .any(|queue| queue.starting_root == checkpoint.root)
        {
            return false;
        }

        let mut new_queue = JobQueue::new(checkpoint.root, checkpoint.slot, checkpoint.slot);
        new_queue.add_job(job);
        self.jobs.push(new_queue);
        true
    }

    pub fn slot_is_subset_of_any_queue(&self, slot: u64) -> bool {
        self.jobs.iter().any(|queue| slot <= queue.starting_slot)
    }

    pub fn reset_or_initialize_next_job(
        &mut self,
        last_root: B256,
        last_slot: u64,
        new_job: JobRequest,
    ) -> Option<JobRequest> {
        for queue in &mut self.jobs {
            if let Some(old_job) = queue.jobs.remove(&last_root) {
                queue.last_fetched_slot = last_slot;
                queue.touch_progress();
                queue.jobs.insert(new_job.root, new_job);
                return Some(old_job);
            }
        }

        None
    }

    pub fn reset_job_with_new_peer_id(
        &mut self,
        old_peer_id: PeerId,
        new_peer_id: PeerId,
    ) -> Option<JobRequest> {
        for queue in &mut self.jobs {
            for job in queue.jobs.values_mut() {
                if job.peer_id == old_peer_id {
                    let old_job = job.clone();
                    job.peer_id = new_peer_id;
                    job.has_been_requested = false;
                    job.time_requested = None;
                    return Some(old_job);
                }
            }
        }

        None
    }

    pub fn replace_job_with_next_job(
        &mut self,
        last_root: B256,
        last_slot: u64,
        new_job: JobRequest,
    ) -> Option<JobRequest> {
        for queue in &mut self.jobs {
            if queue.is_complete {
                continue;
            }
            if let Some(old_job) = queue.jobs.remove(&last_root) {
                queue.last_fetched_slot = last_slot;
                queue.touch_progress();
                queue.jobs.insert(new_job.root, new_job);
                return Some(old_job);
            }
        }

        None
    }

    pub fn unqueued_jobs(&self) -> Vec<JobRequest> {
        let mut unqueued_jobs = Vec::new();

        for queue in &self.jobs {
            for job in queue.jobs.values() {
                if !job.has_been_requested {
                    unqueued_jobs.push(job.clone());
                }
            }
        }

        unqueued_jobs
    }

    pub fn contains_job_root(&self, root: B256) -> bool {
        self.jobs.iter().any(|queue| queue.jobs.contains_key(&root))
    }

    pub fn peer_for_job_root(&self, root: B256) -> Option<PeerId> {
        for queue in &self.jobs {
            if let Some(job) = queue.jobs.get(&root) {
                return Some(job.peer_id);
            }
        }

        None
    }

    pub fn reset_timed_out_jobs(&mut self, timeout: Duration) -> Vec<JobRequest> {
        let mut timed_out_jobs = Vec::new();
        for queue in &mut self.jobs {
            for job in queue.jobs.values_mut() {
                if job.has_been_requested
                    && let Some(time_requested) = job.time_requested
                    && time_requested.elapsed() >= timeout
                {
                    timed_out_jobs.push(job.clone());
                    job.has_been_requested = false;
                    job.time_requested = None;
                }
            }
        }
        timed_out_jobs
    }

    pub fn request_latency_for_root(&self, root: B256) -> Option<Duration> {
        for queue in &self.jobs {
            if let Some(job) = queue.jobs.get(&root)
                && let Some(requested_at) = job.time_requested
            {
                let now = Instant::now();
                return Some(now.saturating_duration_since(requested_at));
            }
        }

        None
    }

    pub fn has_job_for_peer(&self, peer_id: PeerId) -> bool {
        self.jobs
            .iter()
            .any(|queue| queue.jobs.values().any(|job| job.peer_id == peer_id))
    }

    pub fn mark_job_as_requested(&mut self, root: B256) {
        for queue in &mut self.jobs {
            if let Some(job) = queue.jobs.get_mut(&root) {
                job.mark_requested();
                return;
            }
        }
    }

    pub fn recover_stalled_queues(&mut self, timeout: Duration) -> Vec<QueueRecovery> {
        let mut recoveries = Vec::new();
        let now = Instant::now();
        let complete_slots: Vec<u64> = self
            .jobs
            .iter()
            .filter(|queue| queue.is_complete)
            .map(|queue| queue.starting_slot)
            .collect();
        let mut stalled_roots = Vec::new();

        for queue in &self.jobs {
            if queue.is_complete
                || queue.jobs.is_empty()
                || now.saturating_duration_since(queue.last_progress_at) < timeout
            {
                continue;
            }

            let superseded_by_complete_queue = complete_slots
                .iter()
                .any(|slot| *slot >= queue.starting_slot);
            let restart_checkpoint = if superseded_by_complete_queue {
                None
            } else {
                queue.jobs.values().next().map(|job| Checkpoint {
                    root: job.root,
                    slot: queue.last_fetched_slot.saturating_sub(1),
                })
            };

            recoveries.push(QueueRecovery {
                starting_root: queue.starting_root,
                starting_slot: queue.starting_slot,
                job_roots: queue.jobs.keys().copied().collect(),
                peer_ids: queue.jobs.values().map(|job| job.peer_id).collect(),
                restart_checkpoint,
            });
            stalled_roots.push(queue.starting_root);
        }

        self.jobs
            .retain(|queue| !stalled_roots.contains(&queue.starting_root));

        recoveries
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use libp2p_identity::PeerId;

    use super::*;

    fn mock_root(byte_value: u8) -> B256 {
        let mut bytes = [0u8; 32];
        bytes[31] = byte_value;
        B256::from(bytes)
    }

    #[test]
    fn test_backfill_state_add_new_job_queue() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();

        let root_middle = mock_root(1);
        let checkpoint_middle = Checkpoint {
            root: root_middle,
            slot: 100,
        };
        let job_middle = JobRequest::new(peer_id, root_middle);
        assert!(state.add_new_job_queue(checkpoint_middle, job_middle, false));

        let root_low = mock_root(2);
        let checkpoint_low = Checkpoint {
            root: root_low,
            slot: 50,
        };
        let job_low = JobRequest::new(peer_id, root_low);
        assert!(
            !state.add_new_job_queue(checkpoint_low, job_low.clone(), false),
            "Should NOT allow adding lower slot without bypass flag"
        );

        assert!(
            state.add_new_job_queue(checkpoint_low, job_low, true),
            "Should allow adding lower slot with bypass flag"
        );

        let root_high = mock_root(3);
        let checkpoint_high = Checkpoint {
            root: root_high,
            slot: 200,
        };
        let job_high = JobRequest::new(peer_id, root_high);
        assert!(state.add_new_job_queue(checkpoint_high, job_high, false));
        assert_eq!(state.jobs.len(), 3);
    }

    #[test]
    fn test_replace_job_with_next_job() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root_original = mock_root(1);

        let checkpoint = Checkpoint {
            root: root_original,
            slot: 100,
        };
        let job_original = JobRequest::new(peer_id, root_original);
        state.add_new_job_queue(checkpoint, job_original, false);

        let parent_root = mock_root(2);
        let new_job_request = JobRequest::new(peer_id, parent_root);
        let replaced_job = state.replace_job_with_next_job(root_original, 99, new_job_request);

        assert!(replaced_job.is_some());
        assert_eq!(replaced_job.unwrap().root, root_original);

        let queue = &state.jobs[0];
        assert!(!queue.jobs.contains_key(&root_original));
        assert!(queue.jobs.contains_key(&parent_root));
        assert_eq!(queue.last_fetched_slot, 99);
    }

    #[test]
    fn test_mark_job_queue_as_complete() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root = mock_root(1);

        state.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );

        state.mark_job_queue_as_complete(root);
        assert!(state.jobs[0].is_complete);
        assert!(state.jobs[0].jobs.is_empty());
    }

    #[test]
    fn test_get_ready_to_process_queue_ordering() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();

        let root_200 = mock_root(2);
        state.add_new_job_queue(
            Checkpoint {
                root: root_200,
                slot: 200,
            },
            JobRequest::new(peer_id, root_200),
            false,
        );

        let root_100 = mock_root(1);
        state.add_new_job_queue(
            Checkpoint {
                root: root_100,
                slot: 100,
            },
            JobRequest::new(peer_id, root_100),
            true,
        );

        state.mark_job_queue_as_complete(root_200);
        state.mark_job_queue_as_complete(root_100);

        let queue_1 = state.get_ready_to_process_queue().unwrap();
        assert_eq!(queue_1.starting_slot, 100);
        state.remove_processed_queue(root_100);

        let queue_2 = state.get_ready_to_process_queue().unwrap();
        assert_eq!(queue_2.starting_slot, 200);
        state.remove_processed_queue(root_200);

        assert!(state.get_ready_to_process_queue().is_none());
    }

    #[test]
    fn test_get_ready_to_process_queue_skips_older_incomplete_queue() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();

        let root_older = mock_root(1);
        state.add_new_job_queue(
            Checkpoint {
                root: root_older,
                slot: 100,
            },
            JobRequest::new(peer_id, root_older),
            false,
        );

        let root_newer = mock_root(2);
        state.add_new_job_queue(
            Checkpoint {
                root: root_newer,
                slot: 200,
            },
            JobRequest::new(peer_id, root_newer),
            false,
        );

        state.mark_job_queue_as_complete(root_newer);

        let queue = state.get_ready_to_process_queue().unwrap();
        assert_eq!(queue.starting_slot, 200);
        assert_eq!(queue.starting_root, root_newer);
    }

    #[test]
    fn test_recover_stalled_queue_restarts_from_current_missing_root() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root = mock_root(1);

        state.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );

        state.jobs[0].last_fetched_slot = 99;
        state.jobs[0].last_progress_at = Instant::now() - Duration::from_secs(10);

        let recoveries = state.recover_stalled_queues(Duration::from_secs(1));
        assert_eq!(recoveries.len(), 1);
        assert_eq!(recoveries[0].starting_root, root);
        assert_eq!(
            recoveries[0].restart_checkpoint,
            Some(Checkpoint { root, slot: 98 })
        );
        assert!(state.jobs.is_empty());
    }

    #[test]
    fn test_recover_stalled_queue_drops_if_superseded_by_complete_queue() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root_old = mock_root(1);
        let root_new = mock_root(2);

        state.add_new_job_queue(
            Checkpoint {
                root: root_old,
                slot: 100,
            },
            JobRequest::new(peer_id, root_old),
            false,
        );
        state.add_new_job_queue(
            Checkpoint {
                root: root_new,
                slot: 200,
            },
            JobRequest::new(peer_id, root_new),
            false,
        );
        state.mark_job_queue_as_complete(root_new);
        state.jobs[0].last_progress_at = Instant::now() - Duration::from_secs(10);

        let recoveries = state.recover_stalled_queues(Duration::from_secs(1));
        assert_eq!(recoveries.len(), 1);
        assert_eq!(recoveries[0].starting_root, root_old);
        assert_eq!(recoveries[0].restart_checkpoint, None);
        assert_eq!(state.jobs.len(), 1);
        assert_eq!(
            state.get_ready_to_process_queue().unwrap().starting_root,
            root_new
        );
    }

    #[test]
    fn test_absorb_queue_frontier_merges_older_queue_jobs_into_newer_queue() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root_old = mock_root(1);
        let root_new = mock_root(2);
        let root_old_frontier = mock_root(3);
        let root_new_frontier = mock_root(4);

        state.add_new_job_queue(
            Checkpoint {
                root: root_old,
                slot: 120,
            },
            JobRequest::new(peer_id, root_old),
            false,
        );
        state.add_new_job_queue(
            Checkpoint {
                root: root_new,
                slot: 140,
            },
            JobRequest::new(peer_id, root_new),
            false,
        );

        state.replace_job_with_next_job(root_old, 61, JobRequest::new(peer_id, root_old_frontier));
        state.replace_job_with_next_job(root_new, 101, JobRequest::new(peer_id, root_new_frontier));

        let absorption = state.absorb_queue_frontier(root_new_frontier, 100, root_old);

        assert_eq!(
            absorption,
            Some(QueueAbsorption {
                absorbed_starting_root: root_old,
                absorbed_starting_slot: 120,
                merged_job_count: 1,
            })
        );
        assert_eq!(state.jobs.len(), 1);
        assert_eq!(state.jobs[0].starting_root, root_new);
        assert!(!state.jobs[0].jobs.contains_key(&root_new_frontier));
        assert!(state.jobs[0].jobs.contains_key(&root_old_frontier));
        assert_eq!(state.jobs[0].last_fetched_slot, 61);
    }

    #[test]
    fn test_absorb_queue_frontier_ignores_complete_queue_boundary() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root_old = mock_root(1);
        let root_new = mock_root(2);
        let root_new_frontier = mock_root(4);

        state.add_new_job_queue(
            Checkpoint {
                root: root_old,
                slot: 120,
            },
            JobRequest::new(peer_id, root_old),
            false,
        );
        state.add_new_job_queue(
            Checkpoint {
                root: root_new,
                slot: 140,
            },
            JobRequest::new(peer_id, root_new),
            false,
        );

        state.mark_job_queue_as_complete(root_old);
        state.replace_job_with_next_job(root_new, 101, JobRequest::new(peer_id, root_new_frontier));

        let absorption = state.absorb_queue_frontier(root_new_frontier, 100, root_old);

        assert!(absorption.is_none());
        assert_eq!(state.jobs.len(), 2);
        assert!(
            state
                .jobs
                .iter()
                .any(|queue| queue.starting_root == root_old)
        );
        assert!(
            state
                .jobs
                .iter()
                .any(|queue| queue.starting_root == root_new)
        );
    }

    #[test]
    fn test_recover_stalled_absorbed_queue_restarts_from_absorbed_frontier() {
        let mut state = BackfillState::default();
        let peer_old = PeerId::random();
        let peer_new = PeerId::random();
        let root_old = mock_root(1);
        let root_new = mock_root(2);
        let root_old_frontier = mock_root(3);
        let root_new_frontier = mock_root(4);

        state.add_new_job_queue(
            Checkpoint {
                root: root_old,
                slot: 120,
            },
            JobRequest::new(peer_old, root_old),
            false,
        );
        state.add_new_job_queue(
            Checkpoint {
                root: root_new,
                slot: 140,
            },
            JobRequest::new(peer_new, root_new),
            false,
        );

        state.replace_job_with_next_job(root_old, 61, JobRequest::new(peer_old, root_old_frontier));
        state.replace_job_with_next_job(
            root_new,
            101,
            JobRequest::new(peer_new, root_new_frontier),
        );
        let absorption = state.absorb_queue_frontier(root_new_frontier, 100, root_old);

        assert!(absorption.is_some());
        state.jobs[0].last_progress_at = Instant::now() - Duration::from_secs(10);

        let recoveries = state.recover_stalled_queues(Duration::from_secs(1));
        assert_eq!(recoveries.len(), 1);
        assert_eq!(recoveries[0].starting_root, root_new);
        assert_eq!(
            recoveries[0].restart_checkpoint,
            Some(Checkpoint {
                root: root_old_frontier,
                slot: 60,
            })
        );
        assert_eq!(recoveries[0].peer_ids, vec![peer_old]);
        assert!(state.jobs.is_empty());
    }

    #[test]
    fn test_unqueued_jobs_and_mark_requested() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root = mock_root(1);

        state.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );

        let unqueued_jobs_list = state.unqueued_jobs();
        assert_eq!(unqueued_jobs_list.len(), 1);
        assert!(!unqueued_jobs_list[0].has_been_requested);

        state.mark_job_as_requested(root);

        let unqueued_jobs_after = state.unqueued_jobs();
        assert!(unqueued_jobs_after.is_empty());
    }

    #[test]
    fn test_contains_job_root() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root = mock_root(7);

        state.add_new_job_queue(
            Checkpoint { root, slot: 15 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert!(state.contains_job_root(root));
        assert!(!state.contains_job_root(mock_root(9)));
    }

    #[test]
    fn test_reset_timed_out_jobs() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root = mock_root(1);

        state.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );
        state.mark_job_as_requested(root);

        let timed_out_jobs = state.reset_timed_out_jobs(Duration::from_millis(0));
        assert_eq!(timed_out_jobs.len(), 1);
        assert_eq!(timed_out_jobs[0].root, root);

        let unqueued_jobs = state.unqueued_jobs();
        assert_eq!(unqueued_jobs.len(), 1);
        assert!(!unqueued_jobs[0].has_been_requested);
    }

    #[test]
    fn test_peer_for_job_root() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root = mock_root(42);
        state.add_new_job_queue(
            Checkpoint { root, slot: 10 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert_eq!(state.peer_for_job_root(root), Some(peer_id));
        assert_eq!(state.peer_for_job_root(mock_root(100)), None);
    }

    #[test]
    fn test_request_latency_for_root_after_mark_requested() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let root = mock_root(9);
        state.add_new_job_queue(
            Checkpoint { root, slot: 12 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert_eq!(state.request_latency_for_root(root), None);

        state.mark_job_as_requested(root);
        let latency = state.request_latency_for_root(root);
        assert!(latency.is_some());
    }

    #[test]
    fn test_has_job_for_peer() {
        let mut state = BackfillState::default();
        let peer_id = PeerId::random();
        let other_peer = PeerId::random();
        let root = mock_root(77);
        state.add_new_job_queue(
            Checkpoint { root, slot: 1 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert!(state.has_job_for_peer(peer_id));
        assert!(!state.has_job_for_peer(other_peer));
    }
}
