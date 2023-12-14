#[cfg(feature = "file_open")]
use crate::browse_for_image_path;
use crate::{
    appstate::OculanteState,
    set_zoom,
    settings::{set_system_theme, ColorTheme},
    shortcuts::{key_pressed, keypresses_as_string, lookup},
    utils::{
        clipboard_copy, disp_col, disp_col_norm, load_image_from_path, next_image, prev_image,
        send_extended_info, set_title, solo_channel, toggle_fullscreen, unpremult, ColorChannel,
        ImageExt,
    },
};

const ICON_SIZE: f32 = 24.;

use egui_phosphor::regular::*;

use arboard::Clipboard;
use notan::{
    egui::{self, *},
    prelude::{App, Graphics},
};
use std::{collections::BTreeSet, ops::RangeInclusive};
const PANEL_WIDTH: f32 = 240.0;
const PANEL_WIDGET_OFFSET: f32 = 10.0;

#[cfg(feature = "turbo")]
pub trait EguiExt {
    fn label_i(&mut self, _text: &str) -> Response {
        unimplemented!()
    }

    fn label_i_selected(&mut self, _selected: bool, _text: &str) -> Response {
        unimplemented!()
    }

    fn slider_styled<Num: emath::Numeric>(
        &mut self,
        _value: &mut Num,
        _range: RangeInclusive<Num>,
    ) -> Response {
        unimplemented!()
    }

    fn slider_timeline<Num: emath::Numeric>(
        &mut self,
        _value: &mut Num,
        _range: RangeInclusive<Num>,
    ) -> Response {
        unimplemented!()
    }
}

impl EguiExt for Ui {
    /// Draw a justified icon from a string starting with an emoji
    fn label_i(&mut self, text: &str) -> Response {
        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();
        self.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            // self.horizontal(|ui| {
            ui.add_sized(
                egui::Vec2::new(28., ui.available_height()),
                egui::Label::new(RichText::new(icon).color(ui.style().visuals.selection.bg_fill)),
            );
            ui.label(
                RichText::new(description).color(ui.style().visuals.noninteractive().text_color()),
            );
        })
        .response
    }

    /// Draw a justified icon from a string starting with an emoji
    fn label_i_selected(&mut self, selected: bool, text: &str) -> Response {
        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();
        self.horizontal(|ui| {
            let mut r = ui.add_sized(
                egui::Vec2::new(30., ui.available_height()),
                egui::SelectableLabel::new(selected, RichText::new(icon)),
            );
            if ui
                .add_sized(
                    egui::Vec2::new(ui.available_width(), ui.available_height()),
                    egui::SelectableLabel::new(selected, RichText::new(description)),
                )
                .clicked()
            {
                r.clicked = [true, true, true, true, true];
            }
            r
        })
        .inner
    }

    fn slider_styled<Num: emath::Numeric>(
        &mut self,
        value: &mut Num,
        range: RangeInclusive<Num>,
    ) -> Response {
        self.scope(|ui| {
            let color = ui.style().visuals.selection.bg_fill;
            // let color = Color32::RED;
            let available_width = ui.available_width() * 0.6;
            let style = ui.style_mut();
            style.visuals.widgets.hovered.bg_fill = color;
            style.visuals.widgets.hovered.fg_stroke.width = 0.;

            style.visuals.widgets.active.bg_fill = color;
            style.visuals.widgets.active.fg_stroke.width = 0.;

            style.visuals.widgets.inactive.fg_stroke.width = 5.0;
            style.visuals.widgets.inactive.fg_stroke.color = color;
            style.visuals.widgets.inactive.rounding =
                style.visuals.widgets.inactive.rounding.at_least(20.);
            style.visuals.widgets.inactive.expansion = -5.0;

            style.spacing.slider_width = available_width;

            ui.horizontal(|ui| {
                let r = ui.add(Slider::new(value, range).show_value(false).integer());
                ui.monospace(format!("{:.0}", value.to_f64()));
                r
            })
            .inner
        })
        .inner
    }

    fn slider_timeline<Num: emath::Numeric>(
        &mut self,
        value: &mut Num,
        range: RangeInclusive<Num>,
    ) -> Response {
        self.scope(|ui| {
            let color = ui.style().visuals.selection.bg_fill;
            // let color = Color32::RED;
            let available_width = ui.available_width() * 1. - 60.;
            let style = ui.style_mut();
            style.visuals.widgets.hovered.bg_fill = color;
            style.visuals.widgets.hovered.fg_stroke.width = 0.;

            style.visuals.widgets.active.bg_fill = color;
            style.visuals.widgets.active.fg_stroke.width = 0.;

            style.visuals.widgets.inactive.fg_stroke.width = 5.0;
            style.visuals.widgets.inactive.fg_stroke.color = color;
            style.visuals.widgets.inactive.rounding =
                style.visuals.widgets.inactive.rounding.at_least(20.);
            style.visuals.widgets.inactive.expansion = -5.0;

            style.spacing.slider_width = available_width;

            ui.horizontal(|ui| {
                let r = ui.add(
                    Slider::new(value, range.clone())
                        .show_value(false)
                        .integer(),
                );
                ui.monospace(format!(
                    "{:.0}/{:.0}",
                    value.to_f64() + 1.,
                    range.end().to_f64() + 1.
                ));
                r
            })
            .inner
        })
        .inner
    }
}

