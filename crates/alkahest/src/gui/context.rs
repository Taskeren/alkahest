use std::{any::TypeId, sync::Arc};

use alkahest_renderer::{
    gpu::GpuContext,
    util::{d3d::ErrorExt, image::Png},
};
use anyhow::Context;
use egui::{InputState, Key, KeyboardShortcut, Modifiers};
use egui_directx11::DirectX11Renderer;
use egui_winit::EventResponse;
use indexmap::IndexMap;
use smallvec::SmallVec;
use winit::{event::WindowEvent, window::Window};

use super::sodi::Sodi;
use crate::{
    config::APP_DIRS,
    gui::{
        bottom_bar::BottomBar,
        configuration::RenderSettingsPanel,
        console::ConsolePanel,
        crosshair::CrosshairOverlay,
        fps_display::FpsDisplayOverlay,
        gizmo::GizmoSelector,
        inspector::InspectorPanel,
        load_indicator::ResourceLoadIndicatorOverlay,
        menu::MenuBar,
        node_gizmos::NodeGizmoOverlay,
        outliner::OutlinerPanel,
        profiler::PuffinProfiler,
        tfx::{TfxErrorViewer, TfxExternEditor},
    },
    resources::AppResources,
    util::image::EguiPngLoader,
};

pub struct GuiContext {
    pub egui: egui::Context,
    pub integration: egui_winit::State,
    pub renderer: Option<egui_directx11::DirectX11Renderer>,
    gctx: Arc<GpuContext>,
    resources: GuiResources,
}

impl GuiContext {
    pub fn create(window: &Window, gctx: Arc<GpuContext>) -> Self {
        let egui = egui::Context::default();

        egui.add_image_loader(Arc::new(EguiPngLoader::default()));

        if let Ok(Ok(data)) = std::fs::read_to_string(APP_DIRS.config_dir().join("egui.ron"))
            .map(|s| ron::from_str::<egui::Memory>(&s))
        {
            info!("Loaded egui state from egui.ron");
            egui.memory_mut(|memory| *memory = data);
        }

        let integration = egui_winit::State::new(
            egui.clone(),
            egui::ViewportId::default(),
            window,
            None,
            Some(8192),
        );

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Inter-Medium".into(),
            egui::FontData::from_static(include_bytes!("../../assets/fonts/Inter-Medium.ttf")),
        );

        fonts.font_data.insert(
            "materialdesignicons".into(),
            egui::FontData::from_static(include_bytes!(
                "../../assets/fonts/materialdesignicons-webfont.ttf"
            )),
        );
        fonts.font_data.insert(
            "Destiny_Keys".into(),
            egui::FontData::from_static(include_bytes!("../../assets/fonts/Destiny_Keys.otf")),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "materialdesignicons".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(1, "Destiny_Keys".to_owned());
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(2, "Inter-Medium".into());

        egui.set_fonts(fonts);
        egui.set_style(style::style());

        let renderer = gctx.swap_chain.as_ref().map(|swap_chain| {
            egui_directx11::DirectX11Renderer::init_from_swapchain(swap_chain)
                .expect("Failed to initialize egui renderer")
        });

        GuiContext {
            resources: GuiResources::load(&egui),
            egui,
            integration,
            renderer,
            gctx,
        }
    }

    pub fn handle_event(&mut self, window: &Window, event: &WindowEvent) -> EventResponse {
        self.integration.on_window_event(window, event)
    }

