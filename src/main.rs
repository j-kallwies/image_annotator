#![windows_subsystem = "windows"]

use clap::Arg;
use clap::Command;
use log::debug;
use log::error;
use log::info;
use log::trace;
use log::warn;
use nalgebra::Vector2;
use notan::app::Event;
use notan::draw::*;
use notan::egui::CursorIcon;
use notan::egui::{self, *};
use notan::prelude::*;
use shortcuts::key_pressed;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc;
pub mod cache;
pub mod scrubber;
pub mod settings;
pub mod shortcuts;
#[cfg(feature = "turbo")]
use crate::scrubber::find_first_image_in_directory;
use crate::settings::set_system_theme;
use crate::settings::ColorTheme;
use crate::shortcuts::InputEvent::*;
mod utils;
use utils::*;
mod appstate;
mod image_loader;
use appstate::*;
// mod events;
#[cfg(target_os = "macos")]
mod mac;
mod net;
use net::*;
#[cfg(test)]
mod tests;
mod ui;
#[cfg(feature = "update")]
mod update;
use ui::*;
mod yolo_labels;
use yolo_labels::Unnormaliser;

pub const FONT: &[u8; 309828] = include_bytes!("../res/fonts/Inter-Regular.ttf");

#[notan_main]
fn main() -> Result<(), String> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "warning");
    }
    // on debug builds, override log level
    #[cfg(debug_assertions)]
    {
        std::env::set_var("RUST_LOG", "debug");
        let _ = env_logger::try_init();
    }

    let icon_data = include_bytes!("../icon.ico");

    let mut window_config = WindowConfig::new()
        .set_title(&format!("Oculante | {}", env!("CARGO_PKG_VERSION")))
        .set_size(1026, 600) // window's size
        .set_resizable(true) // window can be resized
        .set_window_icon_data(Some(icon_data))
        .set_taskbar_icon_data(Some(icon_data))
        .set_multisampling(0)
        .set_min_size(200, 200);

    #[cfg(target_os = "windows")]
    {
        window_config = window_config
            .set_lazy_loop(true)
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "linux")]
    {
        window_config = window_config
            .set_lazy_loop(true)
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "netbsd")]
    {
        window_config = window_config.set_lazy_loop(true).set_vsync(true);
    }

    #[cfg(target_os = "macos")]
    {
        window_config = window_config
            .set_lazy_loop(true)
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "macos")]
    {
        // MacOS needs an incredible dance performed just to open a file
        let _ = mac::launch();
    }

    // Unfortunately we need to load the persistent settings here, too - the window settings need
    // to be set before window creation
    match settings::PersistentSettings::load() {
        Ok(settings) => {
            window_config.vsync = settings.vsync;
            if settings.window_geometry != Default::default() {
                window_config.width = settings.window_geometry.1 .0 as u32;
                window_config.height = settings.window_geometry.1 .1 as u32;
            }
            debug!("Loaded settings.");
            if settings.zen_mode {
                let mut title_string = window_config.title.clone();
                title_string.push_str(&format!(
                    "          '{}' to disable zen mode",
                    shortcuts::lookup(&settings.shortcuts, &shortcuts::InputEvent::ZenMode)
                ));
                window_config = window_config.set_title(&title_string);
            }
        }
        Err(e) => {
            error!("Could not load settings: {e}");
        }
    }
    window_config.always_on_top = true;

    info!("Starting oculante.");
    notan::init_with(init)
        .add_config(window_config)
        .add_config(EguiConfig)
        .add_config(DrawConfig)
        .event(event)
        .update(update)
        .draw(drawe)
        .build()
}

