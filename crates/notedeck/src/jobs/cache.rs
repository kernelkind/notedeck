use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::mpsc::{Receiver, Sender},
};

use crossbeam::queue::SegQueue;
use egui::TextureHandle;

use crate::{
    jobs::{
        types::{JobAccess, JobId, JobIdAccessible, JobIdType},
        JobPool,
    },
    Animation, Error, TextureState, TexturesCache,
};

pub struct JobsCache {
    receive_new_jobs: Receiver<JobPackage>,
    running: HashSet<JobId>,
    send_new_jobs: Sender<JobPackage>,
    completed: CompletionQueue,
}

type CompletionQueue = std::sync::Arc<SegQueue<JobComplete>>;

pub enum JobOutput {
    Complete(CompleteResponse),
    Next(JobRun),
}

impl JobOutput {
    pub fn complete(response: JobResult) -> Self {
        JobOutput::Complete(CompleteResponse::new(response))
    }

    // pub fn invalid_params() -> Self {
    //     JobOutput::Complete(CompleteResponse::new(Err(JobError::InvalidParameters)))
    // }

    // pub fn img(img: ImageJob) -> Self {
    //     JobOutput::Complete(CompleteResponse::img(img))
    // }
}

pub struct CompleteResponse {
    response: JobResult,
    run_no_output: Option<NoOutputRun>,
}

pub struct JobComplete {
    job_id: JobId,
    response: JobResult,
}

pub enum JobResult {
    StaticImg(Result<TextureHandle, Error>),
    Blurhash(Result<TextureHandle, Error>),
    Animation(Result<Animation, Error>),
}

impl CompleteResponse {
    pub fn new(response: JobResult) -> Self {
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

type JobFut = Pin<Box<dyn Future<Output = JobOutput> + Send + 'static>>;
type SyncFn = Box<dyn FnOnce() -> JobOutput + Send + 'static>;
type AsyncFn = JobFut;

pub enum JobRun {
    Sync(SyncFn),
    Async(AsyncFn),
}

pub struct JobPackage {
    id: JobIdAccessible,
    run: RunType,
}

impl JobPackage {
    pub fn new(id: String, job_type: JobIdType, run: RunType) -> Self {
        Self {
            id: JobIdAccessible::new_public(id, job_type),
            run,
        }
    }
}

pub enum RunType {
    NoOutput(NoOutputRun),
    Output(JobRun),
}

#[derive(Debug)]
pub enum JobError {
    InvalidParameters,
}

impl JobsCache {
    pub fn new(receive_new_jobs: Receiver<JobPackage>, send_new_jobs: Sender<JobPackage>) -> Self {
        Self {
            receive_new_jobs,
            send_new_jobs,
            completed: Default::default(),
            running: Default::default(),
        }
    }

    pub fn run_received(&mut self, pool: &mut JobPool, tex_cache: &mut TexturesCache) {
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

            pre_run_action(pkg.id.clone(), tex_cache);

            run_received_job(
                job_run,
                pool,
                self.send_new_jobs.clone(),
                self.completed.clone(),
                pkg.id,
            );
        }
    }

    pub fn deliver_all_completed(&mut self, tex_cache: &mut TexturesCache) {
        while let Some(res) = self.completed.pop() {
            tracing::trace!("Got completed: {:?}", res.job_id);
            let id = res.job_id.clone();
            deliver_completed_job(res, tex_cache);
            self.running.remove(&id);
        }
    }
}

fn run_received_job(
    job_run: JobRun,
    pool: &mut JobPool,
    send_new_jobs: Sender<JobPackage>,
    completion_queue: CompletionQueue,
    id: JobIdAccessible,
) {
    match job_run {
        JobRun::Sync(run) => {
            run_sync(pool, send_new_jobs, completion_queue, id, run);
        }
        JobRun::Async(run) => {
            run_async(send_new_jobs, completion_queue, id, run);
        }
    }
}

pub fn pre_run_action(id_accessable: JobIdAccessible, tex_cache: &mut TexturesCache) {
    let id = id_accessable.job_id.id;
    match id_accessable.job_id.job_type {
        JobIdType::Blurhash => {
            tex_cache
                .blurred
                .cache
                .insert(id, TextureState::Pending.into());
        }
        JobIdType::StaticImg => {
            tex_cache
                .static_image
                .cache
                .insert(id, TextureState::Pending);
        }
        JobIdType::AnimatedImg => {
            tex_cache.animated.cache.insert(id, TextureState::Pending);
        }
    }
}

fn run_sync<'a, F: Send + 'static>(
    job_pool: &mut JobPool,
    send_new_jobs: Sender<JobPackage>,
    completion_queue: CompletionQueue,
    id: JobIdAccessible,
    run_job: F,
) where
    F: FnOnce() -> JobOutput,
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

fn run_async<'a>(
    send_new_jobs: Sender<JobPackage>,
    completion_queue: CompletionQueue,
    id: JobIdAccessible,
    run_job: JobFut,
) {
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

fn deliver_completed_job(completed: JobComplete, tex_cache: &mut TexturesCache) {
    let id = completed.job_id.id;
    let id_c = id.clone();
    match completed.response {
        JobResult::StaticImg(job_complete) => {
            let r = match job_complete {
                Ok(t) => TextureState::Loaded(t),
                Err(e) => TextureState::Error(e),
            };
            tex_cache.static_image.cache.insert(id, r);
        }
        JobResult::Animation(animation) => {
            let r = match animation {
                Ok(a) => TextureState::Loaded(a),
                Err(e) => TextureState::Error(e),
            };

            tex_cache.animated.cache.insert(id, r);
        }
        JobResult::Blurhash(texture_handle) => {
            let r = match texture_handle {
                Ok(t) => TextureState::Loaded(t),
                Err(e) => TextureState::Error(e),
            };
            tex_cache.blurred.cache.insert(id, r.into());
        }
    }
    tracing::trace!("Delivered job for {id_c}");
}
