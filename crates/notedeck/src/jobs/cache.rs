use std::{
    collections::HashSet,
    fmt::Debug,
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::mpsc::{Receiver, Sender},
};

use crossbeam::queue::SegQueue;

use crate::jobs::{
    types::{JobAccess, JobId, JobIdAccessible},
    JobPool,
};

pub struct JobsCache<K, T: 'static> {
    receive_new_jobs: Receiver<JobPackage<K, T>>,
    running: HashSet<JobId<K>>,
    send_new_jobs: Sender<JobPackage<K, T>>,
    completed: CompletionQueue<K, T>,
}

type CompletionQueue<K, T> = std::sync::Arc<SegQueue<JobComplete<K, T>>>;

pub enum JobOutput<T> {
    Complete(CompleteResponse<T>),
    Next(JobRun<T>),
}

impl<T> JobOutput<T> {
    pub fn complete(response: T) -> Self {
        JobOutput::Complete(CompleteResponse::new(response))
    }

    // pub fn invalid_params() -> Self {
    //     JobOutput::Complete(CompleteResponse::new(Err(JobError::InvalidParameters)))
    // }

    // pub fn img(img: ImageJob) -> Self {
    //     JobOutput::Complete(CompleteResponse::img(img))
    // }
}

pub struct CompleteResponse<T> {
    response: T,
    run_no_output: Option<NoOutputRun>,
}

pub struct JobComplete<K, T> {
    pub job_id: JobId<K>,
    pub response: T,
}

impl<T> CompleteResponse<T> {
    pub fn new(response: T) -> Self {
        Self {
            response,
            run_no_output: None,
        }
    }

    pub fn run_no_output(mut self, run: NoOutputRun) -> Self {
        self.run_no_output = Some(run);
        self
    }
}

