pub mod forward_background_syncer;
pub mod job;
pub mod strategy;

use std::time::{Duration, Instant};

use alloy_primitives::B256;
use libp2p_identity::PeerId;
use ream_consensus_lean::checkpoint::Checkpoint;

use crate::sync::job::{queue::JobQueue, request::JobRequest};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SyncStatus {
    Synced,
    Syncing { jobs: Vec<JobQueue> },
}

#[derive(Debug, Clone)]
pub struct QueueRecovery {
    pub starting_root: B256,
    pub starting_slot: u64,
    pub job_roots: Vec<B256>,
    pub peer_ids: Vec<PeerId>,
    pub restart_checkpoint: Option<Checkpoint>,
}

impl SyncStatus {
    pub fn remove_processed_queue(&mut self, starting_root: B256) {
        if let SyncStatus::Syncing { jobs } = self {
            jobs.retain(|queue| queue.starting_root != starting_root);
        }
    }

    pub fn get_ready_to_process_queue(&mut self) -> Option<JobQueue> {
        if let SyncStatus::Syncing { jobs } = self
            && let Some((index, _)) = jobs
                .iter()
                .enumerate()
                .filter(|(_, queue)| queue.is_complete)
                .min_by_key(|(_, queue)| queue.starting_slot)
        {
            return jobs.get(index).cloned();
        }
        None
    }

    pub fn is_root_start_of_any_queue(&self, root: &B256) -> bool {
        if let SyncStatus::Syncing { jobs } = self {
            // Need at least two queues to consider this check meaningful
            if jobs.len() < 2 {
                return false;
            }

            for queue in jobs {
                if &queue.starting_root == root {
                    return true;
                }
            }
        }
        false
    }