pub fn info_ui(ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    if let Some(img) = &state.current_image {
        let img = img;

        if let Some(p) = img.get_pixel_checked(
            state.cursor_relative.x as u32,
            state.cursor_relative.y as u32,
        ) {
            state.sampled_color = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
        }
    }

    egui::SidePanel::left("side_panel")
        .max_width(PANEL_WIDTH)
        .min_width(PANEL_WIDTH / 2.)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if let Some(texture) = &state.current_texture {
                        // texture.
                        let tex_id = gfx.egui_register_texture(texture);

                        // width of image widget
                        // let desired_width = ui.available_width() - ui.spacing().indent;
                        let desired_width = PANEL_WIDTH - PANEL_WIDGET_OFFSET;

                        let scale = (desired_width / 8.) / texture.size().0;

                        let uv_center = (
                            state.cursor_relative.x / state.image_dimension.0 as f32,
                            (state.cursor_relative.y / state.image_dimension.1 as f32),
                        );

                        egui::Grid::new("info").show(ui, |ui| {
                            ui.label_i(&format!("{ARROWS_OUT} Size",));

                            ui.label(
                                RichText::new(format!(
                                    "{}x{}",
                                    state.image_dimension.0, state.image_dimension.1
                                ))
                                .monospace(),
                            );
                            ui.end_row();

                            if let Some(path) = &state.current_path {
                                // make sure we truncate filenames
                                let max_chars = 20;
                                let file_name =
                                    path.file_name().unwrap_or_default().to_string_lossy();
                                let skip_symbol = if file_name.chars().count() > max_chars {
                                    ".."
                                } else {
                                    ""
                                };

                                ui.label_i(&format!("{} File", IMAGE_SQUARE));
                                ui.label(
                                    RichText::new(format!(
                                        "{skip_symbol}{}",
                                        file_name
                                            .chars()
                                            .rev()
                                            .take(max_chars)
                                            .collect::<String>()
                                            .chars()
                                            .rev()
                                            .collect::<String>()
                                    )), // .monospace(),
                                )
                                .on_hover_text(format!("{}", path.display()));
                                ui.end_row();
                            }

                            ui.label_i(&format!("{PALETTE} RGBA"));
                            ui.label(
                                RichText::new(disp_col(state.sampled_color))
                                    .monospace()
                                    .background_color(Color32::from_rgba_unmultiplied(
                                        255, 255, 255, 6,
                                    )),
                            );
                            ui.end_row();

                            ui.label_i(&format!("{PALETTE} RGBA"));
                            ui.label(
                                RichText::new(disp_col_norm(state.sampled_color, 255.))
                                    .monospace()
                                    .background_color(Color32::from_rgba_unmultiplied(
                                        255, 255, 255, 6,
                                    )),
                            );
                            ui.end_row();

                            ui.label_i("⊞ Pos");
                            ui.label(
                                RichText::new(format!(
                                    "{:.0},{:.0}",
                                    state.cursor_relative.x, state.cursor_relative.y
                                ))
                                .monospace()
                                .background_color(
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 6),
                                ),
                            );
                            ui.end_row();

                            ui.label_i(" UV");
                            ui.label(
                                RichText::new(format!(
                                    "{:.3},{:.3}",
                                    uv_center.0,
                                    1.0 - uv_center.1
                                ))
                                .monospace()
                                .background_color(
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 6),
                                ),
                            );
                            ui.end_row();
                        });

                        // make sure aspect ratio is compensated for the square preview
                        let ratio = texture.size().0 / texture.size().1;
                        let uv_size = (scale, scale * ratio);

                        let preview_rect = ui
                            .add(
                                egui::Image::new(tex_id)
                                    .maintain_aspect_ratio(false)
                                    .fit_to_exact_size(egui::Vec2::splat(desired_width))
                                    .uv(egui::Rect::from_x_y_ranges(
                                        uv_center.0 - uv_size.0..=uv_center.0 + uv_size.0,
                                        uv_center.1 - uv_size.1..=uv_center.1 + uv_size.1,
                                    )),
                            )
                            .rect;

                        // let stroke_color = Color32::from_white_alpha(240);
                        let stroke_color = Color32::RED;
                        let bg_color = Color32::BLACK.linear_multiply(0.25);
                        ui.painter_at(preview_rect).line_segment(
                            [preview_rect.center_bottom(), preview_rect.center_top()],
                            Stroke::new(4., bg_color),
                        );
                        ui.painter_at(preview_rect).line_segment(
                            [preview_rect.left_center(), preview_rect.right_center()],
                            Stroke::new(4., bg_color),
                        );
                        ui.painter_at(preview_rect).line_segment(
                            [preview_rect.center_bottom(), preview_rect.center_top()],
                            Stroke::new(1., stroke_color),
                        );
                        ui.painter_at(preview_rect).line_segment(
                            [preview_rect.left_center(), preview_rect.right_center()],
                            Stroke::new(1., stroke_color),
                        );
                    }
                });

            ui.separator();
            ui.separator();
            ui.label(format!(
                "{:?}",
                state.current_bounding_box_element_under_cursor
            ));
            ui.separator();
            ui.label(format!("{:?}", state.bbox_edit_mode));
            ui.separator();
            ui.label(format!("{:?}", state.annotation_bboxes));
            ui.separator();
        });
}

