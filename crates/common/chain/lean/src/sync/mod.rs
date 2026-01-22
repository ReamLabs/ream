pub mod forward_background_syncer;
pub mod job;

use alloy_primitives::B256;
use libp2p_identity::PeerId;
use ream_consensus_lean::checkpoint::Checkpoint;

use crate::sync::job::{queue::JobQueue, request::JobRequest};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SyncStatus {
    Synced,
    Syncing { jobs: Vec<JobQueue> },
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
                .min_by_key(|(_, queue)| queue.starting_slot)
            && jobs[index].is_complete
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
}
