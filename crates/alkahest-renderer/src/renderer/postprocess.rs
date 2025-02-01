use alkahest_data::technique::StateSelection;

use crate::{ecs::Scene, gpu_event, renderer::Renderer, tfx::externs};

impl Renderer {
    pub fn draw_postprocessing_pass(&self, _scene: &mut Scene) {
        gpu_event!(self.gpu, "postprocess");
        unsafe {
            self.gpu.lock_context().OMSetRenderTargets(Some(&[]), None);
            self.gpu.lock_context().PSSetShaderResources(0, Some(&[]));
        }

        {
            let data = &mut self.data.lock();
            // Ping, Pong
            // Blits shading_result to pong
            let (_source, target) = data.gbuffers.get_postprocess_rt(true);
            self.gpu.blit_texture_alphaluminance(
                &data.gbuffers.shading_result.view,
                &target.render_target,
            );
        }

        if self.settings.feature_fxaa {
            unsafe {
                let data = &mut self.data.lock();
                // (pong, ping)
                // renders to ping
                let (source, target) = data.gbuffers.get_postprocess_rt(true);
                let rt = target.render_target.clone();
                data.externs.fxaa = Some(externs::Fxaa {
                    source_texture: source.view.clone().into(),
                    noise_time: self.time.load().elapsed(),
                    ..Default::default()
                });

                self.gpu
                    .lock_context()
                    .OMSetRenderTargets(Some(&[Some(rt), None]), None);
            }

            gpu_event!(self.gpu, "fxaa");
            let pipeline = if self.settings.fxaa_noise {
                &self.render_globals.pipelines.fxaa_noise
            } else {
                &self.render_globals.pipelines.fxaa
            };

            self.gpu
                .current_states
                .store(StateSelection::new(Some(0), Some(0), Some(0), Some(0)));
            self.execute_global_pipeline(pipeline, "fxaa(_noise)");
        }

        {
            unsafe {
                self.gpu.lock_context().OMSetRenderTargets(Some(&[]), None);
                self.gpu.lock_context().PSSetShaderResources(0, Some(&[]));
            }
            let data = &mut self.data.lock();
            let output_rt = data.gbuffers.get_postprocess_output();
            // output_rt.copy_to(&data.gbuffers.shading_result);
            self.gpu.blit_texture(
                &output_rt.view,
                &data.gbuffers.shading_result.render_target,
                false,
            );
        }
    }
}
