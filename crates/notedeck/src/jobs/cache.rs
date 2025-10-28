use hashbrown::{hash_map::RawEntryMut, HashMap};
use tokio::sync::oneshot::Receiver;

use crate::jobs::{
    types::{Job, JobId, JobIdOwned, JobParams, JobParamsOwned},
    JobPool,
};

#[derive(Default)]
pub struct JobsCache {
    jobs: HashMap<JobIdOwned, JobState>,
}

pub enum JobState {
    Pending(Receiver<Result<Job, JobError>>),
    Error(JobError),
    Completed(Job),
}

pub enum JobError {
    InvalidParameters,
}

impl JobsCache {
    pub fn get_or_insert_with<
        'a,
        F: FnOnce(Option<JobParamsOwned>) -> Result<Job, JobError> + Send + 'static,
    >(
        &'a mut self,
        job_pool: &mut JobPool,
        jobid: &JobId,
        params: Option<JobParams>,
        run_job: F,
    ) -> &'a mut JobState {
        match self.jobs.raw_entry_mut().from_key(jobid) {
            RawEntryMut::Occupied(entry) => 's: {
                let mut state = entry.into_mut();

                let JobState::Pending(promise) = &mut state else {
                    break 's state;
                };

                let Some(res) = promise.try_recv().ok() else {
                    break 's state;
                };

                *state = match res {
                    Ok(j) => JobState::Completed(j),
                    Err(e) => JobState::Error(e),
                };

                state
            }
            RawEntryMut::Vacant(entry) => {
                let owned_params = params.map(JobParams::into);
                let wrapped: Box<dyn FnOnce() -> Result<Job, JobError> + Send + 'static> =
                    Box::new(move || run_job(owned_params));

                let receiver = job_pool.schedule_receivable(wrapped);

                let (_, state) = entry.insert(jobid.into(), JobState::Pending(receiver));

                state
            }
        }
    }

    pub fn get(&self, jobid: &JobId) -> Option<&JobState> {
        self.jobs.get(jobid)
    }
}

pub struct Jobs<'a> {
    cache: &'a mut JobsCache,
    pool: &'a mut JobPool,
}

impl<'a> Jobs<'a> {
    pub fn new(cache: &'a mut JobsCache, pool: &'a mut JobPool) -> Self {
        Self { cache, pool }
    }

    pub fn run<F>(
        &'a mut self,
        jobid: &JobId,
        params: Option<JobParams>,
        run_job: F,
    ) -> &'a mut JobState
    where
        F: FnOnce(Option<JobParams>) -> Result<Job, JobError> + Send + 'static,
    {
        self.cache
            .get_or_insert_with(self.pool, jobid, params, run_job)
    }
}
