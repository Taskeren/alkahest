use std::sync::atomic::Ordering;

use alkahest_data::{
    technique::StateSelection,
    tfx::{TfxRenderStage, TfxShaderStage},
};
use bevy_ecs::entity::Entity;
use serde::{Deserialize, Serialize};

use crate::{
    ecs::{
        render::light::{ShadowGenerationMode, ShadowMapRenderer},
        transform::Transform,
        visibility::{ViewVisibility, VisibilityHelper},
        Scene,
    },
    gpu::DepthMode,
    gpu_event, gpu_profile_event,
    renderer::Renderer,
    util::{black_magic::EntityRefDarkMagic, Hocus},
};

impl Renderer {
    pub fn update_shadow_maps(&self, scene: &mut Scene) {
        if self.settings.shadow_quality == ShadowQuality::Off || self.settings.matcap {
            return;
        }

        self.gpu.set_depth_mode(DepthMode::Flipped);

        gpu_profile_event!(self.gpu, "update_shadow_maps");
        self.gpu
            .current_states
            .store(StateSelection::new(Some(0), Some(2), Some(2), Some(6)));
        self.gpu.flush_states();

        let mut shadow_renderers = vec![];
        for (e, shadow, view_vis) in scene
            .query::<(Entity, &mut ShadowMapRenderer, Option<&ViewVisibility>)>()
            .iter(scene)
        {
            // TODO(cohae): view visibility might change a bit, since shadow maps are technically views as well
            // Only update shadow maps for visible lights
            if view_vis.is_visible(0) || !self.data.lock().asset_manager.is_idle() {
                shadow_renderers.push((e, shadow.last_update));
            }
        }

        shadow_renderers.sort_by_key(|(_, last_update)| *last_update);
        shadow_renderers.truncate(self.settings.shadow_updates_per_frame);

        for (e, _) in shadow_renderers {
            gpu_event!(self.gpu, "update_shadow_map", e.index().to_string());

            let er = scene.entity(e);
            let mut shadow = er.get_mut::<ShadowMapRenderer>().unwrap();
            shadow.last_update = self.frame_index.load(Ordering::Relaxed);
            let transform = er.get::<Transform>().unwrap();

            self.gpu
                .shadowmap_vs_t2
                .bind(&self.gpu, 2, TfxShaderStage::Vertex);

            self.bind_view(&*shadow, e.index() as usize);

            if shadow.stationary_needs_update {
                self.pocus().active_shadow_generation_mode = ShadowGenerationMode::StationaryOnly;
                shadow.bind_for_generation(transform, self, ShadowGenerationMode::StationaryOnly);

                self.run_renderstage_systems(scene.pocus(), TfxRenderStage::ShadowGenerate);

                if !self.data.lock().asset_manager.is_idle() {
                    shadow.stationary_needs_update = true;
                }
            }

            self.pocus().active_shadow_generation_mode = ShadowGenerationMode::MovingOnly;
            shadow.bind_for_generation(transform, self, ShadowGenerationMode::MovingOnly);
            self.run_renderstage_systems(scene, TfxRenderStage::ShadowGenerate);
        }

        self.gpu.set_depth_mode(DepthMode::Normal);
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, strum::EnumIter, strum::Display)]
pub enum ShadowQuality {
    Off,
    Lowest,
    Low,
    Medium,
    High,
    Highest,
}

impl ShadowQuality {
    pub fn pcf_samples(&self) -> ShadowPcfSamples {
        match self {
            ShadowQuality::Off => ShadowPcfSamples::Samples13,
            ShadowQuality::Low => ShadowPcfSamples::Samples13,
            ShadowQuality::Lowest => ShadowPcfSamples::Samples13,
            ShadowQuality::Medium => ShadowPcfSamples::Samples17,
            ShadowQuality::High => ShadowPcfSamples::Samples21,
            ShadowQuality::Highest => ShadowPcfSamples::Samples21,
        }
    }

    pub fn resolution(&self) -> u32 {
        match self {
            ShadowQuality::Off | ShadowQuality::Lowest => 256,
            ShadowQuality::Low => 512,
            ShadowQuality::Medium => 1024,
            ShadowQuality::High => 2048,
            ShadowQuality::Highest => 4096,
        }
    }
}

pub enum ShadowPcfSamples {
    Samples13 = 0,
    Samples17 = 1,
    Samples21 = 2,
}