pub fn settings_ui(app: &mut App, ctx: &Context, state: &mut OculanteState) {
    let mut settings_enabled = state.settings_enabled;
    egui::Window::new("Preferences")
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .open(&mut settings_enabled)
            .resizable(true)
            .default_width(600.)
            .show(ctx, |ui| {

                #[cfg(debug_assertions)]
                if ui.button("send test msg").clicked() {
                    state.send_message("Test");
                }

                egui::ComboBox::from_label("Color theme")
                .selected_text(format!("{:?}", state.persistent_settings.theme))
                .show_ui(ui, |ui| {
                    let mut r = ui.selectable_value(&mut state.persistent_settings.theme, ColorTheme::Dark, "Dark");
                    if ui.selectable_value(&mut state.persistent_settings.theme, ColorTheme::Light, "Light").changed() {
                        r.mark_changed();
                    }
                    if ui.selectable_value(&mut state.persistent_settings.theme, ColorTheme::System, "Same as system").clicked() {
                        r.mark_changed();

                    }

                    if r.changed() {
                        match state.persistent_settings.theme {
                            ColorTheme::Light =>
                                ctx.set_visuals(Visuals::light()),
                            ColorTheme::Dark =>
                                ctx.set_visuals(Visuals::dark()),
                            ColorTheme::System =>
                                set_system_theme(ctx),
                        }
                    }
                }
                );




                egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .color_edit_button_srgb(&mut state.persistent_settings.accent_color)
                            .changed()
                        {
                            let mut style: egui::Style = (*ctx.style()).clone();
                            style.visuals.selection.bg_fill = Color32::from_rgb(
                                state.persistent_settings.accent_color[0],
                                state.persistent_settings.accent_color[1],
                                state.persistent_settings.accent_color[2],
                            );
                            ctx.set_style(style);
                        }
                        ui.label("Accent color");
                    });

                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut state.persistent_settings.background_color);
                        ui.label("Background color");
                    });

                    ui.end_row();

                    ui
                    .checkbox(&mut state.persistent_settings.vsync, "Enable vsync")
                    .on_hover_text(
                        "Vsync reduces tearing and saves CPU. Toggling it off will make some operations such as panning/zooming more snappy. This needs a restart to take effect.",
                    );
                ui
                .checkbox(&mut state.persistent_settings.show_scrub_bar, "Show index slider")
                .on_hover_text(
                    "Enable an index slider to quickly scrub through lots of images",
                );
                    ui.end_row();

                    if ui
                    .checkbox(&mut state.persistent_settings.wrap_folder, "Wrap images at folder boundary")
                    .on_hover_text(
                        "When you move past the first or last image in a folder, should oculante continue or stop?",
                    )
                    .changed()
                {
                    state.scrubber.wrap = state.persistent_settings.wrap_folder;
                }
                ui.horizontal(|ui| {
                    ui.label("Number of image to cache");
                    if ui
                    .add(egui::DragValue::new(&mut state.persistent_settings.max_cache).clamp_range(0..=10000))

                    .on_hover_text(
                        "Keep this many images in memory for faster opening.",
                    )
                    .changed()
                {
                    state.player.cache.cache_size = state.persistent_settings.max_cache;
                    state.player.cache.clear();
                }
                });

                ui.end_row();
                ui
                    .checkbox(&mut state.persistent_settings.keep_view, "Do not reset image view")
                    .on_hover_text(
                        "When a new image is loaded, keep current zoom and offset",
                    );

                ui
                    .checkbox(&mut state.persistent_settings.keep_edits, "Keep image edits")
                    .on_hover_text(
                        "When a new image is loaded, keep current edits",
                    );
                ui.end_row();
                ui
                    .checkbox(&mut state.persistent_settings.show_checker_background, "Show checker background")
                    .on_hover_text(
                        "Show a checker pattern as backdrop.",
                    );

                ui
                    .checkbox(&mut state.persistent_settings.show_frame, "Draw frame around image")
                    .on_hover_text(
                        "Draw a small frame around the image. It is centered on the outmost pixel. This can be helpful on images with lots of transparency.",
                    );
                    ui.end_row();
                if ui.checkbox(&mut state.persistent_settings.zen_mode, "Turn on Zen mode").on_hover_text("Zen mode hides all UI and fits the image to the frame.").changed(){
                    set_title(app, state);
                }


                }


            );

                ui.horizontal(|ui| {
                    ui.label("Configure window title");
                    if ui
                    .text_edit_singleline(&mut state.persistent_settings.title_format)
                    .on_hover_text(
                        "Configure the title. Use {APP}, {VERSION}, {FULLPATH}, {FILENAME} and {RES} as placeholders.",
                    )
                    .changed()
                    {
                        set_title(app, state);
                    }
                });

                if ui.link("Visit github repo").on_hover_text("Check out the source code, request a feature, submit a bug or leave a star if you like it!").clicked() {
                    _ = webbrowser::open("https://github.com/woelper/oculante");
                }


                ui.vertical_centered_justified(|ui| {

                    #[cfg(feature = "update")]
                    if ui.button("Check for updates").on_hover_text("Check and install update if available. You will need to restart the app to use the new version.").clicked() {
                        state.send_message("Checking for updates...");
                        crate::update::update(Some(state.message_channel.0.clone()));
                        state.settings_enabled = false;
                    }

                    if ui.button("Reset all settings").clicked() {
                        state.persistent_settings = Default::default();
                    }
                });

                ui.collapsing("Keybindings",|ui| {
                    keybinding_ui(app, state, ui);
                });

            });
    state.settings_enabled = settings_enabled;
}

