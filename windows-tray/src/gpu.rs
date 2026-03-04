use anyhow::{anyhow, Context};
use crate::log::log_line;
use std::mem::size_of;
use windows::core::{ComInterface, PCSTR};
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, ID3DInclude, D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_10_0,
    D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Buffer, ID3D11ClassLinkage, ID3D11Device, ID3D11DeviceContext,
    ID3D11InputLayout, ID3D11PixelShader, ID3D11RenderTargetView, ID3D11SamplerState,
    ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER,
    D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER, D3D11_BUFFER_DESC,
    D3D11_COMPARISON_NEVER, D3D11_CPU_ACCESS_FLAG, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_FLOAT32_MAX, D3D11_INPUT_ELEMENT_DESC,
    D3D11_INPUT_PER_VERTEX_DATA, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_ALPHA_MODE_UNSPECIFIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM,
    DXGI_FORMAT_R32G32_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIAdapter, IDXGIDevice, IDXGIFactory2, IDXGIOutput, IDXGISwapChain1, DXGI_SCALING_NONE,
    DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_DISCARD,
    DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_SWAP_EFFECT_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrtProfile {
    Off,
    Lite,
    Full,
}

impl CrtProfile {
    pub fn from_settings(value: &str) -> Self {
        match value {
            "lite" => Self::Lite,
            "full" => Self::Full,
            _ => Self::Off,
        }
    }

