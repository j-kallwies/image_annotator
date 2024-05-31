use crate::{
    scrubber::Scrubber,
    settings::PersistentSettings,
    utils::{ExtendedImageInfo, Frame, Player},
};
use image::RgbaImage;
use nalgebra::Vector2;
use notan::graphics::color::Color;
use notan::{egui::epaint::ahash::HashMap, prelude::Texture, AppState};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

#[derive(Debug, Clone)]
pub struct ImageGeometry {
    /// The scale of the displayed image
    pub scale: f32,
    /// Image offset on canvas
    pub offset: Vector2<f32>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Info(String),
    Warning(String),
    Error(String),
    LoadError(String),
    Saved(PathBuf),
}

impl Message {
    pub fn info(m: &str) -> Self {
        Self::Info(m.into())
    }
    pub fn warn(m: &str) -> Self {
        Self::Warning(m.into())
    }
    pub fn err(m: &str) -> Self {
        Self::Error(m.into())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AnnoationBoundingBox {
    x_center: f32,
    y_center: f32,
    width: f32,
    height: f32,
    pub class_id: u32,
}

impl Default for AnnoationBoundingBox {
    fn default() -> AnnoationBoundingBox {
        AnnoationBoundingBox {
            x_center: f32::NAN,
            y_center: f32::NAN,
            width: f32::NAN,
            height: f32::NAN,
            class_id: 0,
        }
    }
}

impl AnnoationBoundingBox {
    pub fn from_center(
        x_center: f32,
        y_center: f32,
        width: f32,
        height: f32,
        class_id: u32,
    ) -> AnnoationBoundingBox {
        AnnoationBoundingBox {
            x_center: x_center,
            y_center: y_center,
            width: width,
            height: height,
            class_id: class_id,
        }
    }

    pub fn x_min(self) -> f32 {
        self.x_center - self.width / 2.0
    }

    pub fn x_max(self) -> f32 {
        self.x_center + self.width / 2.0
    }

    pub fn y_min(self) -> f32 {
        self.y_center - self.height / 2.0
    }

    pub fn y_max(self) -> f32 {
        self.y_center + self.height / 2.0
    }

    pub fn tl_corner(self) -> Vector2<f32> {
        nalgebra::Vector2::new(
            self.x_center - self.width / 2.0,
            self.y_center - self.height / 2.0,
        )
    }

    pub fn tr_corner(self) -> Vector2<f32> {
        nalgebra::Vector2::new(
            self.x_center + self.width / 2.0,
            self.y_center - self.height / 2.0,
        )
    }

    pub fn br_corner(self) -> Vector2<f32> {
        nalgebra::Vector2::new(
            self.x_center + self.width / 2.0,
            self.y_center + self.height / 2.0,
        )
    }

    pub fn bl_corner(self) -> Vector2<f32> {
        nalgebra::Vector2::new(
            self.x_center - self.width / 2.0,
            self.y_center + self.height / 2.0,
        )
    }

    pub fn center(self) -> Vector2<f32> {
        nalgebra::Vector2::new(self.x_center, self.y_center)
    }

    pub fn width(self) -> f32 {
        self.width
    }

    pub fn height(self) -> f32 {
        self.height
    }

    pub fn size(self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn contains(self, p: (f32, f32)) -> bool {
        let dx = p.0 - self.x_center;
        let dy = p.1 - self.y_center;
        dx.abs() <= self.width / 2.0 && dy.abs() <= self.height / 2.0
    }

    pub fn to_yolo_label_str(self, image_width: u32, image_height: u32) -> String {
        format!(
            "{} {} {} {} {}",
            self.class_id,
            self.center().x / (image_width as f32),
            self.center().y / (image_height as f32),
            self.size().0 / (image_width as f32),
            self.size().1 / (image_height as f32),
        )
    }

    pub fn set_corner_points(self: &mut Self, p1: Vector2<f32>, p2: Vector2<f32>) {
        self.x_center = (p1.x + p2.x) / 2.0;
        self.y_center = (p1.y + p2.y) / 2.0;
        self.width = (p1.x - p2.x).abs();
        self.height = (p1.y - p2.y).abs();
    }

    pub fn set_center(self: &mut Self, p_center: Vector2<f32>) {
        self.x_center = p_center.x;
        self.y_center = p_center.y;
    }

    pub fn set_x_min(self: &mut Self, x_min: f32) {
        let x_max = self.x_max();
        self.width = (x_max - x_min).abs();
        self.x_center = (x_max + x_min) / 2.0;
    }

    pub fn set_x_max(self: &mut Self, x_max: f32) {
        let x_min = self.x_min();
        self.width = (x_max - x_min).abs();
        self.x_center = (x_max + x_min) / 2.0;
    }

    pub fn set_y_min(self: &mut Self, y_min: f32) {
        let y_max = self.y_max();
        self.height = (y_max - y_min).abs();
        self.y_center = (y_max + y_min) / 2.0;
    }

    pub fn set_y_max(self: &mut Self, y_max: f32) {
        let y_min = self.y_min();
        self.height = (y_max - y_min).abs();
        self.y_center = (y_max + y_min) / 2.0;
    }

    fn get_part(self: &Self, cursor_position: Vector2<f32>) -> Option<BoundingBoxPart> {
        let catch_radius = 20.0;

        let mut current_best_dist = f32::INFINITY;
        let mut current_best_part = None;

        if cursor_position.x >= self.x_min()
            && cursor_position.y >= self.y_min()
            && cursor_position.x <= self.x_max()
            && cursor_position.y <= self.y_max()
        {
            current_best_part = Some(BoundingBoxPart::CentralArea);
            current_best_dist = 0.0;
        }
        
        let dist = (self.tl_corner() - cursor_position).norm();
        if dist < catch_radius && dist < current_best_dist {
            current_best_part = Some(BoundingBoxPart::CornerUpperLeft);
            current_best_dist = dist;
        }
        
        let dist = (self.tr_corner() - cursor_position).norm();
        if dist < catch_radius && dist < current_best_dist {
            current_best_part = Some(BoundingBoxPart::CornerUpperRight);
            current_best_dist = dist;
        }
        
        let dist = (self.bl_corner() - cursor_position).norm();
        if dist < catch_radius && dist < current_best_dist {
            current_best_part = Some(BoundingBoxPart::CornerLowerLeft);
            current_best_dist = dist;
        } 
        
        let dist = (self.br_corner() - cursor_position).norm();
        if dist < catch_radius && dist < current_best_dist {
            current_best_part = Some(BoundingBoxPart::CornerLowerRight);
            current_best_dist = dist;
        }
        
        let dist = (((self.tl_corner() + self.bl_corner())/2.) - cursor_position).norm();
        if (self.x_min() - cursor_position.x).abs() < catch_radius / 2.
            && cursor_position.y >= self.y_min()
            && cursor_position.y <= self.y_max()
            && dist < current_best_dist
        {
            current_best_part = Some(BoundingBoxPart::EdgeLeft);
            current_best_dist = dist;
        }
        
        let dist = (((self.tr_corner() + self.br_corner())/2.) - cursor_position).norm();
        if (self.x_max() - cursor_position.x).abs() < catch_radius / 2.
            && cursor_position.y >= self.y_min()
            && cursor_position.y <= self.y_max()
            && dist < current_best_dist
        {
            current_best_part = Some(BoundingBoxPart::EdgeRight);
            current_best_dist = dist;
        }
        
        let dist = (((self.tl_corner() + self.tr_corner())/2.) - cursor_position).norm();
        if (self.y_min() - cursor_position.y).abs() < catch_radius / 2.
            && cursor_position.x >= self.x_min()
            && cursor_position.x <= self.x_max()
            && dist < current_best_dist
        {
            current_best_part = Some(BoundingBoxPart::EdgeTop);
            current_best_dist = dist;
        }
        
        let dist = (((self.bl_corner() + self.br_corner())/2.) - cursor_position).norm();
        if (self.y_max() - cursor_position.y).abs() < catch_radius / 2.
            && cursor_position.x >= self.x_min()
            && cursor_position.x <= self.x_max()
            && dist < current_best_dist
        {
            current_best_part = Some(BoundingBoxPart::EdgeBottom);
            current_best_dist = dist;
        }

        return current_best_part;
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BoundingBoxPart {
    CentralArea,
    CornerUpperLeft,
    CornerUpperRight,
    CornerLowerRight,
    CornerLowerLeft,
    EdgeLeft,
    EdgeRight,
    EdgeTop,
    EdgeBottom,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundingBoxElement {
    pub id: usize,
    pub part: BoundingBoxPart,
}

#[derive(Clone, Copy, Debug)]
pub enum BoundingBoxEditMode {
    None,
    New {
        id: usize,
        start_point: Option<Vector2<f32>>,
    },
    DragCorner {
        id: usize,
        part: BoundingBoxPart,
        static_opposite_point: Vector2<f32>,
    },
    DragEdge {
        id: usize,
        part: BoundingBoxPart,
    },
    DragFullBox {
        id: usize,
        offset: Vector2<f32>,
    },
}

impl BoundingBoxEditMode {
    pub fn get_part_element(
        self: &Self,
        cursor_position: Vector2<f32>,
        annoation_bboxes: &Vec<AnnoationBoundingBox>,
    ) -> Option<BoundingBoxElement> {
        for (id, bbox) in annoation_bboxes.iter().enumerate() {
            if let Some(part) = bbox.get_part(cursor_position) {
                return Some(BoundingBoxElement { id: id, part: part });
            }
        }

        // Default
        None
    }

    // Button down => Start Action
    pub fn mouse_button_down(
        self: &mut Self,
        cursor_position: Vector2<f32>,
        annoation_bboxes: &mut Vec<AnnoationBoundingBox>,
        selected_bbox_id: &mut Option<usize>,
        label_class_id: u32,
    ) {
        *selected_bbox_id = None;
        match self {
            BoundingBoxEditMode::None => {
                if let Some(clicked_part_element) =
                    self.get_part_element(cursor_position, annoation_bboxes)
                {
                    match clicked_part_element.part {
                        BoundingBoxPart::CentralArea => {
                            *self = BoundingBoxEditMode::DragFullBox {
                                id: clicked_part_element.id,
                                offset: cursor_position
                                    - annoation_bboxes[clicked_part_element.id].center(),
                            };
                            *selected_bbox_id = Some(clicked_part_element.id);
                            return;
                        }
                        BoundingBoxPart::CornerUpperLeft => {
                            *self = BoundingBoxEditMode::DragCorner {
                                id: clicked_part_element.id,
                                part: clicked_part_element.part,
                                static_opposite_point: annoation_bboxes[clicked_part_element.id]
                                    .br_corner(),
                            };
                            return;
                        }
                        BoundingBoxPart::CornerUpperRight => {
                            *self = BoundingBoxEditMode::DragCorner {
                                id: clicked_part_element.id,
                                part: clicked_part_element.part,
                                static_opposite_point: annoation_bboxes[clicked_part_element.id]
                                    .bl_corner(),
                            };
                            return;
                        }
                        BoundingBoxPart::CornerLowerRight => {
                            *self = BoundingBoxEditMode::DragCorner {
                                id: clicked_part_element.id,
                                part: clicked_part_element.part,
                                static_opposite_point: annoation_bboxes[clicked_part_element.id]
                                    .tl_corner(),
                            };
                            return;
                        }
                        BoundingBoxPart::CornerLowerLeft => {
                            *self = BoundingBoxEditMode::DragCorner {
                                id: clicked_part_element.id,
                                part: clicked_part_element.part,
                                static_opposite_point: annoation_bboxes[clicked_part_element.id]
                                    .tr_corner(),
                            };
                            return;
                        }
                        BoundingBoxPart::EdgeLeft
                        | BoundingBoxPart::EdgeRight
                        | BoundingBoxPart::EdgeTop
                        | BoundingBoxPart::EdgeBottom => {
                            *self = BoundingBoxEditMode::DragEdge {
                                id: clicked_part_element.id,
                                part: clicked_part_element.part,
                            };
                            return;
                        }
                    }
                }

                // Create new BoundingBox
                annoation_bboxes.push(AnnoationBoundingBox::default());
                annoation_bboxes.last_mut().unwrap().class_id = label_class_id;
                *self = BoundingBoxEditMode::New {
                    id: annoation_bboxes.len() - 1,
                    start_point: Some(cursor_position),
                };
            }
            BoundingBoxEditMode::New { id, start_point } => {
                if let Some(bbox) = annoation_bboxes.get_mut(*id) {
                    if start_point.is_none() {
                        *start_point = Some(cursor_position);
                    } else {
                        bbox.set_corner_points(start_point.unwrap(), cursor_position);
                    }
                }
            }
            BoundingBoxEditMode::DragCorner { .. } => {}
            BoundingBoxEditMode::DragEdge { .. } => {}
            BoundingBoxEditMode::DragFullBox { .. } => {}
        }

        // if self.start_point {
        //     self.bbox.set(start_point, cursor_position);
        // }
    }

    // Button down => End Action
    pub fn mouse_button_up(
        self: &mut Self,
        _cursor_position: Vector2<f32>,
        annoation_bboxes: &mut Vec<AnnoationBoundingBox>,
    ) {
        match self {
            BoundingBoxEditMode::None => {}
            BoundingBoxEditMode::New { id, .. } => {
                if annoation_bboxes[*id].size().0 == 0.0 || annoation_bboxes[*id].size().1 == 0.0 {
                    annoation_bboxes.remove(*id);
                }

                *self = BoundingBoxEditMode::None;
            }
            BoundingBoxEditMode::DragCorner { .. } => {
                *self = BoundingBoxEditMode::None;
            }
            BoundingBoxEditMode::DragEdge { .. } => {
                *self = BoundingBoxEditMode::None;
            }
            BoundingBoxEditMode::DragFullBox { .. } => {
                *self = BoundingBoxEditMode::None;
            }
        }

        // if self.start_point {
        //     self.bbox.set(start_point, cursor_position);
        // }
    }

    pub fn update(
        self: &mut Self,
        cursor_position: Vector2<f32>,
        annoation_bboxes: &mut Vec<AnnoationBoundingBox>,
    ) {
        match self {
            BoundingBoxEditMode::None => {}
            BoundingBoxEditMode::New { id, start_point } => {
                if let Some(bbox) = annoation_bboxes.get_mut(*id) {
                    if start_point.is_some() {
                        bbox.set_corner_points(start_point.unwrap(), cursor_position);
                    }
                }
            }
            BoundingBoxEditMode::DragCorner {
                id,
                static_opposite_point,
                ..
            } => annoation_bboxes
                .get_mut(*id)
                .unwrap()
                .set_corner_points(*static_opposite_point, cursor_position),
            BoundingBoxEditMode::DragEdge { id, part } => match *part {
                BoundingBoxPart::EdgeLeft => {
                    annoation_bboxes
                        .get_mut(*id)
                        .unwrap()
                        .set_x_min(cursor_position.x);
                }
                BoundingBoxPart::EdgeRight => {
                    annoation_bboxes
                        .get_mut(*id)
                        .unwrap()
                        .set_x_max(cursor_position.x);
                }
                BoundingBoxPart::EdgeTop => {
                    annoation_bboxes
                        .get_mut(*id)
                        .unwrap()
                        .set_y_min(cursor_position.y);
                }
                BoundingBoxPart::EdgeBottom => {
                    annoation_bboxes
                        .get_mut(*id)
                        .unwrap()
                        .set_y_max(cursor_position.y);
                }
                _ => {}
            },
            BoundingBoxEditMode::DragFullBox { id, offset } => {
                let bbox = annoation_bboxes.get_mut(*id).unwrap();

                bbox.set_center(nalgebra::Vector2::new(
                    cursor_position.x - offset.x,
                    cursor_position.y - offset.y,
                ));
            }
        }
    }
}

/// The state of the application
#[derive(Debug, AppState)]
pub struct OculanteState {
    pub image_geometry: ImageGeometry,
    pub compare_list: HashMap<PathBuf, ImageGeometry>,
    pub drag_enabled: bool,
    pub reset_image: bool,
    pub message: Option<Message>,
    /// Is the image fully loaded?
    pub is_loaded: bool,
    pub window_size: Vector2<f32>,
    pub cursor: Vector2<f32>,
    pub cursor_relative: Vector2<f32>,
    pub cursor_within_image: bool,
    pub image_dimension: (u32, u32),
    pub sampled_color: [f32; 4],
    pub mouse_delta: Vector2<f32>,
    pub texture_channel: (Sender<Frame>, Receiver<Frame>),
    pub message_channel: (Sender<Message>, Receiver<Message>),
    /// Channel to load images from
    pub load_channel: (Sender<PathBuf>, Receiver<PathBuf>),
    pub extended_info_channel: (Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
    pub extended_info_loading: bool,
    /// The Player, responsible for loading and sending Frames
    pub player: Player,
    pub current_texture: Option<Texture>,
    pub current_path: Option<PathBuf>,
    pub current_image: Option<RgbaImage>,
    pub settings_enabled: bool,
    pub image_info: Option<ExtendedImageInfo>,
    pub mouse_grab: bool,
    pub key_grab: bool,
    pub pointer_over_ui: bool,
    /// Things that perisist between launches
    pub persistent_settings: PersistentSettings,
    pub always_on_top: bool,
    pub network_mode: bool,
    pub show_crosshair_lines: bool,
    /// how long the toast message appears
    pub toast_cooldown: f32,
    /// data to transform image once fullscreen is entered/left
    pub fullscreen_offset: Option<(i32, i32)>,
    /// List of images to cycle through. Usually the current dir or dropped files
    pub scrubber: Scrubber,
    pub checker_texture: Option<Texture>,
    pub redraw: bool,
    pub first_start: bool,

    // Image annotation stuff
    pub bbox_edit_mode: BoundingBoxEditMode,
    pub selected_bbox_id: Option<usize>,
    pub annotation_bboxes: Vec<AnnoationBoundingBox>,
    pub current_bounding_box_element_under_cursor: Option<BoundingBoxElement>,
    pub current_label_class: u32,
    pub label_colors: Vec<Color>,
}

impl OculanteState {
    pub fn send_message(&self, msg: &str) {
        _ = self.message_channel.0.send(Message::info(msg));
    }

    pub fn send_message_err(&self, msg: &str) {
        _ = self.message_channel.0.send(Message::err(msg));
    }
}

impl Default for OculanteState {
    fn default() -> OculanteState {
        let tx_channel = mpsc::channel();
        OculanteState {
            image_geometry: ImageGeometry {
                scale: 1.0,
                offset: Default::default(),
            },
            compare_list: Default::default(),
            drag_enabled: Default::default(),
            reset_image: Default::default(),
            message: Default::default(),
            is_loaded: Default::default(),
            cursor: Default::default(),
            cursor_relative: Default::default(),
            cursor_within_image: false,
            image_dimension: (0, 0),
            sampled_color: [0., 0., 0., 0.],
            player: Player::new(tx_channel.0.clone(), 20, 16384),
            texture_channel: tx_channel,
            message_channel: mpsc::channel(),
            load_channel: mpsc::channel(),
            extended_info_channel: mpsc::channel(),
            extended_info_loading: Default::default(),
            mouse_delta: Default::default(),
            current_texture: Default::default(),
            current_image: Default::default(),
            current_path: Default::default(),
            settings_enabled: Default::default(),
            image_info: Default::default(),
            mouse_grab: Default::default(),
            key_grab: Default::default(),
            pointer_over_ui: Default::default(),
            persistent_settings: Default::default(),
            always_on_top: Default::default(),
            show_crosshair_lines: true,
            network_mode: Default::default(),
            window_size: Default::default(),
            toast_cooldown: Default::default(),
            fullscreen_offset: Default::default(),
            scrubber: Default::default(),
            checker_texture: Default::default(),
            redraw: Default::default(),
            first_start: true,
            bbox_edit_mode: BoundingBoxEditMode::None,
            selected_bbox_id: None,
            annotation_bboxes: vec![],
            current_bounding_box_element_under_cursor: None,
            current_label_class: 0,
            label_colors: vec![
                Color {
                    r: 0.894,
                    g: 0.102,
                    b: 0.110,
                    a: 1.0,
                },
                Color {
                    r: 0.216,
                    g: 0.494,
                    b: 0.722,
                    a: 1.0,
                },
                Color {
                    r: 0.302,
                    g: 0.686,
                    b: 0.290,
                    a: 1.0,
                },
                Color {
                    r: 0.596,
                    g: 0.306,
                    b: 0.639,
                    a: 1.0,
                },
                Color {
                    r: 1.00,
                    g: 0.498,
                    b: 0.0,
                    a: 1.0,
                },
                Color {
                    r: 0.651,
                    g: 0.337,
                    b: 0.157,
                    a: 1.0,
                },
                Color {
                    r: 0.969,
                    g: 0.506,
                    b: 0.749,
                    a: 1.0,
                },
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                Color {
                    r: 0.40,
                    g: 0.761,
                    b: 0.647,
                    a: 1.0,
                },
            ],
        }
    }
}
