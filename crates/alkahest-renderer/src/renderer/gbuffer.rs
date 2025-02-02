use std::{mem::size_of, sync::Arc};

use alkahest_data::dxgi::DxgiFormat;
use anyhow::Context;
use crossbeam::atomic::AtomicCell;
use glam::Vec3;
use windows::Win32::Graphics::{
    Direct3D::{D3D11_SRV_DIMENSION_TEXTURE2D, D3D11_SRV_DIMENSION_TEXTURE2DARRAY},
    Direct3D11::*,
    Dxgi::Common::*,
};

use crate::{camera::Camera, gpu::GpuContext, gpu_event, util::d3d::D3dResource};

pub struct GBuffer {
    pub rt0: RenderTarget,
    pub rt1: RenderTarget,
    pub rt1_read: RenderTarget,
    pub rt2: RenderTarget,
    pub rt3: RenderTarget,

    pub light_diffuse: RenderTarget,
    pub light_specular: RenderTarget,
    pub light_ibl_specular: RenderTarget,

    pub shading_result: RenderTarget,
    pub shading_result_read: RenderTarget,
    pub depth: DepthState,
    pub depth_staging: CpuStagingBuffer,

    pub ssao_intermediate: RenderTarget,
    pub atmos_ss_far_lookup: RenderTarget,
    pub atmos_ss_near_lookup: RenderTarget,

    pub depth_angle_density_lookup: RenderTarget,

    pub postprocess_ping: RenderTarget,
    pub postprocess_pong: RenderTarget,
    postprocess_pingpong: AtomicCell<PingPong>,

    current_size: (u32, u32),
}

#[derive(PartialEq, Clone, Copy)]
enum PingPong {
    Ping,
    Pong,
}

impl PingPong {
    pub fn next(&self) -> Self {
        match self {
            PingPong::Ping => PingPong::Pong,
            PingPong::Pong => PingPong::Ping,
        }
    }
}

impl GBuffer {
    pub fn create(mut size: (u32, u32), gctx: Arc<GpuContext>) -> anyhow::Result<Self> {
        if size.0 == 0 || size.1 == 0 {
            size = (1, 1);
        }

        Ok(Self {
            // rt0: RenderTarget::create(size, DxgiFormat::R11G11B10_FLOAT, gctx.clone(), "RT0")
            //     .context("RT0")?,
            rt0: RenderTarget::create(size, DxgiFormat::B8G8R8A8_UNORM_SRGB, gctx.clone(), "RT0")
                .context("RT0")?,
            rt1: RenderTarget::create(size, DxgiFormat::R10G10B10A2_UNORM, gctx.clone(), "RT1")
                .context("RT1")?,
            rt1_read: RenderTarget::create(
                size,
                DxgiFormat::R10G10B10A2_UNORM,
                gctx.clone(),
                "RT1_Clone",
            )
            .context("RT1_Clone")?,
            rt2: RenderTarget::create(size, DxgiFormat::B8G8R8A8_UNORM, gctx.clone(), "RT2")
                .context("RT2")?,
            rt3: RenderTarget::create(size, DxgiFormat::B8G8R8A8_UNORM, gctx.clone(), "RT3")
                .context("RT3")?,

            light_diffuse: RenderTarget::create(
                size,
                DxgiFormat::R11G11B10_FLOAT,
                gctx.clone(),
                "Light_Diffuse",
            )
            .context("Light_Diffuse")?,
            light_specular: RenderTarget::create(
                size,
                DxgiFormat::R11G11B10_FLOAT,
                gctx.clone(),
                "Light_Specular",
            )
            .context("Light_Specular")?,
            light_ibl_specular: RenderTarget::create(
                size,
                DxgiFormat::R11G11B10_FLOAT,
                gctx.clone(),
                "Specular_IBL",
            )
            .context("Specular_IBL")?,

            shading_result: RenderTarget::create(
                size,
                DxgiFormat::R11G11B10_FLOAT,
                gctx.clone(),
                "Staging",
            )
            .context("Staging")?,
            shading_result_read: RenderTarget::create(
                size,
                DxgiFormat::R11G11B10_FLOAT,
                gctx.clone(),
                "Staging_Clone",
            )
            .context("Staging_Clone")?,
            depth: DepthState::create(gctx.clone(), size, "gbuffer_depth").context("Depth")?,
            depth_staging: CpuStagingBuffer::create(
                size,
                DxgiFormat::R32_TYPELESS,
                gctx.clone(),
                "Depth_Buffer_Staging",
            )
            .context("Depth_Buffer_Staging")?,
            ssao_intermediate: RenderTarget::create(
                size,
                DxgiFormat::R8_UNORM,
                gctx.clone(),
                "SSAO_Intermediate",
            )
            .context("SSAO_Intermediate")?,

            atmos_ss_far_lookup: RenderTarget::create(
                (size.0 / 4, size.1 / 4),
                DxgiFormat::R16G16B16A16_FLOAT,
                gctx.clone(),
                "atmos_ss_far_lookup",
            )
            .context("atmos_ss_far_lookup")?,
            atmos_ss_near_lookup: RenderTarget::create(
                (size.0 / 4, size.1 / 4),
                DxgiFormat::R16G16B16A16_FLOAT,
                gctx.clone(),
                "atmos_ss_near_lookup",
            )
            .context("atmos_ss_near_lookup")?,

            depth_angle_density_lookup: RenderTarget::create(
                (512, 512),
                DxgiFormat::R16G16B16A16_FLOAT,
                gctx.clone(),
                "depth_angle_density_lookup",
            )
            .context("depth_angle_density_lookup")?,

            postprocess_ping: RenderTarget::create(
                size,
                DxgiFormat::R8G8B8A8_UNORM_SRGB,
                gctx.clone(),
                "postprocess_ping",
            )
            .context("postprocess_ping")?,
            postprocess_pong: RenderTarget::create(
                size,
                DxgiFormat::R8G8B8A8_UNORM_SRGB,
                gctx.clone(),
                "postprocess_ping",
            )
            .context("postprocess_ping")?,
            postprocess_pingpong: AtomicCell::new(PingPong::Ping),

            current_size: size,
        })
    }

