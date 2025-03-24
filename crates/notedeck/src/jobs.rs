use std::{future::Future, pin::Pin};

use tokio::sync::mpsc::{self, Receiver, Sender};

pub type WorkerId = usize;

type Job<P, T> = Box<
    dyn FnOnce(Option<P>) -> Pin<Box<dyn Future<Output = T> + Send + 'static>> + Send + 'static,
>;

#[allow(dead_code)]
pub struct Jobs<P, T> {
    job_senders: Vec<Sender<StartJob<P, T>>>,
    result_receiver: Receiver<T>,
    next: usize,
}

struct StartJob<P, T> {
    pub params: Option<P>,
    pub job: Job<P, T>,
}

#[allow(dead_code)]
impl<P: Send + 'static, T: Send + 'static> Jobs<P, T> {
    pub fn new(num_workers: usize) -> Self {
        let mut job_senders = Vec::with_capacity(num_workers);
        let (result_sender, result_receiver) = mpsc::channel::<T>(1000);

        for _ in 0..num_workers {
            let (job_tx, mut job_rx) = mpsc::channel::<StartJob<P, T>>(100);
            job_senders.push(job_tx);
            let result_sender = result_sender.clone();

            tokio::spawn(async move {
                while let Some(start_job) = job_rx.recv().await {
                    let params = start_job.params;
                    let job = start_job.job;

                    let res = job(params).await;
                    if let Err(e) = result_sender.send(res).await {
                        tracing::error!("jobs channel full: {:?}", e);
                    }
                }
            });
        }

        Self {
            job_senders,
            result_receiver,
            next: 0,
        }
    }

    pub fn submit<F, Fut>(&mut self, params: Option<P>, job: F)
    where
        F: FnOnce(Option<P>) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let perform_job: Job<P, T> = Box::new(
            move |params: Option<P>| -> Pin<Box<dyn Future<Output = T> + Send + 'static>> {
                Box::pin(job(params))
            },
        );
        let sender = &self.job_senders[self.next];
        self.next = (self.next + 1) % self.job_senders.len();
        let start_job = StartJob {
            params,
            job: perform_job,
        };
        if let Err(e) = sender.try_send(start_job) {
            tracing::error!("jobs channel full or closed: {:?}", e);
        }
    }

    pub fn take_completed(&mut self) -> Option<Vec<T>> {
        let first = self.result_receiver.try_recv().ok()?;
        let mut items = vec![first];
        while let Ok(item) = self.result_receiver.try_recv() {
            items.push(item);
        }
        Some(items)
    }
}

impl<P: Send + 'static, T: Send + 'static> Default for Jobs<P, T> {
    fn default() -> Self {
        Jobs::<P, T>::new(10)
    }
}
