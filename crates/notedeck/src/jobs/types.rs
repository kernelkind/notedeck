#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(crate) struct JobIdAccessible<K> {
    pub access: JobAccess,
    pub job_id: JobId<K>,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct JobId<K> {
    pub id: String,
    pub job_kind: K,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(crate) enum JobAccess {
    Public,   // Jobs requested outside the cache
    Internal, // Jobs requested inside the cache
}

impl<K> JobIdAccessible<K> {
    pub fn new_public(id: String, job_kind: K) -> Self {
        Self {
            job_id: JobId { id, job_kind },
            access: JobAccess::Public,
        }
    }

    pub fn into_internal(mut self) -> Self {
        self.access = JobAccess::Internal;
        self
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum MediaJobKind {
    Blurhash,
    StaticImg,
    AnimatedImg,
}