fn init(gfx: &mut Graphics, plugins: &mut Plugins) -> OculanteState {
    info!("Now matching arguments {:?}", std::env::args());
    // Filter out strange mac args
    let args: Vec<String> = std::env::args().filter(|a| !a.contains("psn_")).collect();

    let matches = Command::new("Oculante")
        .arg(
            Arg::new("INPUT")
                .help("Display this image")
                // .required(true)
                .index(1),
        )
        .arg(
            Arg::new("l")
                .short('l')
                .help("Listen on port")
                .takes_value(true),
        )
        .arg(
            Arg::new("chainload")
                .required(false)
                .takes_value(false)
                .short('c')
                .help("Chainload on Mac"),
        )
        .get_matches_from(args);

    debug!("Completed argument parsing.");

    let maybe_img_location = matches.value_of("INPUT").map(PathBuf::from);

    let mut state = OculanteState {
        texture_channel: mpsc::channel(),
        // current_path: maybe_img_location.cloned(/),
        ..Default::default()
    };

    match settings::PersistentSettings::load() {
        Ok(settings) => {
            state.persistent_settings = settings;
            info!("Successfully loaded previous settings.")
        }
        Err(e) => {
            warn!("Settings failed to load: {e}. This may happen after application updates. Generating a fresh file.");
            state.persistent_settings = Default::default();
            state.persistent_settings.save();
        }
    }

    state.player = Player::new(
        state.texture_channel.0.clone(),
        state.persistent_settings.max_cache,
        gfx.limits().max_texture_size,
    );

    debug!("Image is: {:?}", maybe_img_location);

    if let Some(ref location) = maybe_img_location {
        // Check if path is a directory or a file (and that it even exists)
        let mut start_img_location: Option<PathBuf> = None;

        if let Ok(maybe_location_metadata) = location.metadata() {
            if maybe_location_metadata.is_dir() {
                // Folder - Pick first image from the folder...
                if let Ok(first_img_location) = find_first_image_in_directory(location) {
                    start_img_location = Some(first_img_location);
                }
            } else if is_ext_compatible(location) {
                // Image File with a usable extension
                start_img_location = Some(location.clone());
            } else {
                // Unsupported extension
                state.send_message(&format!("ERROR: Unsupported file: {} - Open Github issue if you think this should not happen.", location.display()));
            }
        } else {
            // Not a valid path, or user doesn't have permission to access?
            state.send_message(&format!("ERROR: Can't open file: {}", location.display()));
        }

        // Assign image path if we have a valid one here
        if let Some(img_location) = start_img_location {
            state.is_loaded = false;
            state.current_path = Some(img_location.clone());
            state
                .player
                .load(&img_location, state.message_channel.0.clone());
        }
    }

    if let Some(port) = matches.value_of("l") {
        match port.parse::<i32>() {
            Ok(p) => {
                state.message = Some(Message::info(&format!("Listening on {p}")));
                recv(p, state.texture_channel.0.clone());
                state.current_path = Some(PathBuf::from(&format!("network port {p}")));
                state.network_mode = true;
            }
            Err(_) => error!("Port must be a number"),
        }
    }

    // Set up egui style
    plugins.egui(|ctx| {
        let mut fonts = FontDefinitions::default();

        fonts
            .font_data
            .insert("my_font".to_owned(), FontData::from_static(FONT));

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "my_font".to_owned());

        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        match state.persistent_settings.theme {
            ColorTheme::Light => ctx.set_visuals(Visuals::light()),
            ColorTheme::Dark => ctx.set_visuals(Visuals::dark()),
            ColorTheme::System => set_system_theme(ctx),
        }

        let mut style: egui::Style = (*ctx.style()).clone();
        let font_scale = 0.80;

        style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 18. * font_scale;
        style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 18. * font_scale;
        style.text_styles.get_mut(&TextStyle::Small).unwrap().size = 15. * font_scale;
        style.text_styles.get_mut(&TextStyle::Heading).unwrap().size = 22. * font_scale;
        debug!("Accent color: {:?}", state.persistent_settings.accent_color);
        style.visuals.selection.bg_fill = Color32::from_rgb(
            state.persistent_settings.accent_color[0],
            state.persistent_settings.accent_color[1],
            state.persistent_settings.accent_color[2],
        );

        let accent_color = style.visuals.selection.bg_fill.to_array();

        let accent_color_luma = (accent_color[0] as f32 * 0.299
            + accent_color[1] as f32 * 0.587
            + accent_color[2] as f32 * 0.114)
            .max(0.)
            .min(255.) as u8;
        let accent_color_luma = if accent_color_luma < 80 { 220 } else { 80 };
        // Set text on highlighted elements
        style.visuals.selection.stroke = Stroke::new(2.0, Color32::from_gray(accent_color_luma));
        ctx.set_fonts(fonts);

        ctx.set_style(style);
    });

    // load checker texture
    if let Ok(checker_image) = image::load_from_memory(include_bytes!("../res/checker.png")) {
        // state.checker_texture = checker_image.into_rgba8().to_texture(gfx);
        // No mipmaps for the checker pattern!
        let img = checker_image.into_rgba8();
        state.checker_texture = gfx
            .create_texture()
            .from_bytes(&img, img.width(), img.height())
            .with_mipmaps(false)
            .with_format(notan::prelude::TextureFormat::SRgba8)
            .build()
            .ok();
    }

    state
}

