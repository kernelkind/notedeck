use std::collections::HashMap;

use egui::{
    Button, Color32, Context, CornerRadius, FontId, Image, Response, Sense, TextureHandle, Window,
};
use notedeck::{
    fonts::get_font_size, note::MediaAction, supported_mime_hosted_at_url, GifState, GifStateMap,
    Images, JobPool, MediaCacheType, NotedeckTextStyle, TexturedImage, UrlMimes,
};

use crate::{
    blur::{
        blur_media, compute_blurhash, Blur, BlurType, PixelDimensions, PointDimensions,
        RenderableBlur,
    },
    gif::{handle_repaint, retrieve_latest_texture},
    images::{render_images, ImageType, MediaUIAction},
    jobs::{BlurhashParams, Job, JobId, JobParams, JobState, JobsCache},
    AnimationHelper, PulseAlpha,
};

pub(crate) fn image_carousel(
    ui: &mut egui::Ui,
    img_cache: &mut Images,
    job_pool: &mut JobPool,
    jobs: &mut JobsCache,
    medias: Vec<MediaRenderType>,
    carousel_id: egui::Id,
) -> Option<MediaAction> {
    // let's make sure everything is within our area

    let height = 360.0;
    let width = ui.available_size().x;
    let spinsz = if height > width { width } else { height };

    let show_popup = ui.ctx().memory(|mem| {
        mem.data
            .get_temp(carousel_id.with("show_popup"))
            .unwrap_or(false)
    });

    let current_image = 'scope: {
        if !show_popup {
            break 'scope None;
        }

        let MediaRenderType::Trusted(media) = &medias[0] else {
            break 'scope None;
        };

        Some(ui.ctx().memory(|mem| {
            mem.data
                .get_temp::<(String, MediaCacheType)>(carousel_id.with("current_image"))
                .unwrap_or_else(|| (media.url.to_owned(), media.media_type.clone()))
        }))
    };
    let mut action = None;

    ui.add_sized([width, height], |ui: &mut egui::Ui| {
        egui::ScrollArea::horizontal()
            .id_salt(carousel_id)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for media in medias {
                        if let Some(cur_action) = render_media(
                            ui,
                            img_cache,
                            job_pool,
                            jobs,
                            media,
                            height,
                            spinsz,
                            carousel_id,
                        ) {
                            action = Some(cur_action)
                        }
                    }
                })
                .response
            })
            .inner
    });

    if show_popup {
        if let Some((image_url, cache_type)) = current_image {
            show_full_screen_media(ui, &image_url, cache_type, img_cache, carousel_id);
        }
    }
    action
}

fn show_full_screen_media(
    ui: &mut egui::Ui,
    image_url: &str,
    cache_type: MediaCacheType,
    img_cache: &mut Images,
    carousel_id: egui::Id,
) {
    Window::new("image_popup")
        .title_bar(false)
        .fixed_size(ui.ctx().screen_rect().size())
        .fixed_pos(ui.ctx().screen_rect().min)
        .frame(egui::Frame::NONE)
        .show(ui.ctx(), |ui| {
            ui.centered_and_justified(|ui| {
                let ctx_cloned = ui.ctx().clone();

                render_images(
                    ctx_cloned,
                    img_cache,
                    cache_type,
                    image_url,
                    ImageType::Content,
                    |state, gif_states| 's: {
                        let notedeck::TextureState::Loaded(textured_image) = state else {
                            break 's None;
                        };

                        render_full_screen_media(
                            ui,
                            textured_image,
                            gif_states,
                            image_url,
                            carousel_id,
                        )
                    },
                )
            })
        });
}

