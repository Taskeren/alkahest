use alkahest_data::tfx::TfxRenderStage;
use hecs::Entity;
use windows::Win32::Graphics::Direct3D11::ID3D11PixelShader;

use crate::{
    ecs::{
        dynamic_geometry::draw_dynamic_model_system, static_geometry::draw_static_instances_system,
        terrain::draw_terrain_patches_system, Scene,
    },
    gpu_event,
    renderer::Renderer,
    shader::shader_ball::draw_shaderball_system,
    tfx::technique::ShaderModule,
};

impl Renderer {
    pub(super) fn run_renderstage_systems(&self, scene: &Scene, stage: TfxRenderStage) {
        gpu_event!(self.gpu, stage.to_string());

        if matches!(
            stage,
            TfxRenderStage::GenerateGbuffer
                | TfxRenderStage::ShadowGenerate
                | TfxRenderStage::DepthPrepass
        ) {
            draw_terrain_patches_system(self, scene);
            draw_shaderball_system(self, scene);
        }

        draw_static_instances_system(self, scene, stage);
        draw_dynamic_model_system(self, scene, stage);
    }
}