fn event(app: &mut App, state: &mut OculanteState, evt: Event) {
    match evt {
        Event::KeyUp { .. } => {
            // Fullscreen needs to be on key up on mac (bug)
            if key_pressed(app, state, Fullscreen) {
                toggle_fullscreen(app, state);
            }
        }
        Event::KeyDown { .. } => {
            debug!("key down");

            // return;
            // pan image with keyboard
            let delta = 40.;
            if key_pressed(app, state, PanRight) {
                state.image_geometry.offset.x += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanUp) {
                state.image_geometry.offset.y -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanLeft) {
                state.image_geometry.offset.x -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanDown) {
                state.image_geometry.offset.y += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, CompareNext) {
                compare_next(state);
            }
            if key_pressed(app, state, ResetView) {
                state.reset_image = true
            }
            if key_pressed(app, state, ZenMode) {
                toggle_zen_mode(state, app);
            }
            if key_pressed(app, state, ZoomActualSize) {
                set_zoom(1.0, None, state);
            }
            if key_pressed(app, state, ZoomDouble) {
                set_zoom(2.0, None, state);
            }
            if key_pressed(app, state, ZoomThree) {
                set_zoom(3.0, None, state);
            }
            if key_pressed(app, state, ZoomFour) {
                set_zoom(4.0, None, state);
            }
            if key_pressed(app, state, ZoomFive) {
                set_zoom(5.0, None, state);
            }
            if key_pressed(app, state, Quit) {
                state.persistent_settings.save_blocking();
                app.backend.exit();
            }
            #[cfg(feature = "file_open")]
            if key_pressed(app, state, Browse) {
                state.redraw = true;
                browse_for_image_path(state);
            }
            if key_pressed(app, state, NextImage) {
                if state.is_loaded {
                    next_image(state)
                }
            }
            if key_pressed(app, state, PreviousImage) {
                if state.is_loaded {
                    prev_image(state)
                }
            }
            if key_pressed(app, state, FirstImage) {
                first_image(state)
            }
            if key_pressed(app, state, LastImage) {
                last_image(state)
            }
            if key_pressed(app, state, AlwaysOnTop) {
                state.always_on_top = !state.always_on_top;
                app.window().set_always_on_top(state.always_on_top);
            }
            if key_pressed(app, state, InfoMode) {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
                send_extended_info(
                    &state.current_image,
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }
            #[cfg(not(target_os = "netbsd"))]
            if key_pressed(app, state, DeleteAnnoation) {
                if let Some(id) = state.selected_bbox_id {
                    state.annotation_bboxes.remove(id);
                    state.selected_bbox_id = None;
                    state.send_message("Deleted annotation");
                }
            }
            if key_pressed(app, state, ZoomIn) {
                let delta = zoomratio(3.5, state.image_geometry.scale);
                let new_scale = state.image_geometry.scale + delta;
                // limit scale
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    state.image_geometry.offset -= scale_pt(
                        state.image_geometry.offset,
                        center,
                        state.image_geometry.scale,
                        delta,
                    );
                    state.image_geometry.scale += delta;
                }
            }
            if key_pressed(app, state, ZoomOut) {
                let delta = zoomratio(-3.5, state.image_geometry.scale);
                let new_scale = state.image_geometry.scale + delta;
                // limit scale
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    state.image_geometry.offset -= scale_pt(
                        state.image_geometry.offset,
                        center,
                        state.image_geometry.scale,
                        delta,
                    );
                    state.image_geometry.scale += delta;
                }
            }
        }
        Event::WindowResize { width, height } => {
            //TODO: remove this if save on exit works
            state.persistent_settings.window_geometry.1 = (width, height);
            state.persistent_settings.window_geometry.0 = (
                app.backend.window().position().0 as u32,
                app.backend.window().position().1 as u32,
            );
            // By resetting the image, we make it fill the window on resize
            if state.persistent_settings.zen_mode {
                state.reset_image = true;
            }
        }
        _ => (),
    }

    // let mouse_pos = app.mouse.position();
    // // dbg!(mouse_pos);

    // let cursor_relative = pos_from_coord(
    //     state.image_geometry.offset,
    //     mouse_pos.size_vec(),
    //     Vector2::new(
    //         state.image_dimension.0 as f32,
    //         state.image_dimension.1 as f32,
    //     ),
    //     state.image_geometry.scale,
    // );
    // dbg!(cursor_relative);

    match evt {
        Event::Exit => {
            info!("About to exit");
            // save position
            state.persistent_settings.window_geometry = (
                (
                    app.window().position().0 as u32,
                    app.window().position().1 as u32,
                ),
                app.window().size(),
            );
            state.persistent_settings.save_blocking();
        }
        Event::MouseWheel { delta_y, .. } => {
            if !state.pointer_over_ui {
                if app.keyboard.ctrl() {
                    // Change image to next/prev
                    // - map scroll-down == next, as that's the natural scrolling direction
                    if delta_y > 0.0 {
                        prev_image(state)
                    } else {
                        next_image(state)
                    }
                } else {
                    let divisor = if cfg!(macos) { 0.1 } else { 10. };
                    // Normal scaling
                    let delta = zoomratio(
                        (delta_y / divisor).max(-5.0).min(5.0),
                        state.image_geometry.scale,
                    );
                    trace!("Delta {delta}, raw {delta_y}");
                    let new_scale = state.image_geometry.scale + delta;
                    // limit scale
                    if new_scale > 0.01 && new_scale < 40. {
                        state.image_geometry.offset -= scale_pt(
                            state.image_geometry.offset,
                            state.cursor,
                            state.image_geometry.scale,
                            delta,
                        );
                        state.image_geometry.scale += delta;
                    }
                }
            }
        }

        Event::Drop(file) => {
            if let Some(p) = file.path {
                if let Some(ext) = p.extension() {
                    if SUPPORTED_EXTENSIONS.contains(&ext.to_string_lossy().to_string().as_str()) {
                        state.is_loaded = false;
                        state.current_image = None;
                        state.player.load(&p, state.message_channel.0.clone());
                        state.current_path = Some(p);
                    } else {
                        state.message = Some(Message::warn("Unsupported file!"));
                    }
                }
            }
        }
        Event::MouseDown { button, .. } => match button {
            MouseButton::Right | MouseButton::Middle => {
                if !state.mouse_grab {
                    state.drag_enabled = true;
                }
            }
            MouseButton::Left => {
                if state.cursor_within_image {
                    state.bbox_edit_mode.mouse_button_down(
                        state.cursor_relative,
                        &mut state.annotation_bboxes,
                        &mut state.selected_bbox_id,
                    );
                }
            }
            _ => {}
        },
        Event::MouseUp { button, .. } => match button {
            MouseButton::Right | MouseButton::Middle => state.drag_enabled = false,
            MouseButton::Left => {
                state
                    .bbox_edit_mode
                    .mouse_button_up(state.cursor_relative, &mut state.annotation_bboxes);
            }
            _ => {}
        },
        _ => {
            // debug!("{:?}", evt);
        }
    }
}