pub enum NoOutputRun {
    Sync(Box<dyn FnOnce() + Send + 'static>),
    Async(Pin<Box<dyn Future<Output = ()> + Send + 'static>>),
}

type SyncFn<T> = Box<dyn FnOnce() -> JobOutput<T> + Send + 'static>;
type AsyncFn<T> = Pin<Box<dyn Future<Output = JobOutput<T>> + Send + 'static>>;

pub enum JobRun<T> {
    Sync(SyncFn<T>),
    Async(AsyncFn<T>),
}

pub struct JobPackage<K, T> {
    id: JobIdAccessible<K>,
    run: RunType<T>,
}

impl<K, T> JobPackage<K, T> {
    pub fn new(id: String, job_kind: K, run: RunType<T>) -> Self {
        Self {
            id: JobIdAccessible::new_public(id, job_kind),
            run,
        }
    }
}

pub enum RunType<T> {
    NoOutput(NoOutputRun),
    Output(JobRun<T>),
}

#[derive(Debug)]
pub enum JobError {
    InvalidParameters,
}

impl<K, T> JobsCache<K, T>
where
    K: Hash + Eq + Clone + Debug + Send + 'static,
    T: Send + 'static,
{
    pub fn new(
        receive_new_jobs: Receiver<JobPackage<K, T>>,
        send_new_jobs: Sender<JobPackage<K, T>>,
    ) -> Self {
        Self {
            receive_new_jobs,
            send_new_jobs,
            completed: Default::default(),
            running: Default::default(),
        }
    }

    pub fn run_received(&mut self, pool: &mut JobPool, mut pre_action: impl FnMut(&JobId<K>)) {
        for pkg in self.receive_new_jobs.try_iter() {
            let id = &pkg.id;
            if id.access == JobAccess::Public && self.running.contains(&id.job_id) {
                tracing::warn!("Ignoring request to run {id:?} since it's already running");
                continue;
                // } else {
                //     tracing::info!("Not ignoring request to run because it is public and not already in the running cache OR is internal: {:?}, in cache: {}", id.access,  self.running.contains(&id.job_id));
            }
            self.running.insert(id.job_id.clone());

            let job_run = match pkg.run {
                RunType::NoOutput(run) => {
                    no_output_run(pool, run);
                    continue;
                }
                RunType::Output(job_run) => job_run,
            };

            pre_action(&id.job_id);

            run_received_job(
                job_run,
                pool,
                self.send_new_jobs.clone(),
                self.completed.clone(),
                pkg.id,
            );
        }
    }

    pub fn deliver_all_completed(&mut self, mut deliver_complete: impl FnMut(JobComplete<K, T>)) {
        while let Some(res) = self.completed.pop() {
            tracing::trace!("Got completed: {:?}", res.job_id);
            let id = res.job_id.clone();
            deliver_complete(res);
            self.running.remove(&id);
        }
    }

    pub fn sender(&self) -> &Sender<JobPackage<K, T>> {
        &self.send_new_jobs
    }
}

fn run_received_job<K, T>(
    job_run: JobRun<T>,
    pool: &mut JobPool,
    send_new_jobs: Sender<JobPackage<K, T>>,
    completion_queue: CompletionQueue<K, T>,
    id: JobIdAccessible<K>,
) where
    K: Hash + Eq + Clone + Debug + Send + 'static,
    T: Send + 'static,
{
    match job_run {
        JobRun::Sync(run) => {
            run_sync(pool, send_new_jobs, completion_queue, id, run);
        }
        JobRun::Async(run) => {
            run_async(send_new_jobs, completion_queue, id, run);
        }
    }
}

fn run_sync<'a, F, K, T>(
    job_pool: &mut JobPool,
    send_new_jobs: Sender<JobPackage<K, T>>,
    completion_queue: CompletionQueue<K, T>,
    id: JobIdAccessible<K>,
    run_job: F,
) where
    F: FnOnce() -> JobOutput<T> + Send + 'static,
    K: Hash + Eq + Clone + Debug + Send + 'static,
    T: Send + 'static,
{
    let id_c = id.clone();
    let wrapped: Box<dyn FnOnce() + Send + 'static> = Box::new(move || {
        let res = run_job();
        match res {
            JobOutput::Complete(complete_response) => {
                completion_queue.push(JobComplete {
                    job_id: id.job_id.clone(),
                    response: complete_response.response,
                });
                if let Some(run) = complete_response.run_no_output {
                    if let Err(e) = send_new_jobs.send(JobPackage {
                        id: id.into_internal(),
                        run: RunType::NoOutput(run),
                    }) {
                        tracing::error!("{e}");
                    }
                }
            }
            JobOutput::Next(job_run) => {
                if let Err(e) = send_new_jobs.send(JobPackage {
                    id: id.into_internal(),
                    run: RunType::Output(job_run),
                }) {
                    tracing::error!("{e}");
                }
            }
        }
    });

    tracing::trace!("Spawning sync job: {id_c:?}");
    job_pool.schedule_no_output(wrapped);
}

fn run_async<'a, K, T>(
    send_new_jobs: Sender<JobPackage<K, T>>,
    completion_queue: CompletionQueue<K, T>,
    id: JobIdAccessible<K>,
    run_job: AsyncFn<T>,
) where
    K: Hash + Eq + Clone + Debug + Send + 'static,
    T: Send + 'static,
{
    tracing::trace!("Spawning async job: {id:?}");
    tokio::spawn(async move {
        {
            let res = run_job.await;
            match res {
                JobOutput::Complete(complete_response) => {
                    completion_queue.push(JobComplete {
                        job_id: id.job_id.clone(),
                        response: complete_response.response,
                    });
                    if let Some(run) = complete_response.run_no_output {
                        if let Err(e) = send_new_jobs.send(JobPackage {
                            id: id.into_internal(),
                            run: RunType::NoOutput(run),
                        }) {
                            tracing::error!("{e}");
                        }
                    }
                }
                JobOutput::Next(job_run) => {
                    if let Err(e) = send_new_jobs.send(JobPackage {
                        id: id.into_internal(),
                        run: RunType::Output(job_run),
                    }) {
                        tracing::error!("{e}");
                    }
                }
            }
        }
    });
}

fn no_output_run(pool: &mut JobPool, run: NoOutputRun) {
    match run {
        NoOutputRun::Sync(c) => {
            tracing::trace!("Spawning no output sync job");
            pool.schedule_no_output(c);
        }
        NoOutputRun::Async(f) => {
            tracing::trace!("Spawning no output async sync job");
            tokio::spawn(f);
        }
    }
}