fn render_full_screen_media(
    ui: &mut egui::Ui,
    textured_image: &mut TexturedImage,
    gif_states: &mut HashMap<String, GifState>,
    image_url: &str,
    carousel_id: egui::Id,
) -> Option<MediaUIAction> {
    let screen_rect = ui.ctx().screen_rect();

    // escape
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(carousel_id.with("show_popup"), false);
        });
    }

    // background
    ui.painter()
        .rect_filled(screen_rect, 0.0, Color32::from_black_alpha(230));

    // zoom init
    let zoom_id = carousel_id.with("zoom_level");
    let mut zoom = ui
        .ctx()
        .memory(|mem| mem.data.get_temp(zoom_id).unwrap_or(1.0_f32));

    // pan init
    let pan_id = carousel_id.with("pan_offset");
    let mut pan_offset = ui
        .ctx()
        .memory(|mem| mem.data.get_temp(pan_id).unwrap_or(egui::Vec2::ZERO));

    // zoom & scroll
    if ui.input(|i| i.pointer.hover_pos()).is_some() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta.y != 0.0 {
            let zoom_factor = if scroll_delta.y > 0.0 { 1.05 } else { 0.95 };
            zoom *= zoom_factor;
            zoom = zoom.clamp(0.1, 5.0);

            if zoom <= 1.0 {
                pan_offset = egui::Vec2::ZERO;
            }

            ui.ctx().memory_mut(|mem| {
                mem.data.insert_temp(zoom_id, zoom);
                mem.data.insert_temp(pan_id, pan_offset);
            });
        }
    }

    let texture = handle_repaint(
        ui,
        retrieve_latest_texture(image_url, gif_states, textured_image),
    );

    let texture_size = texture.size_vec2();
    let screen_size = ui.ctx().screen_rect().size();
    let scale = (screen_size.x / texture_size.x)
        .min(screen_size.y / texture_size.y)
        .min(1.0);
    let scaled_size = texture_size * scale * zoom;

    let visible_width = scaled_size.x.min(screen_size.x);
    let visible_height = scaled_size.y.min(screen_size.y);

    let max_pan_x = ((scaled_size.x - visible_width) / 2.0).max(0.0);
    let max_pan_y = ((scaled_size.y - visible_height) / 2.0).max(0.0);

    if max_pan_x > 0.0 {
        pan_offset.x = pan_offset.x.clamp(-max_pan_x, max_pan_x);
    } else {
        pan_offset.x = 0.0;
    }

    if max_pan_y > 0.0 {
        pan_offset.y = pan_offset.y.clamp(-max_pan_y, max_pan_y);
    } else {
        pan_offset.y = 0.0;
    }

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(visible_width, visible_height),
        egui::Sense::click_and_drag(),
    );

    let uv_min = egui::pos2(
        0.5 - (visible_width / scaled_size.x) / 2.0 + pan_offset.x / scaled_size.x,
        0.5 - (visible_height / scaled_size.y) / 2.0 + pan_offset.y / scaled_size.y,
    );

    let uv_max = egui::pos2(
        uv_min.x + visible_width / scaled_size.x,
        uv_min.y + visible_height / scaled_size.y,
    );

    let uv = egui::Rect::from_min_max(uv_min, uv_max);

    ui.painter()
        .image(texture.id(), rect, uv, egui::Color32::WHITE);
    let img_rect = ui.allocate_rect(rect, Sense::click());

    if img_rect.clicked() {
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(carousel_id.with("show_popup"), true);
        });
    } else if img_rect.clicked_elsewhere() {
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(carousel_id.with("show_popup"), false);
        });
    }

    // Handle dragging for pan
    if response.dragged() {
        let delta = response.drag_delta();

        pan_offset.x -= delta.x;
        pan_offset.y -= delta.y;

        if max_pan_x > 0.0 {
            pan_offset.x = pan_offset.x.clamp(-max_pan_x, max_pan_x);
        } else {
            pan_offset.x = 0.0;
        }

        if max_pan_y > 0.0 {
            pan_offset.y = pan_offset.y.clamp(-max_pan_y, max_pan_y);
        } else {
            pan_offset.y = 0.0;
        }

        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(pan_id, pan_offset);
        });
    }

    // reset zoom on double-click
    if response.double_clicked() {
        pan_offset = egui::Vec2::ZERO;
        zoom = 1.0;
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(pan_id, pan_offset);
            mem.data.insert_temp(zoom_id, zoom);
        });
    }

    copy_link(image_url, response);
    None
}