fn update(app: &mut App, state: &mut OculanteState) {
    if state.first_start {
        app.window().set_always_on_top(false);
    }

    // Save every 1.5 secs
    let t = app.timer.elapsed_f32() % 1.5;
    if t <= 0.01 {
        state.persistent_settings.window_geometry = (
            (
                app.window().position().0 as u32,
                app.window().position().1 as u32,
            ),
            app.window().size(),
        );
        state.persistent_settings.save_blocking();
        trace!("Save {t}");
    }

    let mouse_pos = app.mouse.position();

    state.mouse_delta = Vector2::new(mouse_pos.0, mouse_pos.1) - state.cursor;
    state.cursor = mouse_pos.size_vec();
    (state.cursor_relative, state.cursor_within_image) = pos_from_coord(
        state.image_geometry.offset,
        state.cursor,
        Vector2::new(
            state.image_dimension.0 as f32,
            state.image_dimension.1 as f32,
        ),
        state.image_geometry.scale,
    );

    state
        .bbox_edit_mode
        .update(state.cursor_relative, &mut state.annotation_bboxes);

    if state.drag_enabled {
        if !state.mouse_grab || app.mouse.is_down(MouseButton::Middle) {
            state.image_geometry.offset += state.mouse_delta;
            limit_offset(app, state);
        }
    }

    // Since we can't access the window in the event loop, we store it in the state
    state.window_size = app.window().size().size_vec();

    // redraw if extended info is missing so we make sure it's promply displayed
    if state.persistent_settings.info_enabled && state.image_info.is_none() {
        app.window().request_frame();
    }

    // check extended info has been sent
    if let Ok(info) = state.extended_info_channel.1.try_recv() {
        debug!("Received extended image info for {}", info.name);
        state.image_info = Some(info);
        app.window().request_frame();
    }

    // Only receive messages if current one is cleared
    // debug!("cooldown {}", state.toast_cooldown);

    if state.message.is_none() {
        state.toast_cooldown = 0.;

        // check if a new message has been sent
        if let Ok(msg) = state.message_channel.1.try_recv() {
            debug!("Received message: {:?}", msg);
            match msg {
                Message::LoadError(_) => {
                    state.current_image = None;
                    state.is_loaded = true;
                    state.current_texture = None;
                }
                _ => (),
            }

            state.message = Some(msg);
        }
    }
    state.first_start = false;
}