    pub fn draw_frame<PF>(&mut self, window: &Window, paint: PF)
    where
        PF: FnOnce(&GuiCtx<'_>, &egui::Context),
    {
        profiling::scope!("GuiContext::draw_frame");
        let input = self.integration.take_egui_input(window);

        if let Some(ref swap_chain) = self.gctx.swap_chain {
            if let Some(ref mut renderer) = self.renderer {
                let output = renderer
                    .paint(swap_chain, input, &self.egui, |renderer, context| {
                        paint(
                            &GuiCtx {
                                icons: &self.resources,
                                _integration: renderer,
                            },
                            context,
                        )
                    })
                    .context("Failed to paint egui frame")
                    .map_err(|e| e.with_d3d_error(&self.gctx))
                    .unwrap();

                self.integration
                    .handle_platform_output(window, output.platform_output)
            }
        }
    }

    // pub fn input<R>(&self, reader: impl FnOnce(&InputState) -> R) -> R {
    //     self.egui.input(reader)
    // }

    pub fn input_mut<R>(&mut self, reader: impl FnOnce(&mut InputState) -> R) -> R {
        self.egui.input_mut(reader)
    }
}

impl Drop for GuiContext {
    fn drop(&mut self) {
        match self.egui.memory(ron::to_string) {
            Ok(memory) => {
                if let Err(e) = std::fs::write(APP_DIRS.config_dir().join("egui.ron"), memory) {
                    error!("Failed to write egui state: {e}");
                }
            }
            Err(e) => {
                error!("Failed to serialize egui state: {e}");
            }
        };
    }
}

#[derive(PartialEq)]
pub enum ViewAction {
    Close,
}

pub trait GuiView {
    fn draw(
        &mut self,
        ctx: &egui::Context,
        window: &Window,
        resources: &AppResources,
        gui: &GuiCtx<'_>,
    ) -> Option<ViewAction>;

    fn dispose(&mut self, _ctx: &egui::Context, _resources: &AppResources, _gui: &GuiCtx<'_>) {}
}

#[derive(Default)]
pub struct GuiViewManager {
    views: IndexMap<TypeId, Box<dyn GuiView>>,
    views_overlay: IndexMap<TypeId, Box<dyn GuiView>>,

    pub hide_views: bool,
}

impl GuiViewManager {
    pub fn with_default_views() -> Self {
        let mut views = Self::default();

        views.insert(NodeGizmoOverlay);
        views.insert(MenuBar::default());
        views.insert(ConsolePanel::default());
        views.insert(TfxErrorViewer::default());
        views.insert(TfxExternEditor::default());
        views.insert(RenderSettingsPanel);
        views.insert(BottomBar);
        views.insert(OutlinerPanel::default());
        views.insert(InspectorPanel);
        views.insert(PuffinProfiler);
        views.insert(CrosshairOverlay);
        views.insert(ResourceLoadIndicatorOverlay);
        views.insert(GizmoSelector);
        views.insert(Sodi::default());

        views.insert_overlay(FpsDisplayOverlay::default());

        views
    }

    pub fn insert<T: GuiView + 'static>(&mut self, view: T) {
        self.views.insert(TypeId::of::<T>(), Box::new(view));
    }
    pub fn insert_overlay<T: GuiView + 'static>(&mut self, view: T) {
        self.views_overlay.insert(TypeId::of::<T>(), Box::new(view));
    }

    // pub fn remove<T: GuiView + 'static>(&mut self) {
    //     self.views.shift_remove(&TypeId::of::<T>());
    // }

    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        window: &Window,
        resources: &AppResources,
        gui: &GuiCtx<'_>,
    ) {
        if ctx.input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::H,
            ))
        }) {
            self.hide_views = !self.hide_views;
        }

        if !self.hide_views {
            let mut to_remove = SmallVec::<[TypeId; 4]>::new();
            for (tid, view) in self.views.iter_mut() {
                if let Some(result) = view.draw(ctx, window, resources, gui) {
                    if result == ViewAction::Close {
                        to_remove.push(*tid);
                    }
                }
            }

            for tid in to_remove {
                if let Some(mut view) = self.views.shift_remove(&tid) {
                    view.dispose(ctx, resources, gui);
                }
            }
        }

        for view in self.views_overlay.values_mut() {
            view.draw(ctx, window, resources, gui);
        }
    }
}

pub struct GuiCtx<'a> {
    pub icons: &'a GuiResources,
    pub _integration: &'a mut DirectX11Renderer,
}

pub struct GuiResources {
    pub icon_havok: egui::TextureHandle,
}

impl GuiResources {
    pub fn load(ctx: &egui::Context) -> Self {
        let img = Png::from_bytes(include_bytes!("../../assets/icons/havok_dark_256.png")).unwrap();
        let icon_havok = ctx.load_texture(
            "Havok 64x64",
            egui::ImageData::Color(
                egui::ColorImage::from_rgba_premultiplied(img.dimensions, &img.data).into(),
            ),
            egui::TextureOptions {
                magnification: egui::TextureFilter::Linear,
                minification: egui::TextureFilter::Linear,
                wrap_mode: egui::TextureWrapMode::ClampToEdge,
            },
        );

        Self { icon_havok }
    }
}