fn copy_link(url: &str, img_resp: Response) {
    img_resp.context_menu(|ui| {
        if ui.button("Copy Link").clicked() {
            ui.ctx().copy_text(url.to_owned());
            ui.close_menu();
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn render_media(
    ui: &mut egui::Ui,
    img_cache: &mut Images,
    job_pool: &mut JobPool,
    jobs: &mut JobsCache,
    media_type: MediaRenderType,
    height: f32,
    spinsz: f32,
    carousel_id: egui::Id,
) -> Option<MediaAction> {
    match media_type {
        MediaRenderType::Trusted(renderable_media) => {
            render_trusted_media(
                ui,
                img_cache,
                &renderable_media,
                height,
                spinsz,
                carousel_id,
                jobs,
            );
            None
        }
        MediaRenderType::Untrusted(blur_type) => match blur_type {
            BlurType::Blurhash(renderable_blur) => {
                let available_points = PointDimensions {
                    x: ui.available_width(),
                    y: height,
                };

                let pixel_sizes = renderable_blur
                    .blur
                    .scaled_pixel_dimensions(ui, available_points);

                render_blurhash(ui, job_pool, jobs, &renderable_blur, pixel_sizes, height)
            }
            BlurType::Default(url) => {
                let resp = render_default_blur(ui, height, url);

                if resp.clicked() {
                    Some(MediaAction::Unblur(url.to_owned()))
                } else {
                    None
                }
            }
        },
    }
}

fn render_blur_text(ui: &mut egui::Ui, url: &str, render_rect: egui::Rect) -> egui::Response {
    let helper = AnimationHelper::new_from_rect(ui, ("show_media", url), render_rect);

    let painter = ui.painter_at(helper.get_animation_rect());

    let text_style = NotedeckTextStyle::Button;

    let icon_data = egui::include_image!("../../../../assets/icons/eye-slash-dark.png");

    let icon_size = helper.scale_1d_pos(30.0);
    let animation_fontid = FontId::new(
        helper.scale_1d_pos(get_font_size(ui.ctx(), &text_style)),
        text_style.font_family(),
    );
    let info_galley = painter.layout(
        "Media from someone you don't follow".to_owned(),
        animation_fontid.clone(),
        ui.visuals().text_color(),
        render_rect.width() / 2.0,
    );

    let load_galley = painter.layout_no_wrap(
        "Tap to Load".to_owned(),
        animation_fontid,
        egui::Color32::BLACK,
        // ui.visuals().widgets.inactive.bg_fill,
    );

    let items_height = info_galley.rect.height() + load_galley.rect.height() + icon_size;

    let spacing = helper.scale_1d_pos(8.0);
    let icon_rect = {
        let mut center = helper.get_animation_rect().center();
        center.y -= (items_height / 2.0) + (spacing * 3.0) - (icon_size / 2.0);

        egui::Rect::from_center_size(center, egui::vec2(icon_size, icon_size))
    };

    egui::Image::new(icon_data)
        .max_width(icon_size)
        .paint_at(ui, icon_rect);

    let info_galley_pos = {
        let mut pos = icon_rect.center();
        pos.x -= info_galley.rect.width() / 2.0;
        pos.y = icon_rect.bottom() + spacing;
        pos
    };

    let load_galley_pos = {
        let mut pos = icon_rect.center();
        pos.x -= load_galley.rect.width() / 2.0;
        pos.y = icon_rect.bottom() + info_galley.rect.height() + (4.0 * spacing);
        pos
    };

    let button_rect = egui::Rect::from_min_size(load_galley_pos, load_galley.size()).expand(8.0);

    let button_fill = egui::Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x1F);

    painter.rect(
        button_rect,
        egui::CornerRadius::same(8),
        button_fill,
        egui::Stroke::NONE,
        egui::StrokeKind::Middle,
    );

    painter.galley(info_galley_pos, info_galley, egui::Color32::WHITE);
    painter.galley(load_galley_pos, load_galley, egui::Color32::WHITE);

    helper.take_animation_response()
}

fn render_default_blur(ui: &mut egui::Ui, height: f32, url: &str) -> egui::Response {
    let rect = render_default_blur_bg(ui, height, url, false);
    render_blur_text(ui, url, rect)
}

fn render_default_blur_bg(ui: &mut egui::Ui, height: f32, url: &str, shimmer: bool) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(height, height), egui::Sense::click());

    let painter = ui.painter_at(rect);

    let mut color = crate::colors::MID_GRAY;
    if shimmer {
        let [r, g, b, _a] = color.to_srgba_unmultiplied();
        let cur_alpha = get_blur_current_alpha(ui, url);
        color = Color32::from_rgba_unmultiplied(r, g, b, cur_alpha)
    }

    painter.rect_filled(rect, CornerRadius::same(8), color);

    rect
}

fn render_blurhash(
    ui: &mut egui::Ui,
    job_pool: &mut JobPool,
    jobs: &mut JobsCache,
    renderable_blur: &RenderableBlur,
    dims: PixelDimensions,
    max_height: f32,
) -> Option<MediaAction> {
    let params = BlurhashParams {
        blurhash: renderable_blur.blur.blurhash,
        url: renderable_blur.url,
        ctx: ui.ctx(),
    };

    let job_state = jobs.get_or_insert_with(
        job_pool,
        &JobId::Blurhash(renderable_blur.url),
        Some(JobParams::Blurhash(params)),
        move |params| compute_blurhash(params, dims),
    );

    let JobState::Completed(m_blur_job) = job_state else {
        return None;
    };

    #[allow(irrefutable_let_patterns)]
    let Job::Blurhash(m_texture_handle) = m_blur_job
    else {
        tracing::error!("Did not get the correct job type: {:?}", m_blur_job);
        return None;
    };

    let Some(texture_handle) = &m_texture_handle else {
        return None;
    };

    let resp = ui.add(texture_to_image(texture_handle, max_height));

    if render_blur_text(ui, renderable_blur.url, resp.rect)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        Some(MediaAction::Unblur(renderable_blur.url.to_owned()))
    } else {
        None
    }
}

