mod cache;
mod job_pool;
mod types;

use std::sync::mpsc::{SendError, Sender};

pub use cache::{
    CompleteResponse, JobError, JobOutput, JobPackage, JobResult, JobRun, JobsCache, NoOutputRun,
    RunType,
};
pub use job_pool::JobPool;
pub use types::{FromNetImgParamsOwned, ImageJob, ImgParamsOwned, JobIdType, JobParamsOwned};

pub struct Jobs {
    pub cache: JobsCache,
    sender: JobSender,
}

#[derive(Debug, Clone)]
pub struct JobSender {
    sender: Sender<JobPackage>,
}

/// A thin wrapper
impl JobSender {
    pub fn new(sender: Sender<JobPackage>) -> Self {
        Self { sender }
    }

    pub fn send(&self, job: JobPackage) -> Result<(), SendError<JobPackage>> {
        self.sender.send(job)
    }

    pub fn clone_channel(&self) -> Sender<JobPackage> {
        self.sender.clone()
    }

    pub fn inner(&self) -> &Sender<JobPackage> {
        &self.sender
    }
}

impl Default for Jobs {
    fn default() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let job_sender = JobSender::new(sender.clone());
        Self {
            cache: JobsCache::new(receiver, sender),
            sender: job_sender,
        }
    }
}

impl Jobs {
    pub fn sender(&self) -> &JobSender {
        &self.sender
    }
}