    pub fn mark_job_queue_as_complete(&mut self, last_root: B256) {
        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs.iter_mut() {
                if queue.jobs.contains_key(&last_root) {
                    queue.is_complete = true;
                    queue.touch_progress();
                    queue.jobs.clear();
                    return;
                }
            }
        }
    }

    pub fn add_new_job_queue(
        &mut self,
        checkpoint: Checkpoint,
        job: JobRequest,
        bypass_slot_check: bool,
    ) -> bool {
        match self {
            SyncStatus::Syncing { jobs } => {
                if !bypass_slot_check
                    && jobs
                        .iter()
                        .any(|queue| checkpoint.slot <= queue.starting_slot)
                {
                    return false;
                }

                if jobs
                    .iter()
                    .any(|queue| queue.starting_root == checkpoint.root)
                {
                    return false;
                }

                let mut new_queue =
                    JobQueue::new(checkpoint.root, checkpoint.slot, checkpoint.slot);
                new_queue.add_job(job);
                jobs.push(new_queue);
                true
            }
            SyncStatus::Synced => false,
        }
    }

    pub fn slot_is_subset_of_any_queue(&self, slot: u64) -> bool {
        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs {
                if slot <= queue.starting_slot {
                    return true;
                }
            }
        }
        false
    }

    pub fn reset_or_initialize_next_job(
        &mut self,
        last_root: B256,
        last_slot: u64,
        new_job: JobRequest,
    ) -> Option<JobRequest> {
        match self {
            SyncStatus::Syncing { jobs } => {
                for queue in jobs.iter_mut() {
                    if let Some(old_job) = queue.jobs.remove(&last_root) {
                        queue.last_fetched_slot = last_slot;
                        queue.touch_progress();
                        queue.jobs.insert(new_job.root, new_job);

                        return Some(old_job);
                    }
                }
                None
            }
            SyncStatus::Synced => None,
        }
    }

    pub fn reset_job_with_new_peer_id(
        &mut self,
        old_peer_id: PeerId,
        new_peer_id: PeerId,
    ) -> Option<JobRequest> {
        match self {
            SyncStatus::Syncing { jobs } => {
                for queue in jobs.iter_mut() {
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
            SyncStatus::Synced => None,
        }
    }

    pub fn replace_job_with_next_job(
        &mut self,
        last_root: B256,
        last_slot: u64,
        new_job: JobRequest,
    ) -> Option<JobRequest> {
        match self {
            SyncStatus::Syncing { jobs } => {
                for queue in jobs.iter_mut() {
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
            }
            SyncStatus::Synced => return None,
        }

        None
    }

    pub fn unqueued_jobs(&self) -> Vec<JobRequest> {
        let mut unqueued_jobs = Vec::new();

        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs {
                for job in queue.jobs.values() {
                    if !job.has_been_requested {
                        unqueued_jobs.push(job.clone());
                    }
                }
            }
        }

        unqueued_jobs
    }

    pub fn contains_job_root(&self, root: B256) -> bool {
        if let SyncStatus::Syncing { jobs } = self {
            return jobs.iter().any(|queue| queue.jobs.contains_key(&root));
        }

        false
    }

    pub fn peer_for_job_root(&self, root: B256) -> Option<PeerId> {
        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs {
                if let Some(job) = queue.jobs.get(&root) {
                    return Some(job.peer_id);
                }
            }
        }

        None
    }

    pub fn reset_timed_out_jobs(&mut self, timeout: Duration) -> Vec<JobRequest> {
        let mut timed_out_jobs = Vec::new();
        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs {
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
        }
        timed_out_jobs
    }

    pub fn request_latency_for_root(&self, root: B256) -> Option<Duration> {
        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs {
                if let Some(job) = queue.jobs.get(&root)
                    && let Some(requested_at) = job.time_requested
                {
                    let now = Instant::now();
                    return Some(now.saturating_duration_since(requested_at));
                }
            }
        }

        None
    }

    pub fn has_job_for_peer(&self, peer_id: PeerId) -> bool {
        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs {
                if queue.jobs.values().any(|job| job.peer_id == peer_id) {
                    return true;
                }
            }
        }

        false
    }

    pub fn mark_job_as_requested(&mut self, root: B256) {
        if let SyncStatus::Syncing { jobs } = self {
            for queue in jobs.iter_mut() {
                if let Some(job) = queue.jobs.get_mut(&root) {
                    job.mark_requested();
                    return;
                }
            }
        }
    }

    pub fn recover_stalled_queues(&mut self, timeout: Duration) -> Vec<QueueRecovery> {
        let mut recoveries = Vec::new();

        if let SyncStatus::Syncing { jobs } = self {
            let now = Instant::now();
            let complete_slots: Vec<u64> = jobs
                .iter()
                .filter(|queue| queue.is_complete)
                .map(|queue| queue.starting_slot)
                .collect();
            let mut stalled_roots = Vec::new();

            for queue in jobs.iter() {
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

            jobs.retain(|queue| !stalled_roots.contains(&queue.starting_root));
        }

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
    fn test_sync_status_add_new_job_queue() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();

        let root_middle = mock_root(1);
        let checkpoint_middle = Checkpoint {
            root: root_middle,
            slot: 100,
        };
        let job_middle = JobRequest::new(peer_id, root_middle);
        assert!(status.add_new_job_queue(checkpoint_middle, job_middle, false));

        let root_low = mock_root(2);
        let checkpoint_low = Checkpoint {
            root: root_low,
            slot: 50,
        };
        let job_low = JobRequest::new(peer_id, root_low);
        assert!(
            !status.add_new_job_queue(checkpoint_low, job_low.clone(), false),
            "Should NOT allow adding lower slot without bypass flag"
        );

        assert!(
            status.add_new_job_queue(checkpoint_low, job_low, true),
            "Should allow adding lower slot with bypass flag"
        );

        let root_high = mock_root(3);
        let checkpoint_high = Checkpoint {
            root: root_high,
            slot: 200,
        };
        let job_high = JobRequest::new(peer_id, root_high);
        assert!(status.add_new_job_queue(checkpoint_high, job_high, false));

        if let SyncStatus::Syncing { jobs } = status {
            assert_eq!(jobs.len(), 3);
        } else {
            panic!("Status should be Syncing");
        }
    }

    #[test]
    fn test_replace_job_with_next_job() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root_original = mock_root(1);

        let checkpoint = Checkpoint {
            root: root_original,
            slot: 100,
        };
        let job_original = JobRequest::new(peer_id, root_original);
        status.add_new_job_queue(checkpoint, job_original, false);

        let parent_root = mock_root(2);
        let new_job_request = JobRequest::new(peer_id, parent_root);
        let replaced_job = status.replace_job_with_next_job(root_original, 99, new_job_request);

        assert!(replaced_job.is_some());
        assert_eq!(replaced_job.unwrap().root, root_original);

        if let SyncStatus::Syncing { jobs } = &status {
            let queue = &jobs[0];
            assert!(!queue.jobs.contains_key(&root_original));
            assert!(queue.jobs.contains_key(&parent_root));
            assert_eq!(queue.last_fetched_slot, 99);
        }
    }

    #[test]
    fn test_mark_job_queue_as_complete() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root = mock_root(1);

        status.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );

        status.mark_job_queue_as_complete(root);

        if let SyncStatus::Syncing { jobs } = status {
            assert!(jobs[0].is_complete);
            assert!(jobs[0].jobs.is_empty());
        }
    }

    #[test]
    fn test_get_ready_to_process_queue_ordering() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();

        let root_200 = mock_root(2);
        status.add_new_job_queue(
            Checkpoint {
                root: root_200,
                slot: 200,
            },
            JobRequest::new(peer_id, root_200),
            false,
        );

        let root_100 = mock_root(1);
        status.add_new_job_queue(
            Checkpoint {
                root: root_100,
                slot: 100,
            },
            JobRequest::new(peer_id, root_100),
            true,
        );

        status.mark_job_queue_as_complete(root_200);

        status.mark_job_queue_as_complete(root_100);

        let queue_1 = status.get_ready_to_process_queue().unwrap();
        assert_eq!(queue_1.starting_slot, 100);
        status.remove_processed_queue(root_100);

        let queue_2 = status.get_ready_to_process_queue().unwrap();
        assert_eq!(queue_2.starting_slot, 200);
        status.remove_processed_queue(root_200);

        assert!(status.get_ready_to_process_queue().is_none());
    }

    #[test]
    fn test_get_ready_to_process_queue_skips_older_incomplete_queue() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();

        let root_older = mock_root(1);
        status.add_new_job_queue(
            Checkpoint {
                root: root_older,
                slot: 100,
            },
            JobRequest::new(peer_id, root_older),
            false,
        );

        let root_newer = mock_root(2);
        status.add_new_job_queue(
            Checkpoint {
                root: root_newer,
                slot: 200,
            },
            JobRequest::new(peer_id, root_newer),
            false,
        );

        status.mark_job_queue_as_complete(root_newer);

        let queue = status.get_ready_to_process_queue().unwrap();
        assert_eq!(queue.starting_slot, 200);
        assert_eq!(queue.starting_root, root_newer);
    }

    #[test]
    fn test_recover_stalled_queue_restarts_from_current_missing_root() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root = mock_root(1);

        status.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );

        if let SyncStatus::Syncing { jobs } = &mut status {
            jobs[0].last_fetched_slot = 99;
            jobs[0].last_progress_at = Instant::now() - Duration::from_secs(10);
        }

        let recoveries = status.recover_stalled_queues(Duration::from_secs(1));
        assert_eq!(recoveries.len(), 1);
        assert_eq!(recoveries[0].starting_root, root);
        assert_eq!(
            recoveries[0].restart_checkpoint,
            Some(Checkpoint { root, slot: 98 })
        );
        assert!(matches!(status, SyncStatus::Syncing { jobs } if jobs.is_empty()));
    }

    #[test]
    fn test_recover_stalled_queue_drops_if_superseded_by_complete_queue() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root_old = mock_root(1);
        let root_new = mock_root(2);

        status.add_new_job_queue(
            Checkpoint {
                root: root_old,
                slot: 100,
            },
            JobRequest::new(peer_id, root_old),
            false,
        );
        status.add_new_job_queue(
            Checkpoint {
                root: root_new,
                slot: 200,
            },
            JobRequest::new(peer_id, root_new),
            false,
        );
        status.mark_job_queue_as_complete(root_new);

        if let SyncStatus::Syncing { jobs } = &mut status {
            jobs[0].last_progress_at = Instant::now() - Duration::from_secs(10);
        }

        let recoveries = status.recover_stalled_queues(Duration::from_secs(1));
        assert_eq!(recoveries.len(), 1);
        assert_eq!(recoveries[0].starting_root, root_old);
        assert_eq!(recoveries[0].restart_checkpoint, None);
        assert!(matches!(&status, SyncStatus::Syncing { jobs } if jobs.len() == 1));
        assert_eq!(
            status.get_ready_to_process_queue().unwrap().starting_root,
            root_new
        );
    }

    #[test]
    fn test_unqueued_jobs_and_mark_requested() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root = mock_root(1);

        status.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );

        let unqueued_jobs_list = status.unqueued_jobs();
        assert_eq!(unqueued_jobs_list.len(), 1);
        assert!(!unqueued_jobs_list[0].has_been_requested);

        status.mark_job_as_requested(root);

        let unqueued_jobs_after = status.unqueued_jobs();
        assert!(unqueued_jobs_after.is_empty());
    }

    #[test]
    fn test_contains_job_root() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root = mock_root(7);

        status.add_new_job_queue(
            Checkpoint { root, slot: 15 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert!(status.contains_job_root(root));
        assert!(!status.contains_job_root(mock_root(9)));
    }

    #[test]
    fn test_reset_timed_out_jobs() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root = mock_root(1);

        status.add_new_job_queue(
            Checkpoint { root, slot: 100 },
            JobRequest::new(peer_id, root),
            false,
        );
        status.mark_job_as_requested(root);

        let timed_out_jobs = status.reset_timed_out_jobs(Duration::from_millis(0));
        assert_eq!(timed_out_jobs.len(), 1);
        assert_eq!(timed_out_jobs[0].root, root);

        let unqueued_jobs = status.unqueued_jobs();
        assert_eq!(unqueued_jobs.len(), 1);
        assert!(!unqueued_jobs[0].has_been_requested);
    }

    #[test]
    fn test_peer_for_job_root() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root = mock_root(42);
        status.add_new_job_queue(
            Checkpoint { root, slot: 10 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert_eq!(status.peer_for_job_root(root), Some(peer_id));
        assert_eq!(status.peer_for_job_root(mock_root(100)), None);
    }

    #[test]
    fn test_request_latency_for_root_after_mark_requested() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let root = mock_root(9);
        status.add_new_job_queue(
            Checkpoint { root, slot: 12 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert_eq!(status.request_latency_for_root(root), None);

        status.mark_job_as_requested(root);
        let latency = status.request_latency_for_root(root);
        assert!(latency.is_some());
    }

    #[test]
    fn test_has_job_for_peer() {
        let mut status = SyncStatus::Syncing { jobs: Vec::new() };
        let peer_id = PeerId::random();
        let other_peer = PeerId::random();
        let root = mock_root(77);
        status.add_new_job_queue(
            Checkpoint { root, slot: 1 },
            JobRequest::new(peer_id, root),
            false,
        );

        assert!(status.has_job_for_peer(peer_id));
        assert!(!status.has_job_for_peer(other_peer));
    }
}