fn drawe(app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins, state: &mut OculanteState) {
    let mut draw = gfx.create_draw();

    if let Ok(p) = state.load_channel.1.try_recv() {
        state.is_loaded = false;
        state.current_image = None;
        state.player.load(&p, state.message_channel.0.clone());
        if let Some(dir) = p.parent() {
            state.persistent_settings.last_open_directory = dir.to_path_buf();
        }
        state.current_path = Some(p);
        _ = state.persistent_settings.save();
    }

    // check if a new texture has been sent
    if let Ok(frame) = state.texture_channel.1.try_recv() {
        let img = frame.buffer;
        debug!("Received image buffer: {:?}", img.dimensions());
        state.image_dimension = img.dimensions();
        // state.current_texture = img.to_texture(gfx);

        // debug!("Frame source: {:?}", frame.source);

        set_title(app, state);

        // fill image sequence
        if let Some(p) = &state.current_path {
            state.scrubber = scrubber::Scrubber::new(p);
            state.scrubber.wrap = state.persistent_settings.wrap_folder;

            // debug!("{:#?} from {}", &state.scrubber, p.display());
            if !state.persistent_settings.recent_images.contains(p) {
                state.persistent_settings.recent_images.insert(0, p.clone());
                state.persistent_settings.recent_images.truncate(10);
            }
        }

        match frame.source {
            FrameSource::Still => {
                debug!("Received still");

                if !state.persistent_settings.keep_view {
                    state.reset_image = true;

                    if let Some(p) = state.current_path.clone() {
                        if state.persistent_settings.max_cache != 0 {
                            state.player.cache.insert(&p, img.clone());
                        }
                    }
                }
                // always reset if first image
                if state.current_texture.is_none() {
                    state.reset_image = true;
                }

                state.redraw = false;
                state.image_info = None;
            }
            FrameSource::AnimationStart => {
                state.redraw = true;
                state.reset_image = true
            }
            FrameSource::Animation => {
                state.redraw = true;
            }
        }

        if let Some(tex) = &mut state.current_texture {
            if tex.width() as u32 == img.width() && tex.height() as u32 == img.height() {
                img.update_texture(gfx, tex);
            } else {
                state.current_texture = img.to_texture(gfx);
            }
        } else {
            debug!("Setting texture");
            #[cfg(debug_assertions)]
            {
                _ = img.save("debug.png");
            }
            state.current_texture = img.to_texture(gfx);
        }

        state.is_loaded = true;

        state.current_image = Some(img);
        if state.persistent_settings.info_enabled {
            debug!("Sending extended info");
            send_extended_info(
                &state.current_image,
                &state.current_path,
                &state.extended_info_channel,
            );
        }

        // Remove all previous annotations
        state.annotation_bboxes.clear();

        // Load annotations from file, if they are available
        let labels_filename = get_labels_filename(state);
        if let Ok(labels) = yolo_labels::Labels::from_file(labels_filename) {
            for label in labels.labels {
                let label_img =
                    label.unnormalise((state.image_dimension.0, state.image_dimension.1));
                state
                    .annotation_bboxes
                    .push(AnnoationBoundingBox::from_center(
                        label_img.x_centre,
                        label_img.y_centre,
                        label_img.width,
                        label_img.height,
                    ));
            }
        }
    }

    if state.redraw {
        trace!("Force redraw");
        app.window().request_frame();
    }

    if state.reset_image {
        let window_size = app.window().size().size_vec();
        if let Some(current_image) = &state.current_image {
            let img_size = current_image.size_vec();
            let scale_factor = (window_size.x / img_size.x)
                .min(window_size.y / img_size.y)
                .min(1.0);
            state.image_geometry.scale = scale_factor;
            state.image_geometry.offset =
                window_size / 2.0 - (img_size * state.image_geometry.scale) / 2.0;

            debug!("Image has been reset.");
            state.reset_image = false;
        }
        // app.window().request_frame();
    }

    // TODO: Do we need/want a "global" checker?
    // if state.persistent_settings.show_checker_background {
    //     if let Some(checker) = &state.checker_texture {
    //         draw.pattern(checker)
    //             .blend_mode(BlendMode::ADD)
    //             .size(app.window().width() as f32, app.window().height() as f32);
    //     }
    // }

    if let Some(texture) = &state.current_texture {
        if state.persistent_settings.show_checker_background {
            if let Some(checker) = &state.checker_texture {
                draw.pattern(checker)
                    // .size(texture.width() as f32, texture.height() as f32)
                    .size(texture.width() as f32 * state.image_geometry.scale as f32, texture.height() as f32 * state.image_geometry.scale as f32)
                    .blend_mode(BlendMode::ADD)
                    .translate(state.image_geometry.offset.x, state.image_geometry.offset.y)
                    // .scale(state.image_geometry.scale, state.image_geometry.scale)
                    ;
            }
        }
        draw.image(texture)
            .blend_mode(BlendMode::NORMAL)
            .scale(state.image_geometry.scale, state.image_geometry.scale)
            .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);

        if state.persistent_settings.show_frame {
            draw.rect((0.0, 0.0), texture.size())
                .stroke(1.0)
                .color(Color {
                    r: 0.5,
                    g: 0.5,
                    b: 0.5,
                    a: 0.5,
                })
                .blend_mode(BlendMode::ADD)
                .scale(state.image_geometry.scale, state.image_geometry.scale)
                .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        }

        {
            let color = Color {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 0.5,
            };
            let line_width = 2.0 / state.image_geometry.scale;

            draw.line(
                (0.0, state.cursor_relative.y),
                (texture.width(), state.cursor_relative.y),
            )
            .width(line_width)
            .color(color)
            .scale(state.image_geometry.scale, state.image_geometry.scale)
            .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);

            draw.line(
                (state.cursor_relative.x, 0.0),
                (state.cursor_relative.x, texture.height()),
            )
            .width(line_width)
            .color(color)
            .scale(state.image_geometry.scale, state.image_geometry.scale)
            .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        }

        {
            for (current_id, bbox) in state.annotation_bboxes.iter().enumerate() {
                let mut fill = false;
                state.selected_bbox_id.and_then(|selected_bbox_id| {
                    if selected_bbox_id == current_id {
                        fill = true;
                    }
                    Some(selected_bbox_id)
                });

                let line_color = Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 0.75,
                };
                let line_width = 3.0;

                if fill {
                    draw.rect(
                        vector_to_tuple(
                            bbox.tl_corner()
                                - nalgebra::Vector2::new(line_width / 2.0, line_width / 2.0),
                        ),
                        (bbox.width() + line_width, bbox.height() + line_width),
                    )
                    .stroke(line_width)
                    .color(line_color)
                    .blend_mode(BlendMode::NORMAL)
                    .scale(state.image_geometry.scale, state.image_geometry.scale)
                    .translate(state.image_geometry.offset.x, state.image_geometry.offset.y)
                    .fill_color(Color {
                        r: 0.0,
                        g: 1.0,
                        b: 0.0,
                        a: 0.2,
                    })
                    .fill();
                } else {
                    draw.rect(
                        vector_to_tuple(
                            bbox.tl_corner()
                                - nalgebra::Vector2::new(line_width / 2.0, line_width / 2.0),
                        ),
                        (bbox.width() + line_width, bbox.height() + line_width),
                    )
                    .stroke(line_width)
                    .color(line_color)
                    .blend_mode(BlendMode::NORMAL)
                    .scale(state.image_geometry.scale, state.image_geometry.scale)
                    .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
                }
            }
        }

        state.current_bounding_box_element_under_cursor = state
            .bbox_edit_mode
            .get_part_element(state.cursor_relative, &state.annotation_bboxes);

        // if let Some(bbox_element) = state.current_bounding_box_element_under_cursor {
        //     let bbox = &state.annotation_bboxes[bbox_element.id];

        //     let radius = 15.0;
        //     let line_width = 3.0;
        //     let color = Color {
        //         r: 1.0,
        //         g: 1.0,
        //         b: 1.0,
        //         a: 1.0,
        //     };

        //     // let color_edge = Color {
        //     //     r: 0.0,
        //     //     g: 1.0,
        //     //     b: 0.0,
        //     //     a: 1.0,
        //     // };

        //     match bbox_element.part {
        //         BoundingBoxPart::CornerUpperLeft => {
        //             draw.circle(radius)
        //                 .stroke(3.0)
        //                 .color(color)
        //                 .blend_mode(BlendMode::NORMAL)
        //                 .translate(bbox.tl_corner().x, bbox.tl_corner().y)
        //                 .scale(state.image_geometry.scale, state.image_geometry.scale)
        //                 .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }

        //         BoundingBoxPart::CornerUpperRight => {
        //             draw.circle(radius)
        //                 .stroke(line_width)
        //                 .color(color)
        //                 .blend_mode(BlendMode::NORMAL)
        //                 .translate(bbox.tr_corner().x, bbox.tr_corner().y)
        //                 .scale(state.image_geometry.scale, state.image_geometry.scale)
        //                 .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }

        //         BoundingBoxPart::CornerLowerLeft => {
        //             draw.circle(radius)
        //                 .stroke(line_width)
        //                 .color(color)
        //                 .blend_mode(BlendMode::NORMAL)
        //                 .translate(bbox.ll_corner().x, bbox.ll_corner().y)
        //                 .scale(state.image_geometry.scale, state.image_geometry.scale)
        //                 .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }

        //         BoundingBoxPart::CornerLowerRight => {
        //             draw.circle(radius)
        //                 .stroke(line_width)
        //                 .color(color)
        //                 .blend_mode(BlendMode::NORMAL)
        //                 .translate(bbox.lr_corner().x, bbox.lr_corner().y)
        //                 .scale(state.image_geometry.scale, state.image_geometry.scale)
        //                 .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }

        //         BoundingBoxPart::EdgeLeft => {
        //             draw.line(
        //                 vector_to_tuple(bbox.tl_corner()),
        //                 vector_to_tuple(bbox.ll_corner()),
        //             )
        //             .width(line_width)
        //             .color(color_edge)
        //             .scale(state.image_geometry.scale, state.image_geometry.scale)
        //             .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }

        //         BoundingBoxPart::EdgeRight => {
        //             draw.line(
        //                 vector_to_tuple(bbox.tr_corner()),
        //                 vector_to_tuple(bbox.lr_corner()),
        //             )
        //             .width(line_width)
        //             .color(color_edge)
        //             .scale(state.image_geometry.scale, state.image_geometry.scale)
        //             .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }
        //         BoundingBoxPart::EdgeTop => {
        //             draw.line(
        //                 vector_to_tuple(bbox.tl_corner()),
        //                 vector_to_tuple(bbox.tr_corner()),
        //             )
        //             .width(line_width)
        //             .color(color_edge)
        //             .scale(state.image_geometry.scale, state.image_geometry.scale)
        //             .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }
        //         BoundingBoxPart::EdgeBottom => {
        //             draw.line(
        //                 vector_to_tuple(bbox.ll_corner()),
        //                 vector_to_tuple(bbox.lr_corner()),
        //             )
        //             .width(line_width)
        //             .color(color_edge)
        //             .scale(state.image_geometry.scale, state.image_geometry.scale)
        //             .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        //         }

        //         BoundingBoxPart::CentralArea => {}
        //     }
        // }

        if state.persistent_settings.show_minimap {
            // let offset_x = app.window().size().0 as f32 - state.image_dimension.0 as f32;
            let offset_x = 0.0;

            let scale = 200. / app.window().size().0 as f32;
            let show_minimap = state.image_dimension.0 as f32 * state.image_geometry.scale
                > app.window().size().0 as f32;

            if show_minimap {
                draw.image(texture)
                    .blend_mode(BlendMode::NORMAL)
                    .translate(offset_x, 100.)
                    .scale(scale, scale);
            }
        }
    }

    let egui_output = plugins.egui(|ctx| {
        // the top menu bar

        // Default cursor
        ctx.set_cursor_icon(CursorIcon::Default);

        // Special cursors
        if state.cursor_within_image {
            if let Some(element_under_cursor) = state.current_bounding_box_element_under_cursor {
                match element_under_cursor.part {
                    BoundingBoxPart::EdgeBottom | BoundingBoxPart::EdgeTop => {
                        ctx.set_cursor_icon(CursorIcon::ResizeVertical)
                    }
                    BoundingBoxPart::EdgeLeft | BoundingBoxPart::EdgeRight => {
                        ctx.set_cursor_icon(CursorIcon::ResizeHorizontal)
                    }
                    BoundingBoxPart::CornerUpperLeft | BoundingBoxPart::CornerLowerRight => {
                        ctx.set_cursor_icon(CursorIcon::ResizeNwSe)
                    }
                    BoundingBoxPart::CornerLowerLeft | BoundingBoxPart::CornerUpperRight => {
                        ctx.set_cursor_icon(CursorIcon::ResizeNeSw)
                    }
                    _ => ctx.set_cursor_icon(CursorIcon::PointingHand),
                }
            }

            match state.bbox_edit_mode {
                BoundingBoxEditMode::DragCorner { part, .. } => match part {
                    BoundingBoxPart::CornerLowerLeft | BoundingBoxPart::CornerUpperRight => {
                        ctx.set_cursor_icon(CursorIcon::ResizeNeSw)
                    }
                    BoundingBoxPart::CornerUpperLeft | BoundingBoxPart::CornerLowerRight => {
                        ctx.set_cursor_icon(CursorIcon::ResizeNwSe)
                    }
                    _ => {}
                },
                BoundingBoxEditMode::DragEdge { part, .. } => match part {
                    BoundingBoxPart::EdgeBottom | BoundingBoxPart::EdgeTop => {
                        ctx.set_cursor_icon(CursorIcon::ResizeVertical)
                    }
                    BoundingBoxPart::EdgeLeft | BoundingBoxPart::EdgeRight => {
                        ctx.set_cursor_icon(CursorIcon::ResizeHorizontal)
                    }
                    _ => {}
                },
                BoundingBoxEditMode::DragFullBox { .. } => {
                    ctx.set_cursor_icon(CursorIcon::Move);
                }
                _ => {}
            }
        }

        if !state.persistent_settings.zen_mode {
            egui::TopBottomPanel::top("menu")
                .min_height(30.)
                .default_height(30.)
                .show(ctx, |ui| {
                    main_menu(ui, state, app, gfx);
                });
        }

        if state.persistent_settings.show_scrub_bar {
            egui::TopBottomPanel::bottom("scrubber")
                .max_height(22.)
                .min_height(22.)
                .show(ctx, |ui| {
                    scrubber_ui(state, ui);
                });
        }

        if let Some(message) = &state.message.clone() {
            // debug!("Message is set, showing");
            egui::TopBottomPanel::bottom("message").show_animated(
                ctx,
                state.message.is_some(),
                |ui| {
                    ui.horizontal(|ui| {
                        match message {
                            Message::Info(txt) => {
                                ui.label(format!("💬 {txt}"));
                            }
                            Message::Warning(txt) => {
                                ui.colored_label(Color32::GOLD, format!("⚠ {txt}"));
                            }
                            Message::Error(txt) | Message::LoadError(txt) => {
                                ui.colored_label(Color32::RED, format!("🕱 {txt}"));
                            }
                            Message::Saved(path) => {
                                ui.colored_label(Color32::RED, format!("Saved!"));
                                state.current_path = Some(path.clone());
                                set_title(app, state);
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            if ui.small_button("🗙").clicked() {
                                state.message = None
                            }
                        });
                    });

                    ui.ctx().request_repaint();
                },
            );
            let max_anim_len = 2.5;

            state.toast_cooldown += app.timer.delta_f32();

            if state.toast_cooldown > max_anim_len {
                debug!("Setting message to none, timer reached.");
                state.message = None;
            }
        }

        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
        {
            info_ui(ctx, state, gfx);
        }

        if !state.is_loaded {
            egui::TopBottomPanel::bottom("loader").show_animated(
                ctx,
                state.current_path.is_some(),
                |ui| {
                    if let Some(p) = &state.current_path {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::default());
                            ui.label(format!("Loading {}", p.display()));
                        });
                    }
                    app.window().request_frame();
                },
            );
        }

        state.pointer_over_ui = ctx.is_pointer_over_area();
        // info!("using pointer {}", ctx.is_using_pointer());

        // if there is interaction on the ui (dragging etc)
        // we don't want zoom & pan to work, so we "grab" the pointer
        if ctx.is_using_pointer() || ctx.is_pointer_over_area() {
            state.mouse_grab = true;
        } else {
            state.mouse_grab = false;
        }

        if ctx.wants_keyboard_input() {
            state.key_grab = true;
        } else {
            state.key_grab = false;
        }
        // Settings come last, as they block keyboard grab (for hotkey assigment)
        settings_ui(app, ctx, state);
    });

    if state.network_mode {
        app.window().request_frame();
    }
    let c = state.persistent_settings.background_color;
    // draw.clear(Color:: from_bytes(c[0], c[1], c[2], 255));
    draw.clear(Color::from_rgb(
        c[0] as f32 / 255.,
        c[1] as f32 / 255.,
        c[2] as f32 / 255.,
    ));
    gfx.render(&draw);
    gfx.render(&egui_output);
    if egui_output.needs_repaint() {
        app.window().request_frame();
    }
}