    fn as_f32(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Lite => 1.0,
            Self::Full => 2.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ShaderParams {
    resolution: [f32; 2],
    profile: f32,
    shutdown: f32,
    time_sec: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

pub struct GpuPresenter {
    hwnd: HWND,
    width: u32,
    height: u32,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    backbuffer_rtv: Option<ID3D11RenderTargetView>,
    scene_tex: ID3D11Texture2D,
    scene_srv: ID3D11ShaderResourceView,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    input_layout: ID3D11InputLayout,
    vertex_buffer: ID3D11Buffer,
    constant_buffer: ID3D11Buffer,
    sampler: ID3D11SamplerState,
    clock_started_ms: i64,
}

pub fn probe_hardware() -> anyhow::Result<()> {
    log_line("gpu probe: start");
    let (_device, _context) = create_device().context("create D3D11 device")?;
    log_line("gpu probe: success");
    Ok(())
}

impl GpuPresenter {
    pub fn new(hwnd: HWND, width: i32, height: i32) -> anyhow::Result<Self> {
        if width <= 0 || height <= 0 {
            return Err(anyhow!("Invalid popup size for GPU renderer"));
        }

        let (device, context) = create_device().context("create D3D11 device")?;
        let swap_chain = create_swap_chain(hwnd, &device, width as u32, height as u32)
            .context("create swap chain")?;
        let backbuffer_rtv =
            create_backbuffer_rtv(&device, &swap_chain).context("create backbuffer RTV")?;
        let (scene_tex, scene_srv) = create_scene_texture(&device, width as u32, height as u32)
            .context("create scene texture")?;
        let (vertex_shader, pixel_shader, input_layout) =
            create_shaders(&device).context("create CRT shaders")?;
        let vertex_buffer = create_vertex_buffer(&device).context("create vertex buffer")?;
        let constant_buffer = create_constant_buffer(&device).context("create constant buffer")?;
        let sampler = create_sampler(&device).context("create sampler")?;
        let clock_started_ms = now_epoch_ms();

        Ok(Self {
            hwnd,
            width: width as u32,
            height: height as u32,
            device,
            context,
            swap_chain,
            backbuffer_rtv: Some(backbuffer_rtv),
            scene_tex,
            scene_srv,
            vertex_shader,
            pixel_shader,
            input_layout,
            vertex_buffer,
            constant_buffer,
            sampler,
            clock_started_ms,
        })
    }

    pub fn ensure_size(&mut self, width: i32, height: i32) -> anyhow::Result<()> {
        if width <= 0 || height <= 0 {
            return Err(anyhow!("Invalid popup size"));
        }
        let width_u = width as u32;
        let height_u = height as u32;
        if self.width == width_u && self.height == height_u {
            return Ok(());
        }

        self.width = width_u;
        self.height = height_u;

        unsafe {
            self.context.OMSetRenderTargets(
                None,
                None::<&windows::Win32::Graphics::Direct3D11::ID3D11DepthStencilView>,
            );
            // Release all pipeline references before resizing the swapchain.
            self.context.PSSetShaderResources(0, Some(&[None]));
            self.context.ClearState();
            self.context.Flush();
        }
        self.backbuffer_rtv = None;
        self.backbuffer_rtv = Some(
            create_backbuffer_rtv_after_resize(&self.device, &self.swap_chain, width_u, height_u)
                .context("resize backbuffer RTV")?,
        );
        let (scene_tex, scene_srv) = create_scene_texture(&self.device, width_u, height_u)
            .context("resize scene texture")?;
        self.scene_tex = scene_tex;
        self.scene_srv = scene_srv;
        Ok(())
    }

    pub fn render_bgra_frame(
        &mut self,
        frame_bgra: &[u8],
        frame_width: i32,
        frame_height: i32,
        profile: CrtProfile,
        shutdown_progress: f32,
    ) -> anyhow::Result<()> {
        self.ensure_size(frame_width, frame_height)?;
        let expected = (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(4);
        if frame_bgra.len() < expected {
            return Err(anyhow!("GPU frame buffer too small"));
        }

        unsafe {
            self.context.UpdateSubresource(
                &self.scene_tex,
                0,
                None,
                frame_bgra.as_ptr() as *const _,
                self.width * 4,
                0,
            );

            let params = ShaderParams {
                resolution: [self.width as f32, self.height as f32],
                profile: profile.as_f32(),
                shutdown: shutdown_progress.clamp(0.0, 1.0),
                time_sec: (now_epoch_ms().saturating_sub(self.clock_started_ms) as f32) / 1000.0,
                _pad: [0.0; 3],
            };
            self.context.UpdateSubresource(
                &self.constant_buffer,
                0,
                None,
                &params as *const ShaderParams as *const _,
                0,
                0,
            );

            let backbuffer = self
                .backbuffer_rtv
                .as_ref()
                .ok_or_else(|| anyhow!("Missing backbuffer RTV"))?;
            self.context.OMSetRenderTargets(
                Some(&[Some(backbuffer.clone())]),
                None::<&windows::Win32::Graphics::Direct3D11::ID3D11DepthStencilView>,
            );
            let viewport = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.context.RSSetViewports(Some(&[viewport]));

            self.context
                .ClearRenderTargetView(backbuffer, &[0.0, 0.0, 0.0, 1.0]);

            self.context.IASetInputLayout(&self.input_layout);
            let stride = size_of::<Vertex>() as u32;
            let offset = 0u32;
            let vb = Some(self.vertex_buffer.clone());
            self.context.IASetVertexBuffers(
                0,
                1,
                Some(&vb as *const _),
                Some(&stride as *const _),
                Some(&offset as *const _),
            );
            self.context
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            self.context.VSSetShader(&self.vertex_shader, None);
            self.context.PSSetShader(&self.pixel_shader, None);
            self.context
                .PSSetShaderResources(0, Some(&[Some(self.scene_srv.clone())]));
            self.context
                .PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            self.context
                .VSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.clone())]));
            self.context
                .PSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.clone())]));
            self.context.Draw(6, 0);

            self.swap_chain
                .Present(1, 0)
                .ok()
                .context("swap chain present")?;
        }

        Ok(())
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

fn create_device() -> anyhow::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let feature_levels: [D3D_FEATURE_LEVEL; 2] = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
    let attempt = |flags| -> anyhow::Result<(ID3D11Device, ID3D11DeviceContext)> {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut selected_feature_level = D3D_FEATURE_LEVEL_10_0;
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(0),
                flags,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut selected_feature_level),
                Some(&mut context),
            )
            .context("D3D11CreateDevice failed")?;
        }
        let device = device.ok_or_else(|| anyhow!("D3D11CreateDevice returned no device"))?;
        let context = context.ok_or_else(|| anyhow!("D3D11CreateDevice returned no context"))?;
        Ok((device, context))
    };

    match attempt(D3D11_CREATE_DEVICE_BGRA_SUPPORT) {
        Ok(pair) => Ok(pair),
        Err(primary) => {
            log_line(&format!(
                "gpu create_device with BGRA flag failed: {:#}",
                primary
            ));
            attempt(windows::Win32::Graphics::Direct3D11::D3D11_CREATE_DEVICE_FLAG(0)).map_err(
                |secondary| {
                    anyhow!(
                        "D3D11 device creation failed (BGRA attempt + plain attempt)\nBGRA: {:#}\nPlain: {:#}",
                        primary,
                        secondary
                    )
                },
            )
        }
    }
}