// TODO redo as impl UI
pub fn tooltip(r: Response, tooltip: &str, hotkey: &str, _ui: &mut Ui) -> Response {
    r.on_hover_ui(|ui| {
        let avg = (ui.style().visuals.selection.bg_fill.r() as i32
            + ui.style().visuals.selection.bg_fill.g() as i32
            + ui.style().visuals.selection.bg_fill.b() as i32)
            / 3;
        let contrast_color: u8 = if avg > 128 { 0 } else { 255 };
        ui.horizontal(|ui| {
            ui.label(tooltip);
            ui.label(
                RichText::new(hotkey)
                    .monospace()
                    .color(Color32::from_gray(contrast_color))
                    .background_color(ui.style().visuals.selection.bg_fill),
            );
        });
    })
}

// TODO redo as impl UI
pub fn unframed_button(text: impl Into<String>, ui: &mut Ui) -> Response {
    ui.add(egui::Button::new(RichText::new(text).size(ICON_SIZE)).frame(false))
}

pub fn unframed_button_colored(text: impl Into<String>, is_colored: bool, ui: &mut Ui) -> Response {
    if is_colored {
        ui.add(
            egui::Button::new(
                RichText::new(text)
                    .size(ICON_SIZE)
                    // .heading()
                    .color(ui.style().visuals.selection.bg_fill),
            )
            .frame(false),
        )
    } else {
        ui.add(
            egui::Button::new(
                RichText::new(text).size(ICON_SIZE), // .heading()
            )
            .frame(false),
        )
    }
}