pub(crate) struct RenderableMedia<'a> {
    url: &'a str,
    media_type: MediaCacheType,
}

pub(crate) enum MediaRenderType<'a> {
    Trusted(RenderableMedia<'a>),
    Untrusted(BlurType<'a>),
}

pub(crate) fn find_supported_media_type<'a>(
    ui: &mut egui::Ui,
    urls: &mut UrlMimes,
    blurhashes: &'a HashMap<&'a str, Blur<'a>>,
    media_trusted: bool,
    url: &'a str,
) -> Option<MediaRenderType<'a>> {
    let media_type = supported_mime_hosted_at_url(urls, url)?;

    if blur_media(ui.ctx(), url, media_trusted) {
        let blur_type = match blurhashes.get(url) {
            Some(blur) => BlurType::Blurhash(RenderableBlur { url, blur }),
            None => BlurType::Default(url),
        };
        Some(MediaRenderType::Untrusted(blur_type))
    } else {
        Some(MediaRenderType::Trusted(RenderableMedia {
            url,
            media_type,
        }))
    }
}

fn render_trusted_media(
    ui: &mut egui::Ui,
    img_cache: &mut Images,
    renderable_media: &RenderableMedia,
    height: f32,
    spinsz: f32,
    carousel_id: egui::Id,
    jobs: &JobsCache,
) {
    let ctx = ui.ctx().clone();
    let url = renderable_media.url;
    let cache_type = renderable_media.media_type.clone();
    render_images(
        ctx,
        img_cache,
        cache_type,
        url,
        ImageType::Content,
        |cur_state, gif_states| match cur_state {
            notedeck::TextureState::Pending => {
                shimmer_loading_media(jobs, ui, url, height);
                None
            }
            notedeck::TextureState::Error(_) => {
                ui.allocate_space(egui::vec2(spinsz, spinsz));
                None
            }
            notedeck::TextureState::Loading { actual_image_tex } => {
                show_image_transition(jobs, ui, height, url, actual_image_tex)
            }
            notedeck::TextureState::Loaded(textured_image) => {
                render_success_media(
                    ui,
                    url,
                    textured_image,
                    gif_states,
                    &renderable_media.media_type,
                    height,
                    carousel_id,
                );
                None
            }
        },
    );
}