    pub fn resize(&mut self, mut new_size: (u32, u32)) -> anyhow::Result<()> {
        if new_size.0 == 0 || new_size.1 == 0 {
            new_size = (1, 1);
        }

        self.rt0.resize(new_size).context("RT0")?;
        self.rt1.resize(new_size).context("RT1")?;
        self.rt1_read.resize(new_size).context("RT1_Clone")?;
        self.rt2.resize(new_size).context("RT2")?;
        self.rt3.resize(new_size).context("RT3")?;

        self.light_diffuse
            .resize(new_size)
            .context("Light_Diffuse")?;
        self.light_specular
            .resize(new_size)
            .context("Light_Specular")?;
        self.light_ibl_specular
            .resize(new_size)
            .context("Specular_IBL")?;

        self.shading_result.resize(new_size).context("Staging")?;
        self.shading_result_read
            .resize(new_size)
            .context("Staging_Clone")?;
        self.depth.resize(new_size).context("Depth")?;
        self.depth_staging.resize(new_size).context("Depth")?;

        self.atmos_ss_near_lookup
            .resize((new_size.0 / 4, new_size.1 / 4))?;
        self.atmos_ss_far_lookup
            .resize((new_size.0 / 4, new_size.1 / 4))?;
        self.ssao_intermediate.resize(new_size)?;

        self.postprocess_ping.resize(new_size)?;
        self.postprocess_pong.resize(new_size)?;

        self.current_size = new_size;
        Ok(())
    }

    pub fn depth_buffer_read(&self, x: usize, y: usize) -> f32 {
        self.depth_staging
            .map(D3D11_MAP_READ, |m| unsafe {
                let data = m
                    .pData
                    .cast::<u8>()
                    .add(y * m.RowPitch as usize + x * size_of::<f32>())
                    .cast::<f32>();

                data.read()
            })
            .unwrap_or(0.0)
    }
    pub fn depth_buffer_read_center(&self) -> f32 {
        self.depth_buffer_read(
            (self.current_size.0 / 2) as usize,
            (self.current_size.1 / 2) as usize,
        )
    }

    pub fn depth_buffer_distance_pos_center(&self, camera: &Camera) -> (f32, Vec3) {
        let raw_depth = self.depth_buffer_read_center();
        let pos = camera
            .projective_to_world
            .project_point3(Vec3::new(0.0, 0.0, raw_depth));
        let distance = (pos - camera.position()).length();
        (distance, pos)
    }

