use std::sync::Arc;

use alkahest_data::{
    dxgi::DxgiFormat, geometry::EPrimitiveType, technique::StateSelection, tfx::TfxShaderStage,
};
use glam::{FloatExt, Mat4, Vec3, Vec4};
use windows::Win32::Graphics::Direct3D11::{ID3D11PixelShader, ID3D11VertexShader};

use crate::{
    gpu::{buffer::ConstantBufferCached, texture::Texture, util::DxDeviceExt, GpuContext},
    include_dxbc,
    renderer::Renderer,
};

pub struct SsaoRenderer {
    pub scope: ConstantBufferCached<ScopeAlkahestSsao>,
    noise_texture: Texture,

    shader_vs: ID3D11VertexShader,
    shader_ps: ID3D11PixelShader,
    shader_blur_ps: ID3D11PixelShader,
}

impl SsaoRenderer {
    pub fn new(gctx: Arc<GpuContext>) -> anyhow::Result<Self> {
        let mut noise = vec![];
        let mut rng = fastrand::Rng::with_seed(0xcb65a5a72901bc71);
        for _ in 0..16 {
            noise.push(Vec3::new(rng.f32() * 2.0 - 1.0, rng.f32() * 2.0 - 1.0, 0.0));
        }

        let noise_texture = Texture::load_2d_raw(
            &gctx.device,
            4,
            4,
            bytemuck::cast_slice(&noise),
            DxgiFormat::R32G32B32_FLOAT,
            Some("SSAO Noise Texture"),
        )?;

        let shader_vs = gctx
            .device
            .load_vertex_shader(include_dxbc!(vs "postprocess/ssao.hlsl"))
            .unwrap();
        let shader_ps = gctx
            .device
            .load_pixel_shader(include_dxbc!(ps "postprocess/ssao.hlsl"))
            .unwrap();
        let shader_blur_ps = gctx
            .device
            .load_pixel_shader(include_dxbc!(ps "postprocess/ssao_blur_and_apply.hlsl"))
            .unwrap();

        Ok(Self {
            scope: ConstantBufferCached::create_init(gctx.clone(), &ScopeAlkahestSsao::default())?,
            noise_texture,
            shader_vs,
            shader_ps,
            shader_blur_ps,
        })
    }

    pub fn draw(&self, renderer: &Renderer) {
        let (intermediate_rt, intermediate_view) = {
            let e = &renderer.data.lock().gbuffers.ssao_intermediate;
            e.clear(&[0.0, 0.0, 0.0, 0.0]);
            (e.render_target.clone(), e.view.clone())
        };
        let externs = &mut renderer.data.lock().externs;
        {
            let scope = self.scope.data();
            if let Some(view) = &externs.view {
                scope.target_pixel_to_world = view.target_pixel_to_world
            } else {
                return;
            }
        }

        if let Some(deferred) = &externs.deferred {
            unsafe {
                renderer.gpu.lock_context().PSSetShaderResources(
                    0,
                    Some(&[deferred.deferred_depth.view(), deferred.deferred_rt1.view()]),
                );

                renderer
                    .gpu
                    .lock_context()
                    .PSSetConstantBuffers(0, Some(&[Some(self.scope.buffer().clone())]));
            }

            self.noise_texture
                .bind(&renderer.gpu, 2, TfxShaderStage::Pixel);
        } else {
            return;
        }

        unsafe {
            let dxstate = renderer.gpu.backup_state();

            renderer
                .gpu
                .lock_context()
                .OMSetRenderTargets(Some(&[Some(intermediate_rt)]), None);

            renderer.gpu.set_blend_state(0);
            renderer.gpu.lock_context().RSSetState(None);
            renderer.gpu.set_input_topology(EPrimitiveType::Triangles);
            renderer.gpu.lock_context().OMSetDepthStencilState(None, 0);
            renderer
                .gpu
                .lock_context()
                .VSSetShader(&self.shader_vs, None);
            renderer
                .gpu
                .lock_context()
                .PSSetShader(&self.shader_ps, None);

            renderer.gpu.lock_context().Draw(3, 0);

            renderer.gpu.current_states.store(StateSelection::new(
                Some(3),
                Some(0),
                Some(1),
                Some(1),
            ));
            renderer.gpu.set_blend_state(3);
            renderer.gpu.restore_state(&dxstate);
            renderer
                .gpu
                .lock_context()
                .PSSetShader(&self.shader_blur_ps, None);
            renderer
                .gpu
                .lock_context()
                .PSSetShaderResources(0, Some(&[Some(intermediate_view)]));
            renderer.gpu.lock_context().Draw(3, 0);
        }
    }
}

const KERNEL_SIZE: usize = 32;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct ScopeAlkahestSsao {
    pub target_pixel_to_world: Mat4,

    pub radius: f32,
    pub bias: f32,
    pub kernel_size: i32,
    pub samples: [Vec4; KERNEL_SIZE],
}

impl Default for ScopeAlkahestSsao {
    fn default() -> Self {
        // TODO(cohae): Configurable kernel size
        let mut samples = [Vec4::ZERO; KERNEL_SIZE];
        let mut rng = fastrand::Rng::with_seed(0xbc65a5a72901bc71);
        for (i, sample) in samples.iter_mut().enumerate() {
            let mut nsample = Vec3::new(rng.f32() * 2.0 - 1.0, rng.f32() * 2.0 - 1.0, 0.0);
            nsample = nsample.normalize();
            nsample *= rng.f32();

            let mut scale = i as f32 / KERNEL_SIZE as f32;
            scale = 0.1f32.lerp(1.0, scale * scale);
            nsample *= scale;
            *sample = nsample.extend(1.0);
        }

        Self {
            target_pixel_to_world: Default::default(),
            radius: 1.00,
            bias: 0.10,
            kernel_size: KERNEL_SIZE as _,
            samples,
        }
    }
}
