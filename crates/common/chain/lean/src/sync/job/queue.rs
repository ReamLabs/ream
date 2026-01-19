use std::collections::HashMap;

use alloy_primitives::B256;

use crate::sync::job::request::JobRequest;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct JobQueue {
    pub starting_root: B256,
    pub starting_slot: u64,
    pub last_fetched_slot: u64,
    pub jobs: HashMap<B256, JobRequest>,
    pub is_complete: bool,
}

impl JobQueue {
    pub fn new(starting_root: B256, starting_slot: u64, last_fetched_slot: u64) -> Self {
        JobQueue {
            starting_root,
            starting_slot,
            last_fetched_slot,
            jobs: HashMap::new(),
            is_complete: false,
        }
    }

    pub fn add_job(&mut self, job: JobRequest) {
        self.jobs.insert(job.root, job);
    }

    pub fn get(&mut self, root: &B256) -> Option<&mut JobRequest> {
        self.jobs.get_mut(root)
    }
}

#[cfg(test)]
mod tests {
    use libp2p_identity::PeerId;

    use super::*;

    #[test]
    fn test_job_queue_creation_and_ops() {
        let root = B256::repeat_byte(1);
        let mut queue = JobQueue::new(root, 100, 100);

        assert_eq!(queue.starting_root, root);
        assert_eq!(queue.starting_slot, 100);
        assert_eq!(queue.last_fetched_slot, 100);
        assert!(!queue.is_complete);

        let job_root = B256::repeat_byte(2);
        let job = JobRequest::new(PeerId::random(), job_root);

        queue.add_job(job.clone());
        assert!(queue.jobs.contains_key(&job_root));

        let retrieved = queue.get(&job_root).unwrap();
        assert_eq!(retrieved.root, job_root);
    }
}
