use std::path::PathBuf;

use crate::{media::http::HyperHttpResponse, TexturedImage};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(crate) struct JobIdAccessible {
    pub access: JobAccess,
    pub job_id: JobId,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(crate) struct JobId {
    pub id: String,
    pub job_type: JobIdType,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(crate) enum JobAccess {
    Public,
    Internal,
}

impl JobIdAccessible {
    pub fn new_public(id: String, job_type: JobIdType) -> Self {
        Self {
            job_id: JobId { id, job_type },
            access: JobAccess::Public,
        }
    }

    pub fn into_internal(mut self) -> Self {
        self.access = JobAccess::Internal;
        self
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum JobIdType {
    Blurhash,
    StaticImg,
    AnimatedImg,
}

#[derive(Debug)]
pub enum JobParamsOwned {
    Blurhash(BlurhashParamsOwned),
    DiskImg(ImgParamsOwned),
    NetImg(FromNetImgParamsOwned),
}

#[derive(Debug)]
pub struct BlurhashParamsOwned {
    pub blurhash: String,
    pub url: String,
    pub ctx: egui::Context,
}
#[derive(Debug)]
pub struct ImgParamsOwned {
    pub url: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct FromNetImgParamsOwned {
    pub http_resp: HyperHttpResponse,
    pub img_params: ImgParamsOwned,
}

pub struct ImageJob {
    pub job: Result<TexturedImage, crate::Error>,
}