fn render_success_media(
    ui: &mut egui::Ui,
    url: &str,
    tex: &mut TexturedImage,
    gifs: &mut GifStateMap,
    cache_type: &MediaCacheType,
    height: f32,
    carousel_id: egui::Id,
) {
    let texture = handle_repaint(ui, retrieve_latest_texture(url, gifs, tex));
    let img = texture_to_image(texture, height);
    let img_resp = ui.add(Button::image(img).frame(false));

    if img_resp.clicked() {
        ui.ctx().memory_mut(|mem| {
            mem.data.insert_temp(carousel_id.with("show_popup"), true);
            mem.data.insert_temp(
                carousel_id.with("current_image"),
                (url.to_owned(), cache_type.clone()),
            );
        });
    }

    copy_link(url, img_resp);
}

fn texture_to_image(tex: &TextureHandle, max_height: f32) -> egui::Image {
    Image::new(tex)
        .max_height(max_height)
        .corner_radius(5.0)
        .maintain_aspect_ratio(true)
}

fn shimmer_loading_media(jobs: &JobsCache, ui: &mut egui::Ui, url: &str, max_height: f32) {
    if let Some(JobState::Completed(Job::Blurhash(Some(blur_texture)))) =
        jobs.get(&JobId::Blurhash(url))
    {
        shimmer_blurhash(blur_texture, ui, url, max_height);
    } else {
        render_default_blur_bg(ui, max_height, url, true);
    };
}

static BLUR_SHIMMER_ID: fn(&str) -> egui::Id = |url| egui::Id::new(("blur_shimmer", url));

fn get_blur_current_alpha(ui: &mut egui::Ui, url: &str) -> u8 {
    let id = BLUR_SHIMMER_ID(url);

    let (alpha_min, alpha_max) = if ui.visuals().dark_mode {
        (150, 255)
    } else {
        (220, 255)
    };
    PulseAlpha::new(ui.ctx(), id, alpha_min, alpha_max)
        .with_speed(0.3)
        .start_max_alpha()
        .animate()
}

fn shimmer_blurhash(tex: &TextureHandle, ui: &mut egui::Ui, url: &str, max_height: f32) {
    let cur_alpha = get_blur_current_alpha(ui, url);

    let scaled = ScaledTexture::new(tex, max_height);
    let img = scaled.get_image();
    show_blurhash_with_alpha(ui, img, cur_alpha);
}

fn fade_color(alpha: u8) -> egui::Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
}

fn show_blurhash_with_alpha(ui: &mut egui::Ui, img: Image, alpha: u8) {
    let cur_color = fade_color(alpha);

    let img = img.tint(cur_color);

    ui.add(img);
}

type FinishedTransition = bool;

fn show_transition(
    jobs: &JobsCache,
    ui: &mut egui::Ui,
    max_height: f32,
    url: &str,
    image_tex: &TexturedImage,
) -> FinishedTransition {
    let image_handle = image_tex.get_first_texture();
    match get_transition_type(jobs, url) {
        TransitionType::Blur { blur_texture } => {
            render_blur_transition(ui, url, max_height, blur_texture, image_handle)
        }
        TransitionType::Default => {
            ui.add(texture_to_image(image_handle, max_height));
            true
        }
    }
}