pub fn scrubber_ui(state: &mut OculanteState, ui: &mut Ui) {
    let len = state.scrubber.len().saturating_sub(1);

    if ui
        .slider_timeline(&mut state.scrubber.index, 0..=len)
        .changed()
    {
        let p = state.scrubber.set(state.scrubber.index);
        state.current_path = Some(p.clone());
        state.player.load(&p, state.message_channel.0.clone());
    }
}

fn keybinding_ui(app: &mut App, state: &mut OculanteState, ui: &mut Ui) {
    // Make sure no shortcuts are received by the application
    state.key_grab = true;

    let no_keys_pressed = app.keyboard.down.is_empty();

    ui.horizontal(|ui| {
        ui.label("While this is open, regular shortcuts will not work.");
        if no_keys_pressed {
            ui.label(egui::RichText::new("Please press & hold a key").color(Color32::RED));
        }
    });

    let k = app
        .keyboard
        .down
        .iter()
        .map(|k| format!("{:?}", k.0))
        .collect::<BTreeSet<String>>();

    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let s = state.persistent_settings.shortcuts.clone();
            let mut ordered_shortcuts = state
                .persistent_settings
                .shortcuts
                .iter_mut()
                .collect::<Vec<_>>();
            ordered_shortcuts
                .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            egui::Grid::new("info").num_columns(2).show(ui, |ui| {
                for (event, keys) in ordered_shortcuts {
                    ui.label(format!("{event:?}"));

                    ui.label(lookup(&s, event));
                    if !no_keys_pressed {
                        if ui
                            .button(format!("Assign {}", keypresses_as_string(&k)))
                            .clicked()
                        {
                            *keys = app
                                .keyboard
                                .down
                                .iter()
                                .map(|(k, _)| format!("{k:?}"))
                                .collect();
                        }
                    } else {
                        ui.add_enabled(false, egui::Button::new("Press key(s)..."));
                    }
                    ui.end_row();
                }
            });
        });
}