    /// Returns (source, target). If `swap_after_use` is enabled, the order of the buffers will be reversed on the next call
    pub fn get_postprocess_rt(&self, swap_after_use: bool) -> (&RenderTarget, &RenderTarget) {
        let current_pingpong = self.postprocess_pingpong.load();
        let r = match current_pingpong {
            PingPong::Ping => (&self.postprocess_ping, &self.postprocess_pong),
            PingPong::Pong => (&self.postprocess_pong, &self.postprocess_ping),
        };

        assert_ne!(&r.0.texture, &r.1.texture);

        if swap_after_use {
            self.postprocess_pingpong.store(current_pingpong.next());
        }

        r
    }

    /// Get the last rendertarget that was written to in the postprocess pass
    pub fn get_postprocess_output(&self) -> &RenderTarget {
        match self.postprocess_pingpong.load() {
            PingPong::Ping => &self.postprocess_ping,
            PingPong::Pong => &self.postprocess_pong,
        }
    }
}

pub struct RenderTarget {
    pub texture: ID3D11Texture2D,
    pub render_target: ID3D11RenderTargetView,
    pub view: ID3D11ShaderResourceView,
    pub format: DxgiFormat,
    pub name: String,

    gctx: Arc<GpuContext>,
}

impl RenderTarget {
    pub fn create(
        size: (u32, u32),
        format: DxgiFormat,
        gctx: Arc<GpuContext>,
        name: &str,
    ) -> anyhow::Result<Self> {
        let size = if size.0 == 0 || size.1 == 0 {
            warn!("Zero size render target requested for {name}, using 1x1");
            (1, 1)
        } else {
            size
        };

        unsafe {
            let mut texture = None;
            gctx.device
                .CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: size.0,
                        Height: size.1,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT(format as i32),
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0)
                            as u32,
                        CPUAccessFlags: Default::default(),
                        MiscFlags: Default::default(),
                    },
                    None,
                    Some(&mut texture),
                )
                .context("Failed to create texture")?;
            let texture = texture.unwrap();

            let mut render_target = None;
            gctx.device
                .CreateRenderTargetView(&texture, None, Some(&mut render_target))
                .context("Failed to create RTV")?;
            let render_target = render_target.unwrap();

            let mut view = None;
            gctx.device
                .CreateShaderResourceView(
                    &texture,
                    Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: DXGI_FORMAT(format as i32),
                        ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_SRV {
                                MostDetailedMip: 0,
                                MipLevels: 1,
                            },
                        },
                    }),
                    Some(&mut view),
                )
                .context("Failed to create SRV")?;
            let view = view.unwrap();

            texture.set_debug_name(name);

            Ok(Self {
                texture,
                render_target,
                view,
                format,
                name: name.to_string(),
                gctx,
            })
        }
    }

    pub fn copy_to(&self, dest: &RenderTarget) {
        gpu_event!(self.gctx, "copy", format!("{}->{}", self.name, dest.name));
        unsafe {
            self.gctx
                .lock_context()
                .CopyResource(&dest.texture, &self.texture)
        }
    }

    pub fn copy_to_staging(&self, dest: &CpuStagingBuffer) {
        gpu_event!(
            self.gctx,
            "copy_to_cpu",
            format!("{}->{}", self.name, dest.name)
        );
        unsafe {
            self.gctx
                .lock_context()
                .CopyResource(&dest.texture, &self.texture)
        }
    }

    pub fn resize(&mut self, new_size: (u32, u32)) -> anyhow::Result<()> {
        *self = Self::create(new_size, self.format, self.gctx.clone(), &self.name)?;
        Ok(())
    }

    pub fn clear(&self, color: &[f32; 4]) {
        unsafe {
            self.gctx
                .lock_context()
                .ClearRenderTargetView(&self.render_target, color)
        }
    }

    pub fn get_desc(&self) -> D3D11_TEXTURE2D_DESC {
        unsafe {
            let mut desc = Default::default();
            self.texture.GetDesc(&mut desc);
            desc
        }
    }

    pub fn viewport(&self) -> D3D11_VIEWPORT {
        let desc = self.get_desc();
        D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: desc.Width as f32,
            Height: desc.Height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        }
    }

    /// Binds this render target to RT0, disabling depth/stencil and adjusting the viewport
    pub fn bind(&self) {
        unsafe {
            self.gctx
                .lock_context()
                .OMSetRenderTargets(Some(&[Some(self.render_target.clone())]), None);
            self.gctx
                .lock_context()
                .RSSetViewports(Some(std::slice::from_ref(&self.viewport())));
        }
    }
}