// Show file browser to select image to load
#[cfg(feature = "file_open")]
fn browse_for_image_path(state: &mut OculanteState) {
    let start_directory = state.persistent_settings.last_open_directory.clone();
    let load_sender = state.load_channel.0.clone();
    state.redraw = true;
    std::thread::spawn(move || {
        let file_dialog_result = rfd::FileDialog::new()
            .add_filter("All Supported Image Types", utils::SUPPORTED_EXTENSIONS)
            .add_filter("All File Types", &["*"])
            .set_directory(start_directory)
            .pick_file();
        if let Some(file_path) = file_dialog_result {
            let _ = load_sender.send(file_path);
        }
    });
}

// Make sure offset is restricted to window size so we don't offset to infinity
fn limit_offset(app: &mut App, state: &mut OculanteState) {
    let window_size = app.window().size();
    let scaled_image_size = (
        state.image_dimension.0 as f32 * state.image_geometry.scale,
        state.image_dimension.1 as f32 * state.image_geometry.scale,
    );
    state.image_geometry.offset.x = state
        .image_geometry
        .offset
        .x
        .min(window_size.0 as f32)
        .max(-scaled_image_size.0);
    state.image_geometry.offset.y = state
        .image_geometry
        .offset
        .y
        .min(window_size.1 as f32)
        .max(-scaled_image_size.1);
}

fn set_zoom(scale: f32, from_center: Option<Vector2<f32>>, state: &mut OculanteState) {
    let delta = scale - state.image_geometry.scale;
    let zoom_point = from_center.unwrap_or(state.cursor);
    state.image_geometry.offset -= scale_pt(
        state.image_geometry.offset,
        zoom_point,
        state.image_geometry.scale,
        delta,
    );
    state.image_geometry.scale = scale;
}
