use hashbrown::{hash_map::RawEntryMut, HashMap};
use poll_promise::Promise;

use crate::jobs::{
    types::{Job, JobId, JobIdOwned, JobParams, JobParamsOwned},
    JobPool,
};

#[derive(Default)]
pub struct JobsCache {
    jobs: HashMap<JobIdOwned, JobState>,
}

pub enum JobState {
    Pending(Promise<Option<Result<Job, JobError>>>),
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

                let Some(res) = promise.ready_mut() else {
                    break 's state;
                };

                let Some(res) = res.take() else {
                    tracing::error!("Failed to take the promise for job: {:?}", jobid);
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
                let wrapped: Box<dyn FnOnce() -> Option<Result<Job, JobError>> + Send + 'static> =
                    Box::new(move || Some(run_job(owned_params)));

                let promise = Promise::spawn_async(job_pool.schedule(wrapped));

                let (_, state) = entry.insert(jobid.into(), JobState::Pending(promise));

                state
            }
        }
    }

    pub fn get(&self, jobid: &JobId) -> Option<&JobState> {
        self.jobs.get(jobid)
    }
}