pub struct CpuStagingBuffer {
    pub texture: ID3D11Texture2D,
    pub format: DxgiFormat,
    pub name: String,
    gctx: Arc<GpuContext>,
}

impl CpuStagingBuffer {
    pub fn create(
        size: (u32, u32),
        format: DxgiFormat,
        gctx: Arc<GpuContext>,
        name: &str,
    ) -> anyhow::Result<Self> {
        unsafe {
            let mut texture = None;
            gctx.device
                .CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: size.0,
                        Height: size.1,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT(format as i32),
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_STAGING,
                        BindFlags: Default::default(),
                        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                        MiscFlags: Default::default(),
                    },
                    None,
                    Some(&mut texture),
                )
                .context("Failed to create staging buffer")?;
            let texture = texture.unwrap();

            texture.set_debug_name(name);

            Ok(Self {
                texture,
                format,
                name: name.to_string(),
                gctx,
            })
        }
    }

    pub fn resize(&mut self, new_size: (u32, u32)) -> anyhow::Result<()> {
        *self = Self::create(new_size, self.format, self.gctx.clone(), &self.name)?;
        Ok(())
    }

    pub fn map<R>(
        &self,
        mode: D3D11_MAP,
        f: impl FnOnce(D3D11_MAPPED_SUBRESOURCE) -> R,
    ) -> anyhow::Result<R> {
        unsafe {
            #[allow(clippy::uninit_assumed_init)]
            let mut ptr = D3D11_MAPPED_SUBRESOURCE::default();
            self.gctx
                .lock_context()
                .Map(&self.texture, 0, mode, 0, Some(&mut ptr))
                .context("Failed to map ConstantBuffer")?;

            let r = f(ptr);

            self.gctx.lock_context().Unmap(&self.texture, 0);

            Ok(r)
        }
    }
}

pub struct DepthState {
    pub texture: ID3D11Texture2D,
    // TODO(cohae): replace with global depth stencil states
    pub state: ID3D11DepthStencilState,
    pub state_readonly: ID3D11DepthStencilState,
    pub view: ID3D11DepthStencilView,
    pub texture_view: ID3D11ShaderResourceView,

    pub texture_copy: ID3D11Texture2D,
    pub texture_copy_view: ID3D11ShaderResourceView,
    gctx: Arc<GpuContext>,
    name: String,
}