pub fn show_image_transition(
    jobs: &JobsCache,
    ui: &mut egui::Ui,
    max_height: f32,
    url: &str,
    image_tex: &TexturedImage,
) -> Option<MediaUIAction> {
    if show_transition(jobs, ui, max_height, url, image_tex) {
        Some(MediaUIAction::DoneLoading)
    } else {
        None
    }
}

// return true if transition is finished
fn render_blur_transition(
    ui: &mut egui::Ui,
    url: &str,
    max_height: f32,
    blur_texture: &TextureHandle,
    image_texture: &TextureHandle,
) -> FinishedTransition {
    let scaled_texture = ScaledTexture::new(image_texture, max_height);

    let blur_img = texture_to_image(blur_texture, max_height);
    match get_blur_transition_state(ui.ctx(), url) {
        BlurTransitionState::StoppingShimmer { cur_alpha } => {
            show_blurhash_with_alpha(ui, blur_img, cur_alpha);
            false
        }
        BlurTransitionState::FadingBlur => render_blur_fade(ui, url, blur_img, &scaled_texture),
    }
}

struct ScaledTexture<'a> {
    tex: &'a TextureHandle,
    max_height: f32,
    pub scaled_size: egui::Vec2,
}

impl<'a> ScaledTexture<'a> {
    pub fn new(tex: &'a TextureHandle, max_height: f32) -> Self {
        let scaled_size = {
            let mut size = tex.size_vec2();

            if size.y > max_height {
                let old_y = size.y;
                size.y = max_height;
                size.x *= max_height / old_y;
            }

            size
        };

        Self {
            tex,
            max_height,
            scaled_size,
        }
    }

    pub fn get_image(&self) -> Image {
        texture_to_image(self.tex, self.max_height)
            .max_size(self.scaled_size)
            .shrink_to_fit()
    }
}

fn render_blur_fade(
    ui: &mut egui::Ui,
    url: &str,
    blur_img: Image,
    image_texture: &ScaledTexture,
) -> FinishedTransition {
    let blur_fade_id = ui.id().with(("blur_fade", url));

    let cur_alpha = {
        PulseAlpha::new(ui.ctx(), blur_fade_id, 0, 255)
            .start_max_alpha()
            .with_speed(0.3)
            .animate()
    };

    let img = image_texture.get_image();

    let blur_img = blur_img.tint(fade_color(cur_alpha));

    let alloc_size = image_texture.scaled_size;

    let (rect, _) = ui.allocate_exact_size(alloc_size, egui::Sense::hover());

    img.paint_at(ui, rect);
    blur_img.paint_at(ui, rect);

    cur_alpha == 0
}

fn get_blur_transition_state(ctx: &Context, url: &str) -> BlurTransitionState {
    let shimmer_id = BLUR_SHIMMER_ID(url);

    let max_alpha = 255.0;
    let cur_shimmer_alpha = ctx.animate_value_with_time(shimmer_id, max_alpha, 0.3);
    if cur_shimmer_alpha == max_alpha {
        BlurTransitionState::FadingBlur
    } else {
        let cur_alpha = (cur_shimmer_alpha).clamp(0.0, max_alpha) as u8;
        BlurTransitionState::StoppingShimmer { cur_alpha }
    }
}

enum BlurTransitionState {
    StoppingShimmer { cur_alpha: u8 },
    FadingBlur,
}

fn get_transition_type<'a>(jobs: &'a JobsCache, url: &str) -> TransitionType<'a> {
    if let Some(JobState::Completed(Job::Blurhash(Some(blur_texture)))) =
        jobs.get(&JobId::Blurhash(url))
    {
        TransitionType::Blur { blur_texture }
    } else {
        TransitionType::Default
    }
}

enum TransitionType<'a> {
    Blur { blur_texture: &'a TextureHandle },
    Default,
}
