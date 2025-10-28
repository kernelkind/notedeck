use egui::TextureHandle;

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum JobIdOwned {
    Blurhash(String), // image URL
}

impl<'a> From<&JobId<'a>> for JobIdOwned {
    fn from(jobid: &JobId<'a>) -> Self {
        match jobid {
            JobId::Blurhash(s) => JobIdOwned::Blurhash(s.to_string()),
        }
    }
}

#[derive(Debug, Hash)]
pub enum JobId<'a> {
    Blurhash(&'a str), // image URL
}

impl hashbrown::Equivalent<JobIdOwned> for JobId<'_> {
    fn equivalent(&self, key: &JobIdOwned) -> bool {
        match (self, key) {
            (JobId::Blurhash(a), JobIdOwned::Blurhash(b)) => *a == b.as_str(),
        }
    }
}

#[derive(Debug)]
pub enum JobParamsOwned {
    Blurhash(BlurhashParamsOwned),
}

impl<'a> From<JobParams<'a>> for JobParamsOwned {
    fn from(params: JobParams<'a>) -> Self {
        match params {
            JobParams::Blurhash(bp) => JobParamsOwned::Blurhash(bp.into()),
        }
    }
}

#[derive(Debug)]
pub enum JobParams<'a> {
    Blurhash(BlurhashParams<'a>),
}

#[derive(Debug)]
pub struct BlurhashParamsOwned {
    pub blurhash: String,
    pub url: String,
    pub ctx: egui::Context,
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

#[derive(Debug)]
pub struct BlurhashParams<'a> {
    pub blurhash: &'a str,
    pub url: &'a str,
    pub ctx: &'a egui::Context,
}

pub enum Job {
    Blurhash(Option<TextureHandle>),
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Job::Blurhash(_) => write!(f, "Blurhash"),
        }
    }
}