impl DepthState {
    pub fn create(gctx: Arc<GpuContext>, size: (u32, u32), name: &str) -> anyhow::Result<Self> {
        let size = if size.0 == 0 || size.1 == 0 {
            warn!("Zero size depth state requested, using 1x1");
            (4, 4)
        } else {
            size
        };

        let mut texture = None;
        unsafe {
            gctx.device
                .CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: size.0,
                        Height: size.1,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_R32_TYPELESS,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: (D3D11_BIND_DEPTH_STENCIL.0 | D3D11_BIND_SHADER_RESOURCE.0)
                            as u32,
                        CPUAccessFlags: Default::default(),
                        MiscFlags: Default::default(),
                    },
                    None,
                    Some(&mut texture),
                )
                .context("Failed to create depth texture")?
        };
        let texture = texture.unwrap();
        texture.set_debug_name(&format!("{name} (Texture)"));

        let mut state = None;
        unsafe {
            gctx.device
                .CreateDepthStencilState(
                    &D3D11_DEPTH_STENCIL_DESC {
                        DepthEnable: true.into(),
                        DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ALL,
                        DepthFunc: D3D11_COMPARISON_GREATER_EQUAL,
                        StencilEnable: false.into(),
                        StencilReadMask: 0xff,
                        StencilWriteMask: 0xff,
                        FrontFace: D3D11_DEPTH_STENCILOP_DESC {
                            StencilFailOp: D3D11_STENCIL_OP_KEEP,
                            StencilDepthFailOp: D3D11_STENCIL_OP_INCR,
                            StencilPassOp: D3D11_STENCIL_OP_KEEP,
                            StencilFunc: D3D11_COMPARISON_ALWAYS,
                        },
                        BackFace: D3D11_DEPTH_STENCILOP_DESC {
                            StencilFailOp: D3D11_STENCIL_OP_KEEP,
                            StencilDepthFailOp: D3D11_STENCIL_OP_DECR,
                            StencilPassOp: D3D11_STENCIL_OP_KEEP,
                            StencilFunc: D3D11_COMPARISON_ALWAYS,
                        },
                    },
                    Some(&mut state),
                )
                .context("Failed to create depth stencil state")?
        };
        let state = state.unwrap();

        let mut state_readonly = None;
        unsafe {
            gctx.device
                .CreateDepthStencilState(
                    &D3D11_DEPTH_STENCIL_DESC {
                        DepthEnable: true.into(),
                        DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
                        DepthFunc: D3D11_COMPARISON_GREATER_EQUAL,
                        StencilEnable: false.into(),
                        StencilReadMask: 0xff,
                        StencilWriteMask: 0xff,
                        FrontFace: D3D11_DEPTH_STENCILOP_DESC {
                            StencilFailOp: D3D11_STENCIL_OP_KEEP,
                            StencilDepthFailOp: D3D11_STENCIL_OP_INCR,
                            StencilPassOp: D3D11_STENCIL_OP_KEEP,
                            StencilFunc: D3D11_COMPARISON_ALWAYS,
                        },
                        BackFace: D3D11_DEPTH_STENCILOP_DESC {
                            StencilFailOp: D3D11_STENCIL_OP_KEEP,
                            StencilDepthFailOp: D3D11_STENCIL_OP_DECR,
                            StencilPassOp: D3D11_STENCIL_OP_KEEP,
                            StencilFunc: D3D11_COMPARISON_ALWAYS,
                        },
                    },
                    Some(&mut state_readonly),
                )
                .context("Failed to create read-only depth stencil state")?
        };
        let state_readonly = state_readonly.unwrap();

        let mut view = None;
        unsafe {
            gctx.device
                .CreateDepthStencilView(
                    &texture,
                    Some(&D3D11_DEPTH_STENCIL_VIEW_DESC {
                        Format: DXGI_FORMAT_D32_FLOAT,
                        ViewDimension: D3D11_DSV_DIMENSION_TEXTURE2D,
                        Flags: 0,
                        Anonymous: D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                            Texture2D: { D3D11_TEX2D_DSV { MipSlice: 0 } },
                        },
                    }),
                    Some(&mut view),
                )
                .context("Failed to create depth stencil view")?
        };
        let view = view.unwrap();
        view.set_debug_name(&format!("{name} (DSV)"));

        let mut texture_view = None;
        unsafe {
            gctx.device.CreateShaderResourceView(
                &texture,
                Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_R32_FLOAT,
                    ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D11_TEX2D_SRV {
                            MostDetailedMip: 0,
                            MipLevels: 1,
                        },
                    },
                }),
                Some(&mut texture_view),
            )?
        };
        let texture_view = texture_view.unwrap();
        texture_view.set_debug_name(&format!("{name} (SRV)"));

        let mut texture_copy = None;
        unsafe {
            gctx.device
                .CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: size.0,
                        Height: size.1,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_R32_TYPELESS,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                        CPUAccessFlags: Default::default(),
                        MiscFlags: Default::default(),
                    },
                    None,
                    Some(&mut texture_copy),
                )
                .context("Failed to create depth texture")?
        };
        let texture_copy = texture_copy.unwrap();

        let mut texture_copy_view = None;
        unsafe {
            gctx.device.CreateShaderResourceView(
                &texture_copy,
                Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_R32_FLOAT,
                    ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D11_TEX2D_SRV {
                            MostDetailedMip: 0,
                            MipLevels: 1,
                        },
                    },
                }),
                Some(&mut texture_copy_view),
            )?
        };
        let texture_copy_view = texture_copy_view.unwrap();

        Ok(Self {
            texture,
            state,
            state_readonly,
            view,
            texture_view,
            texture_copy,
            texture_copy_view,
            gctx,
            name: name.to_string(),
        })
    }

    /// Copies the depth texture to texture_copy
    pub fn copy_depth(&self) {
        unsafe {
            self.gctx
                .lock_context()
                .CopyResource(&self.texture_copy, &self.texture);
        }
    }

    pub fn copy_to_staging(&self, dest: &CpuStagingBuffer) {
        unsafe {
            self.gctx
                .lock_context()
                .CopyResource(&dest.texture, &self.texture)
        }
    }

    pub fn resize(&mut self, new_size: (u32, u32)) -> anyhow::Result<()> {
        *self = Self::create(self.gctx.clone(), new_size, &self.name)?;
        Ok(())
    }

    pub fn clear(&self, depth: f32, stencil: u8) {
        unsafe {
            self.gctx.lock_context().ClearDepthStencilView(
                &self.view,
                (D3D11_CLEAR_DEPTH | D3D11_CLEAR_STENCIL).0 as _,
                depth,
                stencil,
            )
        }
    }
}

