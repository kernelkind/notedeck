use std::{collections::HashMap, path::PathBuf};

use egui::TextureHandle;

use crate::{
    media::{
        http::http_req,
        images::{fetch_static_img_from_disk, parse_img_response},
        load_texture_checked,
    },
    CompleteResponse, ImageType, JobIdType, JobOutput, JobPackage, JobResult, JobRun, JobSender,
    MediaCache, RunType, TextureState,
};

pub struct StaticImgTexCache {
    pub(crate) cache: HashMap<String, TextureState<TextureHandle>>,
    static_img_cache_path: PathBuf,
}

impl StaticImgTexCache {
    pub fn new(static_img_cache_path: PathBuf) -> Self {
        Self {
            cache: Default::default(),
            static_img_cache_path,
        }
    }

    pub fn contains(&self, url: &str) -> bool {
        self.cache.contains_key(url)
    }

    pub fn get(&self, url: &str) -> Option<&TextureState<TextureHandle>> {
        self.cache.get(url)
    }

    pub fn request(&self, jobs: &JobSender, ctx: &egui::Context, url: &str, imgtype: ImageType) {
        let _ = self.get_or_request(jobs, ctx, url, imgtype);
    }

    pub fn get_or_request(
        &self,
        jobs: &JobSender,
        ctx: &egui::Context,
        url: &str,
        imgtype: ImageType,
    ) -> &TextureState<TextureHandle> {
        if let Some(res) = self.cache.get(url) {
            return res;
        }

        let key = MediaCache::key(url);
        let path = self.static_img_cache_path.join(key);

        if path.exists() {
            let ctx = ctx.clone();
            let url = url.to_owned();
            if let Err(e) = jobs.send(JobPackage::new(
                url.to_owned(),
                JobIdType::StaticImg,
                RunType::Output(JobRun::Sync(Box::new(move || {
                    JobOutput::Complete(CompleteResponse::new(JobResult::StaticImg(
                        fetch_static_img_from_disk(ctx.clone(), &url, &path),
                    )))
                }))),
            )) {
                tracing::error!("{e}");
            }
        } else {
            let url = url.to_owned();
            let ctx = ctx.clone();
            if let Err(e) = jobs.send(JobPackage::new(
                url.to_owned(),
                JobIdType::StaticImg,
                RunType::Output(JobRun::Async(Box::pin(fetch_static_img_from_net(
                    url,
                    ctx,
                    self.static_img_cache_path.clone(),
                    imgtype,
                )))),
            )) {
                tracing::error!("{e}");
            }
        }

        &TextureState::Pending
    }
}

async fn fetch_static_img_from_net(
    url: String,
    ctx: egui::Context,
    path: PathBuf,
    imgtype: ImageType,
) -> JobOutput {
    tracing::trace!("fetch static img from net: starting job. sending http request for {url}");
    let res = match http_req(&url).await {
        Ok(r) => r,
        Err(e) => {
            return JobOutput::complete(JobResult::StaticImg(Err(crate::Error::Generic(format!(
                "Http error: {e}"
            )))));
        }
    };

    tracing::trace!("static img from net: parsing http request from {url}");
    JobOutput::Next(JobRun::Sync(Box::new(move || {
        let img = match parse_img_response(res, imgtype) {
            Ok(i) => i,
            Err(e) => {
                return JobOutput::Complete(CompleteResponse::new(JobResult::StaticImg(Err(e))))
            }
        };

        let texture_handle =
            load_texture_checked(&ctx, url.clone(), img.clone(), Default::default());

        JobOutput::Complete(
            CompleteResponse::new(JobResult::StaticImg(Ok(texture_handle))).run_no_output(
                crate::NoOutputRun::Sync(Box::new(move || {
                    tracing::trace!("static img from net: Saving output from {url}");
                    if let Err(e) = MediaCache::write(&path, &url, img) {
                        tracing::error!("{e}");
                    }
                })),
            ),
        )
    })))
}