fn create_swap_chain(
    hwnd: HWND,
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> anyhow::Result<IDXGISwapChain1> {
    let dxgi_device: IDXGIDevice = device.cast().context("cast ID3D11Device -> IDXGIDevice")?;
    let adapter = unsafe { dxgi_device.GetAdapter() }.context("IDXGIDevice::GetAdapter failed")?;
    let factory: IDXGIFactory2 =
        unsafe { adapter.GetParent() }.context("Get DXGI factory failed")?;
    let _ = unsafe { factory.MakeWindowAssociation(hwnd, 2) };

    let base_desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: width,
        Height: height,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Stereo: false.into(),
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_IGNORE,
        Flags: 0,
    };

    let mut attempts: Vec<(&str, DXGI_SWAP_CHAIN_DESC1)> = Vec::new();
    attempts.push(("flip-sequential/stretch/alpha-ignore", base_desc));
    let mut desc2 = base_desc;
    desc2.Scaling = DXGI_SCALING_NONE;
    attempts.push(("flip-sequential/none/alpha-ignore", desc2));
    let mut desc3 = base_desc;
    desc3.SwapEffect = DXGI_SWAP_EFFECT_SEQUENTIAL;
    desc3.AlphaMode = DXGI_ALPHA_MODE_UNSPECIFIED;
    attempts.push(("sequential/stretch/alpha-unspecified", desc3));
    let mut desc4 = base_desc;
    desc4.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;
    desc4.BufferCount = 1;
    desc4.AlphaMode = DXGI_ALPHA_MODE_UNSPECIFIED;
    attempts.push(("discard/stretch/alpha-unspecified", desc4));

    let mut errors = Vec::new();
    for (label, desc) in attempts {
        match unsafe {
            factory.CreateSwapChainForHwnd(device, hwnd, &desc, None, None::<&IDXGIOutput>)
        } {
            Ok(chain) => {
                log_line(&format!("gpu swapchain create success via {}", label));
                return Ok(chain);
            }
            Err(err) => {
                errors.push(format!("{} => {}", label, err));
            }
        }
    }

    Err(anyhow!(
        "CreateSwapChainForHwnd failed for all variants: {}",
        errors.join(" | ")
    ))
}

fn create_backbuffer_rtv(
    device: &ID3D11Device,
    swap_chain: &IDXGISwapChain1,
) -> anyhow::Result<ID3D11RenderTargetView> {
    let back_buffer: ID3D11Texture2D =
        unsafe { swap_chain.GetBuffer(0) }.context("Get backbuffer failed")?;
    let mut rtv = None;
    unsafe {
        device
            .CreateRenderTargetView(&back_buffer, None, Some(&mut rtv))
            .context("CreateRenderTargetView failed")?;
    }
    rtv.ok_or_else(|| anyhow!("CreateRenderTargetView returned null"))
}

fn create_backbuffer_rtv_after_resize(
    device: &ID3D11Device,
    swap_chain: &IDXGISwapChain1,
    width: u32,
    height: u32,
) -> anyhow::Result<ID3D11RenderTargetView> {
    unsafe {
        swap_chain
            .ResizeBuffers(0, width, height, DXGI_FORMAT(0), 0)
            .context("ResizeBuffers failed")?;
    }
    create_backbuffer_rtv(device, swap_chain)
}

fn create_scene_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> anyhow::Result<(ID3D11Texture2D, ID3D11ShaderResourceView)> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: D3D11_CPU_ACCESS_FLAG(0).0 as u32,
        MiscFlags: 0,
    };
    let mut tex = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .context("CreateTexture2D scene texture failed")?;
    }
    let tex = tex.ok_or_else(|| anyhow!("CreateTexture2D returned null texture"))?;

    let mut srv = None;
    unsafe {
        device
            .CreateShaderResourceView(&tex, None, Some(&mut srv))
            .context("CreateShaderResourceView failed")?;
    }
    let srv = srv.ok_or_else(|| anyhow!("CreateShaderResourceView returned null"))?;
    Ok((tex, srv))
}