#[derive(Debug)]
pub struct ShadowDepthMap {
    pub texture: ID3D11Texture2D,
    pub state: ID3D11DepthStencilState,
    pub views: Vec<ID3D11DepthStencilView>,
    pub texture_view: ID3D11ShaderResourceView,
    pub layers: usize,
}

impl ShadowDepthMap {
    pub fn create(size: (u32, u32), layers: usize, device: &ID3D11Device) -> anyhow::Result<Self> {
        let mut texture = None;
        unsafe {
            device
                .CreateTexture2D(
                    &D3D11_TEXTURE2D_DESC {
                        Width: size.0,
                        Height: size.1,
                        MipLevels: 1,
                        ArraySize: layers as u32,
                        Format: DXGI_FORMAT_R32_TYPELESS,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: (D3D11_BIND_DEPTH_STENCIL.0 | D3D11_BIND_SHADER_RESOURCE.0)
                            as u32,
                        CPUAccessFlags: Default::default(),
                        MiscFlags: Default::default(),
                    },
                    None,
                    Some(&mut texture),
                )
                .context("Failed to create depth texture")?
        };
        let texture = texture.unwrap();

        let mut state = None;
        unsafe {
            device
                .CreateDepthStencilState(
                    &D3D11_DEPTH_STENCIL_DESC {
                        DepthEnable: true.into(),
                        DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ALL,
                        DepthFunc: D3D11_COMPARISON_LESS_EQUAL,
                        StencilEnable: false.into(),
                        StencilReadMask: 0xff,
                        StencilWriteMask: 0xff,
                        FrontFace: D3D11_DEPTH_STENCILOP_DESC {
                            StencilFailOp: D3D11_STENCIL_OP_KEEP,
                            StencilDepthFailOp: D3D11_STENCIL_OP_INCR,
                            StencilPassOp: D3D11_STENCIL_OP_KEEP,
                            StencilFunc: D3D11_COMPARISON_ALWAYS,
                        },
                        BackFace: D3D11_DEPTH_STENCILOP_DESC {
                            StencilFailOp: D3D11_STENCIL_OP_KEEP,
                            StencilDepthFailOp: D3D11_STENCIL_OP_DECR,
                            StencilPassOp: D3D11_STENCIL_OP_KEEP,
                            StencilFunc: D3D11_COMPARISON_ALWAYS,
                        },
                    },
                    Some(&mut state),
                )
                .context("Failed to create depth stencil state")?
        };
        let state = state.unwrap();

        let mut views = vec![];

        for i in 0..layers {
            let mut view = None;
            unsafe {
                device
                    .CreateDepthStencilView(
                        &texture,
                        Some(&D3D11_DEPTH_STENCIL_VIEW_DESC {
                            Format: DXGI_FORMAT_D32_FLOAT,
                            ViewDimension: D3D11_DSV_DIMENSION_TEXTURE2DARRAY,
                            Flags: 0,
                            Anonymous: D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                                Texture2DArray: {
                                    D3D11_TEX2D_ARRAY_DSV {
                                        MipSlice: 0,
                                        ArraySize: 1,
                                        FirstArraySlice: i as u32,
                                    }
                                },
                            },
                        }),
                        Some(&mut view),
                    )
                    .context("Failed to create depth stencil view")?
            };

            views.push(view.unwrap());
        }

        let mut texture_view = None;
        unsafe {
            device.CreateShaderResourceView(
                &texture,
                Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_R32_FLOAT,
                    ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
                    Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2DArray: D3D11_TEX2D_ARRAY_SRV {
                            MostDetailedMip: 0,
                            MipLevels: 1,
                            FirstArraySlice: 0,
                            ArraySize: layers as u32,
                        },
                    },
                }),
                Some(&mut texture_view),
            )?
        };
        let texture_view = texture_view.unwrap();

        Ok(Self {
            texture,
            state,
            views,
            texture_view,
            layers,
        })
    }

    pub fn resize(&mut self, new_size: (u32, u32), device: &ID3D11Device) -> anyhow::Result<()> {
        *self = Self::create(new_size, self.layers, device)?;
        Ok(())
    }
}
