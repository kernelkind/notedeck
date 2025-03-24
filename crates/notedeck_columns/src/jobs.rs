use std::future::Future;

use egui::TextureHandle;
use hashbrown::{hash_map::RawEntryMut, HashMap};
use notedeck::jobs::Jobs;

#[allow(dead_code)]
pub enum ColumnsJob {
    Blurhash(Option<TextureHandle>),
}

#[allow(dead_code)]
pub enum JobState {
    Pending,
    Error(JobError),
    Completed(ColumnsJob),
}

#[allow(dead_code)]
pub enum JobError {
    InvalidParameters,
}

#[allow(dead_code)]
pub struct CompletedJob {
    pub id: JobIdOwned,
    pub job: Result<ColumnsJob, JobError>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum JobParams<'a> {
    Blurhash(BlurhashParams<'a>),
}

#[allow(dead_code)]
pub enum JobParamsOwned {
    Blurhash(BlurhashParamsOwned),
}

impl<'a> From<BlurhashParams<'a>> for BlurhashParamsOwned {
    fn from(params: BlurhashParams<'a>) -> Self {
        BlurhashParamsOwned {
            blurhash: params.blurhash.to_owned(),
            url: params.url.to_owned(),
            ctx: params.ctx.clone(),
        }
    }
}

impl<'a> From<JobParams<'a>> for JobParamsOwned {
    fn from(params: JobParams<'a>) -> Self {
        match params {
            JobParams::Blurhash(bp) => JobParamsOwned::Blurhash(bp.into()),
        }
    }
}

#[derive(Debug)]
pub struct BlurhashParams<'a> {
    pub blurhash: &'a str,
    pub url: &'a str,
    pub ctx: &'a egui::Context,
}

#[allow(dead_code)]
pub struct BlurhashParamsOwned {
    pub blurhash: String,
    pub url: String,
    pub ctx: egui::Context,
}

// The hash of each JobId case must match the corresponding JobIdOwned case.
// Otherwise, odd things start happening in the HashMap.
impl std::hash::Hash for JobId<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            JobId::Blurhash(s) => s.hash(state),
        }
    }
}

impl std::hash::Hash for JobIdOwned {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            JobIdOwned::Blurhash(s) => s.hash(state),
        }
    }
}

impl<'a> From<&JobId<'a>> for JobIdOwned {
    fn from(jobid: &JobId<'a>) -> Self {
        match jobid {
            JobId::Blurhash(s) => JobIdOwned::Blurhash(s.to_string()),
        }
    }
}

impl hashbrown::Equivalent<JobIdOwned> for JobId<'_> {
    fn equivalent(&self, key: &JobIdOwned) -> bool {
        match (self, key) {
            (JobId::Blurhash(a), JobIdOwned::Blurhash(b)) => *a == b.as_str(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum JobIdOwned {
    Blurhash(String), // image URL
}

#[allow(dead_code)]
pub enum JobId<'a> {
    Blurhash(&'a str), // image URL
}

impl std::fmt::Debug for ColumnsJob {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ColumnsJob::Blurhash(_) => write!(f, "ProcessBlurhash"),
        }
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct JobsCache {
    cache: HashMap<JobIdOwned, JobState>,
    jobs: Jobs<JobParamsOwned, CompletedJob>,
}

#[allow(dead_code)]
impl JobsCache {
    pub fn get_or_insert_with<
        'a,
        T: FnOnce(Option<JobParamsOwned>) -> Fut + Send + 'static,
        Fut: Future<Output = CompletedJob> + Send + 'static,
    >(
        &'a mut self,
        jobid: &JobId,
        params: Option<JobParams>,
        run_job: T,
    ) -> &'a mut JobState {
        self.move_completed();
        match self.cache.raw_entry_mut().from_key(jobid) {
            RawEntryMut::Occupied(entry) => entry.into_mut(),
            RawEntryMut::Vacant(entry) => {
                let (_, state) = entry.insert(jobid.into(), JobState::Pending);

                self.jobs.submit(params.map(JobParams::into), run_job);

                state
            }
        }
    }

    fn move_completed(&mut self) {
        let Some(completed_res) = self.jobs.take_completed() else {
            return;
        };

        for completed in completed_res {
            let state = match completed.job {
                Ok(j) => JobState::Completed(j),
                Err(e) => JobState::Error(e),
            };

            self.cache.insert(completed.id, state);
        }
    }
}