fn create_shaders(
    device: &ID3D11Device,
) -> anyhow::Result<(ID3D11VertexShader, ID3D11PixelShader, ID3D11InputLayout)> {
    let vs_blob = compile_shader(VS_SOURCE, "main", "vs_4_0").context("compile VS")?;
    let ps_blob = compile_shader(PS_SOURCE, "main", "ps_4_0").context("compile PS")?;

    let vs_bytes = blob_to_bytes(&vs_blob);
    let ps_bytes = blob_to_bytes(&ps_blob);

    let mut vs = None;
    let mut ps = None;
    unsafe {
        device
            .CreateVertexShader(vs_bytes, None::<&ID3D11ClassLinkage>, Some(&mut vs))
            .context("CreateVertexShader failed")?;
        device
            .CreatePixelShader(ps_bytes, None::<&ID3D11ClassLinkage>, Some(&mut ps))
            .context("CreatePixelShader failed")?;
    }
    let vs = vs.ok_or_else(|| anyhow!("CreateVertexShader returned null"))?;
    let ps = ps.ok_or_else(|| anyhow!("CreatePixelShader returned null"))?;

    let input_desc = [
        D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(b"POSITION\0".as_ptr()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(b"TEXCOORD\0".as_ptr()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 8,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
    ];
    let mut input_layout = None;
    unsafe {
        device
            .CreateInputLayout(&input_desc, vs_bytes, Some(&mut input_layout))
            .context("CreateInputLayout failed")?;
    }
    let input_layout = input_layout.ok_or_else(|| anyhow!("CreateInputLayout returned null"))?;
    Ok((vs, ps, input_layout))
}

fn create_vertex_buffer(device: &ID3D11Device) -> anyhow::Result<ID3D11Buffer> {
    let vertices: [Vertex; 6] = [
        Vertex {
            pos: [-1.0, 1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            pos: [1.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            pos: [-1.0, -1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            pos: [-1.0, -1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            pos: [1.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            pos: [1.0, -1.0],
            uv: [1.0, 1.0],
        },
    ];
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: (size_of::<Vertex>() * vertices.len()) as u32,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
        StructureByteStride: 0,
    };
    let init_data = D3D11_SUBRESOURCE_DATA {
        pSysMem: vertices.as_ptr() as *const _,
        SysMemPitch: 0,
        SysMemSlicePitch: 0,
    };
    let mut buffer = None;
    unsafe {
        device
            .CreateBuffer(&desc, Some(&init_data), Some(&mut buffer))
            .context("CreateBuffer vertex buffer failed")?;
    }
    buffer.ok_or_else(|| anyhow!("CreateBuffer returned null vertex buffer"))
}

fn create_constant_buffer(device: &ID3D11Device) -> anyhow::Result<ID3D11Buffer> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: size_of::<ShaderParams>() as u32,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
        StructureByteStride: 0,
    };
    let mut buffer = None;
    unsafe {
        device
            .CreateBuffer(&desc, None, Some(&mut buffer))
            .context("CreateBuffer constant buffer failed")?;
    }
    buffer.ok_or_else(|| anyhow!("CreateBuffer returned null constant buffer"))
}

fn create_sampler(device: &ID3D11Device) -> anyhow::Result<ID3D11SamplerState> {
    let desc = D3D11_SAMPLER_DESC {
        Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
        AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
        MipLODBias: 0.0,
        MaxAnisotropy: 1,
        ComparisonFunc: D3D11_COMPARISON_NEVER,
        BorderColor: [0.0, 0.0, 0.0, 0.0],
        MinLOD: 0.0,
        MaxLOD: D3D11_FLOAT32_MAX,
    };
    let mut sampler = None;
    unsafe {
        device
            .CreateSamplerState(&desc, Some(&mut sampler))
            .context("CreateSamplerState failed")?;
    }
    sampler.ok_or_else(|| anyhow!("CreateSamplerState returned null"))
}

fn compile_shader(source: &str, entry: &str, target: &str) -> anyhow::Result<ID3DBlob> {
    let mut code = None;
    let mut errors = None;
    let entry_c = format!("{}\0", entry);
    let target_c = format!("{}\0", target);
    unsafe {
        let result = D3DCompile(
            source.as_ptr() as *const _,
            source.len(),
            PCSTR::null(),
            None,
            None::<&ID3DInclude>,
            PCSTR(entry_c.as_ptr()),
            PCSTR(target_c.as_ptr()),
            0,
            0,
            &mut code,
            Some(&mut errors),
        );
        if let Err(err) = result {
            let detail = errors
                .as_ref()
                .map(blob_to_utf8)
                .unwrap_or_else(String::new);
            log_line(&format!(
                "gpu shader compile failed entry={} target={} err={} detail={}",
                entry, target, err, detail
            ));
            return Err(anyhow!("D3DCompile failed: {} {}", err, detail));
        }
    }
    code.ok_or_else(|| anyhow!("D3DCompile returned no shader blob"))
}

fn blob_to_bytes(blob: &ID3DBlob) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize())
    }
}

fn blob_to_utf8(blob: &ID3DBlob) -> String {
    let bytes = blob_to_bytes(blob);
    String::from_utf8_lossy(bytes).to_string()
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_millis() as i64)
        .unwrap_or(0)
}