// TODO(cohae): Will be replaced by the panels system with the new UI
#[derive(Default)]
pub struct HiddenWindows {
    pub tfx_extern_editor: bool,
    pub tfx_extern_debugger: bool,
    pub cpu_profiler: bool,
}

mod style {
    // Generated by egui-themer (https://github.com/grantshandy/egui-themer).

    use egui::{
        epaint::Shadow,
        style::{Interaction, Selection, Spacing, WidgetVisuals, Widgets},
        Color32, Margin, Rounding, Stroke, Style, Vec2, Visuals,
    };

    pub fn style() -> Style {
        Style {
            // override the text styles here:
            // override_text_style: Option<TextStyle>

            // override the font id here:
            // override_font_id: Option<FontId>

            // set your text styles here:
            // text_styles: BTreeMap<TextStyle, FontId>,

            // set your drag value text style:
            // drag_value_text_style: TextStyle,
            spacing: Spacing {
                item_spacing: Vec2 { x: 8.0, y: 3.0 },
                window_margin: Margin {
                    left: 6.0,
                    right: 6.0,
                    top: 6.0,
                    bottom: 6.0,
                },
                button_padding: Vec2 { x: 9.0, y: 5.0 },
                menu_margin: Margin {
                    left: 6.0,
                    right: 6.0,
                    top: 6.0,
                    bottom: 6.0,
                },
                indent: 18.0,
                interact_size: Vec2 { x: 40.0, y: 20.0 },
                slider_width: 100.0,
                combo_width: 100.0,
                text_edit_width: 280.0,
                icon_width: 14.0,
                icon_width_inner: 8.0,
                icon_spacing: 4.0,
                tooltip_width: 600.0,
                indent_ends_with_horizontal_line: true,
                combo_height: 200.0,
                // scroll_bar_width: 8.0,
                // scroll_handle_min_length: 12.0,
                // scroll_bar_inner_margin: 4.0,
                // scroll_bar_outer_margin: 0.0,
                ..Default::default()
            },
            interaction: Interaction {
                resize_grab_radius_side: 5.0,
                resize_grab_radius_corner: 10.0,
                show_tooltips_only_when_still: true,
                ..Default::default()
            },
            visuals: Visuals {
                dark_mode: true,
                override_text_color: None,
                widgets: Widgets {
                    noninteractive: WidgetVisuals {
                        bg_fill: Color32::from_rgba_premultiplied(60, 60, 60, 128),
                        weak_bg_fill: Color32::from_rgba_premultiplied(38, 38, 38, 128),
                        bg_stroke: Stroke {
                            width: 0.20,
                            color: Color32::from_rgb(255, 255, 255),
                        },
                        rounding: Rounding {
                            nw: 6.0,
                            ne: 6.0,
                            sw: 6.0,
                            se: 6.0,
                        },
                        fg_stroke: Stroke {
                            width: 1.5,
                            color: Color32::from_rgba_premultiplied(180, 180, 180, 255),
                        },
                        expansion: 0.0,
                    },
                    inactive: WidgetVisuals {
                        bg_fill: Color32::from_rgba_premultiplied(60, 60, 60, 255),
                        weak_bg_fill: Color32::from_rgba_premultiplied(38, 38, 38, 255),
                        bg_stroke: Stroke {
                            width: 0.25,
                            color: Color32::from_rgba_premultiplied(255, 255, 255, 255),
                        },
                        rounding: Rounding {
                            nw: 6.0,
                            ne: 6.0,
                            sw: 6.0,
                            se: 6.0,
                        },
                        fg_stroke: Stroke {
                            width: 1.5,
                            color: Color32::from_rgba_premultiplied(180, 180, 180, 255),
                        },
                        expansion: 0.0,
                    },
                    hovered: WidgetVisuals {
                        bg_fill: Color32::from_rgba_premultiplied(70, 70, 70, 255),
                        weak_bg_fill: Color32::from_rgba_premultiplied(70, 70, 70, 255),
                        bg_stroke: Stroke {
                            width: 0.5,
                            color: Color32::from_rgba_premultiplied(150, 150, 150, 255),
                        },
                        rounding: Rounding {
                            nw: 6.0,
                            ne: 6.0,
                            sw: 6.0,
                            se: 6.0,
                        },
                        fg_stroke: Stroke {
                            width: 1.5,
                            color: Color32::from_rgba_premultiplied(240, 240, 240, 255),
                        },
                        expansion: 1.0,
                    },
                    active: WidgetVisuals {
                        bg_fill: Color32::from_rgba_premultiplied(55, 55, 55, 255),
                        weak_bg_fill: Color32::from_rgba_premultiplied(55, 55, 55, 255),
                        bg_stroke: Stroke {
                            width: 0.25,
                            color: Color32::from_rgba_premultiplied(255, 255, 255, 255),
                        },
                        rounding: Rounding {
                            nw: 6.0,
                            ne: 6.0,
                            sw: 6.0,
                            se: 6.0,
                        },
                        fg_stroke: Stroke {
                            width: 1.5,
                            color: Color32::from_rgba_premultiplied(255, 255, 255, 255),
                        },
                        expansion: 0.0,
                    },
                    open: WidgetVisuals {
                        bg_fill: Color32::from_rgba_premultiplied(27, 27, 27, 255),
                        weak_bg_fill: Color32::from_rgba_premultiplied(27, 27, 27, 255),
                        bg_stroke: Stroke {
                            width: 0.25,
                            color: Color32::from_rgba_premultiplied(60, 60, 60, 255),
                        },
                        rounding: Rounding {
                            nw: 6.0,
                            ne: 6.0,
                            sw: 6.0,
                            se: 6.0,
                        },
                        fg_stroke: Stroke {
                            width: 1.5,
                            color: Color32::from_rgba_premultiplied(210, 210, 210, 255),
                        },
                        expansion: 0.0,
                    },
                },
                selection: Selection {
                    bg_fill: Color32::from_rgba_premultiplied(23, 95, 93, 255),
                    stroke: Stroke {
                        width: 5.4,
                        color: Color32::from_rgba_premultiplied(255, 255, 255, 255),
                    },
                },
                hyperlink_color: Color32::from_rgba_premultiplied(90, 170, 255, 255),
                faint_bg_color: Color32::from_rgba_premultiplied(5, 5, 5, 0),
                extreme_bg_color: Color32::from_rgba_premultiplied(10, 10, 10, 255),
                code_bg_color: Color32::from_rgba_premultiplied(64, 64, 64, 255),
                warn_fg_color: Color32::from_rgba_premultiplied(255, 143, 0, 255),
                error_fg_color: Color32::from_rgba_premultiplied(255, 0, 0, 255),
                window_rounding: Rounding {
                    nw: 6.0,
                    ne: 6.0,
                    sw: 6.0,
                    se: 6.0,
                },
                window_shadow: Shadow {
                    // extrusion: 16.0,
                    // color: Color32::from_rgba_premultiplied(0, 0, 0, 96),
                    ..Default::default()
                },
                window_fill: Color32::from_rgba_premultiplied(6, 5, 7, 255),
                window_stroke: Stroke {
                    width: 1.0,
                    color: Color32::from_rgba_premultiplied(21, 21, 21, 255),
                },
                menu_rounding: Rounding {
                    nw: 6.0,
                    ne: 6.0,
                    sw: 6.0,
                    se: 6.0,
                },
                panel_fill: Color32::from_rgba_premultiplied(27, 27, 27, 255),
                popup_shadow: Shadow {
                    // extrusion: 16.0,
                    // color: Color32::from_rgba_premultiplied(0, 0, 0, 96),
                    ..Default::default()
                },
                resize_corner_size: 12.0,
                // text_cursor_width: 2.0,
                // text_cursor_preview: false,
                clip_rect_margin: 3.0,
                button_frame: true,
                collapsing_header_frame: false,
                indent_has_left_vline: true,
                striped: false,
                slider_trailing_fill: false,
                ..Default::default()
            },
            animation_time: 0.083333336,
            explanation_tooltips: false,
            ..Default::default()
        }
    }
}
