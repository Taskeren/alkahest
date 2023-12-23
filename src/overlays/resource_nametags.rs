use crate::{
    camera::FpsCamera,
    ecs::{
        components::{Label, ResourceOriginType, ResourcePoint},
        transform::Transform,
    },
    map::MapDataList,
    map_resources::MapResource,
    render::debug::DebugShapes,
    resources::Resources,
};

use egui::{Color32, Rect};
use glam::Vec2;
use std::{cell::RefCell, rc::Rc};
use winit::window::Window;

use super::{camera_settings::CameraPositionOverlay, gui::Overlay};

pub struct ResourceTypeOverlay {
    pub debug_overlay: Rc<RefCell<CameraPositionOverlay>>,
}

impl Overlay for ResourceTypeOverlay {
    fn draw(
        &mut self,
        ctx: &egui::Context,
        _window: &Window,
        resources: &mut Resources,
        gui: super::gui::GuiContext<'_>,
    ) -> bool {
        if self.debug_overlay.borrow().show_map_resources {
            let screen_size = ctx.screen_rect().size();

            let painter = ctx.layer_painter(egui::LayerId::background());

            let camera = resources.get::<FpsCamera>().unwrap();

            let maps = resources.get::<MapDataList>().unwrap();
            if let Some((_, _, m)) = maps.current_map() {
                struct StrippedResourcePoint {
                    resource: MapResource,
                    has_havok_data: bool,
                    origin: ResourceOriginType,
                    label: Option<String>,
                }

                let mut rp_list = vec![];

                for (_, (transform, res, label)) in m
                    .scene
                    .query::<(&Transform, &ResourcePoint, Option<&Label>)>()
                    .iter()
                {
                    if !self.debug_overlay.borrow().map_resource_filter[res.resource.index()] {
                        continue;
                    }

                    if res.origin == ResourceOriginType::Map
                        && !self.debug_overlay.borrow().map_resource_show_map
                    {
                        continue;
                    }

                    if matches!(
                        res.origin,
                        ResourceOriginType::Activity | ResourceOriginType::Activity2
                    ) && !self.debug_overlay.borrow().map_resource_show_activity
                    {
                        continue;
                    }

                    if self.debug_overlay.borrow().map_resource_only_show_named && label.is_none() {
                        continue;
                    }

                    let distance = transform.translation.distance(camera.position);
                    {
                        let debug_overlay = self.debug_overlay.borrow();
                        if debug_overlay.map_resource_distance_limit_enabled
                            && distance > self.debug_overlay.borrow().map_resource_distance
                        {
                            continue;
                        }
                    }

                    // Draw the debug shape before we cull the points to prevent shapes from popping in/out when the point goes off/onscreen
                    let mut debug_shapes = resources.get_mut::<DebugShapes>().unwrap();
                    res.resource.draw_debug_shape(transform, &mut debug_shapes);

                    if !camera.is_point_visible(transform.translation) {
                        continue;
                    }

                    rp_list.push((
                        distance,
                        *transform,
                        StrippedResourcePoint {
                            resource: res.resource.clone(),
                            has_havok_data: res.has_havok_data,
                            origin: res.origin,
                            label: label.map(|v| v.0.clone()),
                        },
                    ))
                }

                if self.debug_overlay.borrow().map_resource_label_background {
                    rp_list.sort_by(|a, b| a.0.total_cmp(&b.0));
                    rp_list.reverse();
                }

                for (_, transform, res) in rp_list {
                    let projected_point = camera
                        .projection_view_matrix
                        .project_point3(transform.translation);

                    let screen_point = Vec2::new(
                        ((projected_point.x + 1.0) * 0.5) * screen_size.x,
                        ((1.0 - projected_point.y) * 0.5) * screen_size.y,
                    );

                    let c = res.resource.debug_color();
                    let color = egui::Color32::from_rgb(c[0], c[1], c[2]);
                    if self.debug_overlay.borrow().show_map_resource_label {
                        let debug_string = res.resource.debug_string();
                        let debug_string = if let Some(l) = res.label {
                            format!("{l}\n{debug_string}")
                        } else {
                            debug_string
                        };

                        let debug_string_font = egui::FontId::proportional(14.0);
                        let debug_string_pos: egui::Pos2 =
                            (screen_point + Vec2::new(14.0, 0.0)).to_array().into();
                        if self.debug_overlay.borrow().map_resource_label_background {
                            let debug_string_galley = painter.layout_no_wrap(
                                debug_string.clone(),
                                debug_string_font.clone(),
                                Color32::WHITE,
                            );
                            let mut debug_string_rect = egui::Align2::LEFT_CENTER.anchor_rect(
                                Rect::from_min_size(debug_string_pos, debug_string_galley.size()),
                            );
                            debug_string_rect.extend_with_x(debug_string_pos.x - 11.0 - 14.0);

                            painter.rect(
                                debug_string_rect,
                                egui::Rounding::none(),
                                Color32::from_black_alpha(128),
                                egui::Stroke::default(),
                            );
                        }

                        painter.text(
                            debug_string_pos,
                            egui::Align2::LEFT_CENTER,
                            debug_string,
                            debug_string_font,
                            color,
                        );
                    }

                    painter.text(
                        screen_point.to_array().into(),
                        egui::Align2::CENTER_CENTER,
                        res.resource.debug_icon().to_string(),
                        egui::FontId::proportional(22.0),
                        color,
                    );

                    if res.has_havok_data {
                        painter.image(
                            gui.icons.icon_havok.id(),
                            egui::Rect::from_center_size(
                                egui::Pos2::from(screen_point.to_array())
                                    - egui::pos2(12., 12.).to_vec2(),
                                egui::vec2(16.0, 16.0),
                            ),
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }

                    if res.origin != ResourceOriginType::Map {
                        painter.rect(
                            egui::Rect::from_min_size(
                                screen_point.to_array().into(),
                                [11.0, 11.0].into(),
                            ),
                            egui::Rounding::none(),
                            Color32::from_black_alpha(152),
                            egui::Stroke::default(),
                        );

                        painter.text(
                            egui::Pos2::from(screen_point.to_array()) + egui::vec2(5.5, 5.5),
                            egui::Align2::CENTER_CENTER,
                            match res.origin {
                                ResourceOriginType::Map => "M",
                                ResourceOriginType::Activity => "A",
                                ResourceOriginType::Activity2 => "A2",
                            },
                            egui::FontId::monospace(12.0),
                            match res.origin {
                                ResourceOriginType::Map => Color32::LIGHT_RED,
                                ResourceOriginType::Activity => Color32::GREEN,
                                ResourceOriginType::Activity2 => Color32::RED,
                            },
                        );
                    }
                }
            }
        }

        true
    }
}