const VS_SOURCE: &str = r#"
struct VSIn {
    float2 pos : POSITION;
    float2 uv  : TEXCOORD0;
};
struct VSOut {
    float4 pos : SV_POSITION;
    float2 uv  : TEXCOORD0;
};
VSOut main(VSIn input) {
    VSOut o;
    o.pos = float4(input.pos, 0.0, 1.0);
    o.uv = input.uv;
    return o;
}
"#;

const PS_SOURCE: &str = r#"
Texture2D sceneTex : register(t0);
SamplerState sceneSmp : register(s0);

cbuffer Params : register(b0) {
    float2 resolution;
    float profile;
    float shutdown;
    float timeSec;
    float3 _pad;
};

float4 sampleScene(float2 uv) {
    return sceneTex.Sample(sceneSmp, uv);
}

float vignetteFactor(float2 uv, float strength) {
    float2 p = uv * 2.0 - 1.0;
    float d = dot(p, p);
    return saturate(1.0 - d * strength);
}

float scanlineFactor(float2 uv, float intensity) {
    float wave = sin(uv.y * resolution.y * 3.14159);
    return 1.0 - intensity * (0.5 + 0.5 * wave);
}

float3 phosphorMask(float2 uv) {
    float triad = frac(uv.x * resolution.x / 3.0) * 3.0;
    if (triad < 1.0) return float3(1.18, 0.88, 0.88);
    if (triad < 2.0) return float3(0.88, 1.18, 0.88);
    return float3(0.88, 0.88, 1.18);
}

float2 curvatureWarp(float2 uv, float amount) {
    float2 p = uv * 2.0 - 1.0;
    float r2 = dot(p, p);
    p *= (1.0 + amount * r2);
    return p * 0.5 + 0.5;
}

float3 bloomApprox(float2 uv, float strength) {
    float2 px = 1.0 / resolution;
    float3 c = 0;
    c += sampleScene(uv + float2(0.0, -2.0) * px).rgb;
    c += sampleScene(uv + float2(-2.0, 0.0) * px).rgb;
    c += sampleScene(uv).rgb * 2.0;
    c += sampleScene(uv + float2(2.0, 0.0) * px).rgb;
    c += sampleScene(uv + float2(0.0, 2.0) * px).rgb;
    c /= 6.0;
    return c * strength;
}

float4 main(float4 pos : SV_POSITION, float2 uvIn : TEXCOORD0) : SV_TARGET {
    float2 uv = uvIn;
    if (profile >= 2.0) {
        uv = curvatureWarp(uv, 0.07);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            return float4(0.0, 0.0, 0.0, 1.0);
        }
    }

    float shutdownP = saturate(shutdown);
    if (profile >= 2.0 && shutdownP > 0.001) {
        float center = 0.5;
        float halfBand = max(0.005, 0.5 * (1.0 - shutdownP * 0.97));
        if (abs(uv.y - center) > halfBand) {
            return float4(0.0, 0.0, 0.0, 1.0);
        }
    }

    float3 color;
    if (profile >= 2.0) {
        float2 px = 1.0 / resolution;
        float shift = 1.0 + shutdownP * 1.5;
        float r = sampleScene(uv + float2(shift * px.x, 0)).r;
        float g = sampleScene(uv).g;
        float b = sampleScene(uv - float2(shift * px.x, 0)).b;
        color = float3(r, g, b);
        color += bloomApprox(uv, 0.36);
        color *= phosphorMask(uv);
        color *= scanlineFactor(uv, 0.20);
        color *= vignetteFactor(uv, 0.10);
        color += shutdownP * 0.10;
    } else if (profile >= 1.0) {
        color = sampleScene(uv).rgb;
        color += bloomApprox(uv, 0.17);
        color *= scanlineFactor(uv, 0.14);
        color *= vignetteFactor(uv, 0.06);
    } else {
        color = sampleScene(uv).rgb;
    }

    return float4(saturate(color), 1.0);
}
"#;
