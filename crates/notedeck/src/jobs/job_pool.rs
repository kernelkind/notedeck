use crossbeam::queue::SegQueue;
use std::{future::Future, sync::Arc};
use tokio::sync::oneshot::{self, Receiver};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct JobPool {
    tx: Arc<SegQueue<Job>>,
}

impl Default for JobPool {
    fn default() -> Self {
        JobPool::new(2)
    }
}

impl JobPool {
    pub fn new(num_threads: usize) -> Self {
        let queue = SegQueue::<Job>::new();
        let arc_queue = Arc::new(queue);
        for _ in 0..num_threads {
            let queue_ref = arc_queue.clone();
            std::thread::spawn(move || loop {
                let Some(job) = queue_ref.pop() else {
                    continue;
                };

                job();
            });
        }

        Self {
            tx: arc_queue.clone(),
        }
    }

    pub fn schedule<F, T>(&self, job: F) -> impl Future<Output = T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let rx_result = self.schedule_receivable(job);
        async move {
            rx_result.await.unwrap_or_else(|_| {
                panic!("Worker thread or channel dropped before returning the result.")
            })
        }
    }

    pub fn schedule_receivable<F, T>(&self, job: F) -> Receiver<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx_result, rx_result) = oneshot::channel::<T>();

        let job = Box::new(move || {
            let output = job();
            let _ = tx_result.send(output);
        });

        self.tx.push(job);

        rx_result
    }
}

#[cfg(test)]
mod tests {
    use crate::jobs::JobPool;

    fn test_fn(a: u32, b: u32) -> u32 {
        a + b
    }

    #[tokio::test]
    async fn test() {
        let pool = JobPool::default();

        // Now each job can return different T
        let future_str = pool.schedule(|| -> String { "hello from string job".into() });

        let a = 5;
        let b = 6;
        let future_int = pool.schedule(move || -> u32 { test_fn(a, b) });

        println!("(Meanwhile we can do more async work) ...");

        let s = future_str.await;
        let i = future_int.await;

        println!("Got string: {:?}", s);
        println!("Got integer: {}", i);
    }
}