// fn keystrokes(ui: &mut Ui) {
//     ui.add(Button::new(format!("{:?}", k.0)).fill(Color32::DARK_BLUE));
// }

pub fn main_menu(ui: &mut Ui, state: &mut OculanteState, app: &mut App, gfx: &mut Graphics) {
    ui.horizontal_centered(|ui| {
        use crate::shortcuts::InputEvent::*;

        // ui.label("Channels");

        #[cfg(feature = "file_open")]
        if unframed_button(FOLDER, ui)
            .on_hover_text("Browse for image")
            .clicked()
        {
            browse_for_image_path(state)
        }

        let mut changed_channels = false;

        if key_pressed(app, state, RedChannel) {
            state.persistent_settings.current_channel = ColorChannel::Red;
            changed_channels = true;
        }
        if key_pressed(app, state, GreenChannel) {
            state.persistent_settings.current_channel = ColorChannel::Green;
            changed_channels = true;
        }
        if key_pressed(app, state, BlueChannel) {
            state.persistent_settings.current_channel = ColorChannel::Blue;
            changed_channels = true;
        }
        if key_pressed(app, state, AlphaChannel) {
            state.persistent_settings.current_channel = ColorChannel::Alpha;
            changed_channels = true;
        }

        if key_pressed(app, state, RGBChannel) {
            state.persistent_settings.current_channel = ColorChannel::Rgb;
            changed_channels = true;
        }
        if key_pressed(app, state, RGBAChannel) {
            state.persistent_settings.current_channel = ColorChannel::Rgba;
            changed_channels = true;
        }

        // TODO: remove redundancy
        if changed_channels {
            if let Some(img) = &state.current_image {
                match &state.persistent_settings.current_channel {
                    ColorChannel::Rgb => state.current_texture = unpremult(img).to_texture(gfx),
                    ColorChannel::Rgba => state.current_texture = img.to_texture(gfx),
                    _ => {
                        state.current_texture =
                            solo_channel(img, state.persistent_settings.current_channel as usize)
                                .to_texture(gfx)
                    }
                }
            }
        }

        if state.current_path.is_some() {
            if tooltip(
                unframed_button(CARET_LEFT, ui),
                "Previous image",
                &lookup(&state.persistent_settings.shortcuts, &PreviousImage),
                ui,
            )
            .clicked()
            {
                prev_image(state)
            }
            if tooltip(
                unframed_button(CARET_RIGHT, ui),
                "Next image",
                &lookup(&state.persistent_settings.shortcuts, &NextImage),
                ui,
            )
            .clicked()
            {
                next_image(state)
            }
        }

        if state.current_image.is_some() {
            if tooltip(
                // ui.checkbox(&mut state.info_enabled, "ℹ Info"),
                ui.selectable_label(
                    state.persistent_settings.info_enabled,
                    RichText::new(format!("{}", INFO)).size(ICON_SIZE * 0.8),
                ),
                "Show image info",
                &lookup(&state.persistent_settings.shortcuts, &InfoMode),
                ui,
            )
            .clicked()
            {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
                send_extended_info(
                    &state.current_image,
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }
        }

        // FIXME This crashes/freezes!
        // if tooltip(
        //     unframed_button("⛶", ui),
        //     "Full Screen",
        //     &lookup(&state.persistent_settings.shortcuts, &Fullscreen),
        //     ui,
        // )
        // .clicked()
        // {
        //     toggle_fullscreen(app, state);
        // }

        if tooltip(
            unframed_button(ARROWS_OUT_SIMPLE, ui),
            "Toggle fullscreen",
            &lookup(&state.persistent_settings.shortcuts, &Fullscreen),
            ui,
        )
        .clicked()
        {
            toggle_fullscreen(app, state);
        }

        if tooltip(
            unframed_button_colored(ARROW_LINE_UP, state.always_on_top, ui),
            "Always on top",
            &lookup(&state.persistent_settings.shortcuts, &AlwaysOnTop),
            ui,
        )
        .clicked()
        {
            state.always_on_top = !state.always_on_top;
            app.window().set_always_on_top(state.always_on_top);
        }

        if let Some(p) = &state.current_path {
            if tooltip(
                unframed_button(TRASH, ui),
                "Remove the selected annation",
                &lookup(&state.persistent_settings.shortcuts, &DeleteAnnoation),
                ui,
            )
            .clicked()
            {
                if let Some(id) = state.selected_bbox_id {
                    state.annotation_bboxes.remove(id);
                    state.selected_bbox_id = None;
                    state.send_message("Deleted annotation");
                }
            }
        }

        ui.add_space(ui.available_width() - 32.);

        ui.scope(|ui| {
            // ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);
            // maybe override font size?
            ui.style_mut().visuals.button_frame = false;
            ui.style_mut().visuals.widgets.inactive.expansion = 20.;

            // FIXME: Needs submenu not to be out of bounds
            // ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {

            ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);

            ui.menu_button(RichText::new(LIST).size(ICON_SIZE), |ui| {
                if ui.button("Reset view").clicked() {
                    state.reset_image = true;
                    ui.close_menu();
                }
                if ui.button("View 1:1").clicked() {
                    set_zoom(
                        1.0,
                        Some(nalgebra::Vector2::new(
                            app.window().width() as f32 / 2.,
                            app.window().height() as f32 / 2.,
                        )),
                        state,
                    );
                    ui.close_menu();
                }

                let copy_pressed = key_pressed(app, state, Copy);
                if let Some(img) = &state.current_image {
                    if ui
                        .button("🗐 Copy")
                        .on_hover_text("Copy image to clipboard")
                        .clicked()
                        || copy_pressed
                    {
                        clipboard_copy(img);
                        ui.close_menu();
                    }
                }

                if ui
                    .button("📋 Paste")
                    .on_hover_text("Paste image from clipboard")
                    .clicked()
                    || key_pressed(app, state, Paste)
                {
                    if let Ok(clipboard) = &mut Clipboard::new() {
                        if let Ok(imagedata) = clipboard.get_image() {
                            if let Some(image) = image::RgbaImage::from_raw(
                                imagedata.width as u32,
                                imagedata.height as u32,
                                (imagedata.bytes).to_vec(),
                            ) {
                                state.current_path = None;
                                // Stop in the even that an animation is running
                                state.player.stop();
                                _ = state
                                    .player
                                    .image_sender
                                    .send(crate::utils::Frame::new_still(image));
                                // Since pasted data has no path, make sure it's not set
                                state.send_message("Image pasted");
                            }
                        } else {
                            state.send_message_err("Clipboard did not contain image")
                        }
                    }
                    ui.close_menu();
                }

                if ui.button("⛭ Preferences").clicked() {
                    state.settings_enabled = !state.settings_enabled;
                    ui.close_menu();
                }

                ui.menu_button("Recent", |ui| {
                    for r in &state.persistent_settings.recent_images.clone() {
                        if let Some(filename) = r.file_name() {
                            if ui.button(filename.to_string_lossy()).clicked() {
                                load_image_from_path(r, state);
                                ui.close_menu();
                            }
                        }
                    }
                });

                // TODO: expose favourites with a tool button
                // ui.menu_button("Favourites", |ui| {
                //     for r in &state.persistent_settings.favourite_images.clone() {
                //         if let Some(filename) = r.file_name() {
                //             if ui.button(filename.to_string_lossy()).clicked() {
                //ui.close_menu();

                //             }
                //         }
                //     }

                // });
            });

            // });
        });
    });
}
