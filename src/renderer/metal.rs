use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlendOperation, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder,
    MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice, MTLLibrary, MTLLoadAction,
    MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResourceOptions, MTLSamplerAddressMode,
    MTLSamplerDescriptor, MTLSamplerMinMagFilter, MTLSamplerState, MTLStoreAction, MTLViewport,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use crate::layout::{LayoutMetrics, layout_metrics};
use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::cells::{
    Quad, RenderSnapshot, RowGeometry, SelectionRange, TextInstance, build_chrome_quads,
    build_cursor_quad, build_row_geometry,
};

const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct SolidVertex {
    float4 position;
    float4 color;
};

struct SolidRasterizerData {
    float4 position [[position]];
    float4 color;
};

vertex SolidRasterizerData solid_vertex_main(const device SolidVertex* vertices [[buffer(0)]], uint vertex_id [[vertex_id]]) {
    SolidVertex input_vertex = vertices[vertex_id];
    SolidRasterizerData out;
    out.position = input_vertex.position;
    out.color = input_vertex.color;
    return out;
}

fragment float4 solid_fragment_main(SolidRasterizerData in [[stage_in]]) {
    return in.color;
}

struct TextInstance {
    float2 origin;
    float2 size;
    float2 uv_origin;
    float2 uv_size;
    float4 color;
};

struct TextUniforms {
    float2 view_size;
};

struct TextRasterizerData {
    float4 position [[position]];
    float2 uv;
    float4 color;
};

vertex TextRasterizerData text_vertex_main(
    const device TextInstance* instances [[buffer(0)]],
    constant TextUniforms& uniforms [[buffer(1)]],
    uint vertex_id [[vertex_id]],
    uint instance_id [[instance_id]]
) {
    TextInstance instance = instances[instance_id];
    float2 quad;
    switch (vertex_id) {
        case 0: quad = float2(0.0, 0.0); break;
        case 1: quad = float2(1.0, 0.0); break;
        case 2: quad = float2(0.0, 1.0); break;
        case 3: quad = float2(1.0, 0.0); break;
        case 4: quad = float2(1.0, 1.0); break;
        default: quad = float2(0.0, 1.0); break;
    }
    float2 pixel = instance.origin + (quad * instance.size);
    float2 ndc = float2((pixel.x / uniforms.view_size.x) * 2.0 - 1.0,
                        1.0 - (pixel.y / uniforms.view_size.y) * 2.0);

    TextRasterizerData out;
    out.position = float4(ndc, 0.0, 1.0);
    out.uv = instance.uv_origin + (quad * instance.uv_size);
    out.color = instance.color;
    return out;
}

fragment float4 text_fragment_main(
    TextRasterizerData in [[stage_in]],
    texture2d<float> atlas [[texture(0)]],
    sampler atlas_sampler [[sampler(0)]]
) {
    float coverage = atlas.sample(atlas_sampler, in.uv).r;
    return float4(in.color.rgb, in.color.a * coverage);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct SolidVertex {
    position: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct TextUniforms {
    view_size: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderFrameInput {
    pub view_width: f64,
    pub view_height: f64,
    pub pixel_width: f64,
    pub pixel_height: f64,
    pub scale_factor: f64,
    pub cursor_visible: bool,
    pub selection: Option<SelectionRange>,
}

pub struct TerminalRenderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn objc2_metal::MTLCommandQueue>>,
    solid_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    text_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    text_sampler: Retained<ProtocolObject<dyn MTLSamplerState>>,
    layer: Retained<CAMetalLayer>,
    atlas: GlyphAtlas,
    drawable_width: f64,
    drawable_height: f64,
    chrome_quads: Vec<Quad>,
    row_caches: Vec<RowGeometry>,
    cached_cols: u16,
    cached_rows: u16,
    last_selection: Option<SelectionRange>,
}

struct FrameResources {
    drawable: Retained<ProtocolObject<dyn CAMetalDrawable>>,
    command_buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    encoder: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
}

struct DrawBatches {
    background_quads: Vec<Quad>,
    text_instances: Vec<TextInstance>,
    overlay_quads: Vec<Quad>,
}

impl TerminalRenderer {
    pub fn new() -> Result<Self, String> {
        let device =
            MTLCreateSystemDefaultDevice().ok_or("failed to create the default Metal device")?;
        let command_queue = device
            .newCommandQueue()
            .ok_or("failed to create a Metal command queue")?;

        let source = NSString::from_str(SHADER_SOURCE);
        let library = device
            .newLibraryWithSource_options_error(&source, None)
            .map_err(|error| format!("failed to compile Metal shaders: {}", error))?;

        let solid_pipeline_state = build_solid_pipeline(&device, &library)?;
        let text_pipeline_state = build_text_pipeline(&device, &library)?;
        let text_sampler = build_text_sampler(&device)?;
        let atlas = GlyphAtlas::new(&device)?;

        let layer = CAMetalLayer::new();
        layer.setDevice(Some(&*device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setFramebufferOnly(true);
        layer.setPresentsWithTransaction(false);
        layer.setDisplaySyncEnabled(true);
        layer.setAllowsNextDrawableTimeout(false);
        layer.setNeedsDisplayOnBoundsChange(true);

        Ok(Self {
            device,
            command_queue,
            solid_pipeline_state,
            text_pipeline_state,
            text_sampler,
            layer,
            atlas,
            drawable_width: 0.0,
            drawable_height: 0.0,
            chrome_quads: Vec::new(),
            row_caches: Vec::new(),
            cached_cols: 0,
            cached_rows: 0,
            last_selection: None,
        })
    }

    pub fn layer(&self) -> &CAMetalLayer {
        &self.layer
    }

    pub fn resize(
        &mut self,
        view_width: f64,
        view_height: f64,
        width_px: f64,
        height_px: f64,
        scale_factor: f64,
    ) {
        let drawable_width = width_px.max(1.0).round();
        let drawable_height = height_px.max(1.0).round();
        self.layer.setFrame(objc2_foundation::NSRect::new(
            objc2_foundation::NSPoint::new(0.0, 0.0),
            objc2_foundation::NSSize::new(view_width.max(1.0), view_height.max(1.0)),
        ));
        self.layer.setContentsScale(scale_factor.max(1.0));

        if (self.drawable_width - drawable_width).abs() >= f64::EPSILON
            || (self.drawable_height - drawable_height).abs() >= f64::EPSILON
        {
            self.drawable_width = drawable_width;
            self.drawable_height = drawable_height;
            self.layer.setDrawableSize(objc2_foundation::NSSize::new(
                drawable_width,
                drawable_height,
            ));
        }
    }

    pub fn render(
        &mut self,
        input: RenderFrameInput,
        snapshot: &RenderSnapshot,
    ) -> Result<(), String> {
        self.update_drawable(&input);

        let Some(frame) = self.prepare_frame_resources()? else {
            return Ok(());
        };

        let metrics = self.update_layout_caches(&input, snapshot);
        let batches = self.collect_draw_batches(metrics, snapshot, input.cursor_visible);
        self.encode_draw_batches(&frame.encoder, metrics, &batches)?;
        self.present_frame(frame);
        Ok(())
    }

    pub fn device_name(&self) -> String {
        self.device.name().to_string()
    }

    fn update_caches(
        &mut self,
        metrics: LayoutMetrics,
        snapshot: &RenderSnapshot,
        selection: Option<SelectionRange>,
    ) {
        let dims_changed = self.cached_cols != snapshot.cols || self.cached_rows != snapshot.rows;
        if dims_changed
            || snapshot.damage.full_rebuild
            || self.row_caches.len() != snapshot.rows as usize
        {
            self.chrome_quads = build_chrome_quads(metrics);
            self.row_caches = (0..snapshot.rows)
                .map(|row| build_row_geometry(metrics, snapshot, &self.atlas, row, selection))
                .collect();
        } else {
            if selection != self.last_selection || snapshot.damage.selection_dirty {
                for row in 0..snapshot.rows {
                    self.row_caches[row as usize] =
                        build_row_geometry(metrics, snapshot, &self.atlas, row, selection);
                }
            } else {
                for &row in &snapshot.damage.dirty_rows {
                    if row < snapshot.rows {
                        self.row_caches[row as usize] =
                            build_row_geometry(metrics, snapshot, &self.atlas, row, selection);
                    }
                }
            }
            if snapshot.damage.global_dirty {
                self.chrome_quads = build_chrome_quads(metrics);
            }
        }

        self.cached_cols = snapshot.cols;
        self.cached_rows = snapshot.rows;
        self.last_selection = selection;
    }

    fn update_drawable(&mut self, input: &RenderFrameInput) {
        self.resize(
            input.view_width,
            input.view_height,
            input.pixel_width,
            input.pixel_height,
            input.scale_factor,
        );
    }

    fn prepare_frame_resources(&self) -> Result<Option<FrameResources>, String> {
        let Some(drawable) = self.layer.nextDrawable() else {
            return Ok(None);
        };

        let pass_descriptor = MTLRenderPassDescriptor::renderPassDescriptor();
        let color_attachments = pass_descriptor.colorAttachments();
        let color_attachment = unsafe { color_attachments.objectAtIndexedSubscript(0) };
        color_attachment.setTexture(Some(&*drawable.texture()));
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setStoreAction(MTLStoreAction::Store);
        color_attachment.setClearColor(MTLClearColor {
            red: 0.05,
            green: 0.06,
            blue: 0.08,
            alpha: 1.0,
        });

        let command_buffer = self
            .command_queue
            .commandBuffer()
            .ok_or("failed to create a Metal command buffer")?;
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&pass_descriptor)
            .ok_or("failed to create a Metal render encoder")?;

        encoder.setViewport(MTLViewport {
            originX: 0.0,
            originY: 0.0,
            width: self.drawable_width,
            height: self.drawable_height,
            znear: 0.0,
            zfar: 1.0,
        });

        Ok(Some(FrameResources {
            drawable,
            command_buffer,
            encoder,
        }))
    }

    fn update_layout_caches(
        &mut self,
        input: &RenderFrameInput,
        snapshot: &RenderSnapshot,
    ) -> LayoutMetrics {
        let metrics = layout_metrics(
            input.view_width,
            input.view_height,
            snapshot.cols,
            snapshot.rows,
        );
        self.update_caches(metrics, snapshot, input.selection);
        metrics
    }

    fn collect_draw_batches(
        &self,
        metrics: LayoutMetrics,
        snapshot: &RenderSnapshot,
        cursor_visible: bool,
    ) -> DrawBatches {
        let mut background_quads = self.chrome_quads.clone();
        let mut overlay_quads = Vec::new();
        let mut text_instances = Vec::new();

        for row_cache in &self.row_caches {
            background_quads.extend_from_slice(&row_cache.background_quads);
            overlay_quads.extend_from_slice(&row_cache.overlay_quads);
            text_instances.extend_from_slice(&row_cache.text_instances);
        }

        if let Some(cursor_quad) = build_cursor_quad(metrics, snapshot, cursor_visible) {
            overlay_quads.push(cursor_quad);
        }

        DrawBatches {
            background_quads,
            text_instances,
            overlay_quads,
        }
    }

    fn encode_draw_batches(
        &self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        metrics: LayoutMetrics,
        batches: &DrawBatches,
    ) -> Result<(), String> {
        draw_solid_pass(
            &self.device,
            encoder,
            &self.solid_pipeline_state,
            metrics.view_width,
            metrics.view_height,
            &batches.background_quads,
        )?;
        draw_text_pass(
            &self.device,
            encoder,
            &self.text_pipeline_state,
            &self.text_sampler,
            &self.atlas,
            metrics.view_width,
            metrics.view_height,
            &batches.text_instances,
        )?;
        draw_solid_pass(
            &self.device,
            encoder,
            &self.solid_pipeline_state,
            metrics.view_width,
            metrics.view_height,
            &batches.overlay_quads,
        )?;
        Ok(())
    }

    fn present_frame(&self, frame: FrameResources) {
        frame.encoder.endEncoding();
        let _: () = unsafe { msg_send![&*frame.command_buffer, presentDrawable: &*frame.drawable] };
        frame.command_buffer.commit();
    }
}

fn build_solid_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>, String> {
    let vertex_name = NSString::from_str("solid_vertex_main");
    let fragment_name = NSString::from_str("solid_fragment_main");
    let vertex_function = library
        .newFunctionWithName(&vertex_name)
        .ok_or("missing solid Metal vertex function")?;
    let fragment_function = library
        .newFunctionWithName(&fragment_name)
        .ok_or("missing solid Metal fragment function")?;

    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setVertexFunction(Some(&*vertex_function));
    descriptor.setFragmentFunction(Some(&*fragment_function));
    let attachments = descriptor.colorAttachments();
    let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
    attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);

    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .map_err(|error| format!("failed to create solid Metal pipeline state: {}", error))
}

fn build_text_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>, String> {
    let vertex_name = NSString::from_str("text_vertex_main");
    let fragment_name = NSString::from_str("text_fragment_main");
    let vertex_function = library
        .newFunctionWithName(&vertex_name)
        .ok_or("missing text Metal vertex function")?;
    let fragment_function = library
        .newFunctionWithName(&fragment_name)
        .ok_or("missing text Metal fragment function")?;

    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setVertexFunction(Some(&*vertex_function));
    descriptor.setFragmentFunction(Some(&*fragment_function));
    let attachments = descriptor.colorAttachments();
    let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
    attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    attachment.setBlendingEnabled(true);
    attachment.setSourceRGBBlendFactor(MTLBlendFactor::SourceAlpha);
    attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    attachment.setRgbBlendOperation(MTLBlendOperation::Add);
    attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
    attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    attachment.setAlphaBlendOperation(MTLBlendOperation::Add);

    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .map_err(|error| format!("failed to create text Metal pipeline state: {}", error))
}

fn build_text_sampler(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Result<Retained<ProtocolObject<dyn MTLSamplerState>>, String> {
    let descriptor = MTLSamplerDescriptor::new();
    descriptor.setMinFilter(MTLSamplerMinMagFilter::Linear);
    descriptor.setMagFilter(MTLSamplerMinMagFilter::Linear);
    descriptor.setSAddressMode(MTLSamplerAddressMode::ClampToEdge);
    descriptor.setTAddressMode(MTLSamplerAddressMode::ClampToEdge);
    descriptor.setRAddressMode(MTLSamplerAddressMode::ClampToEdge);

    device
        .newSamplerStateWithDescriptor(&descriptor)
        .ok_or("failed to create text sampler".to_string())
}

fn draw_solid_pass(
    device: &ProtocolObject<dyn MTLDevice>,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    pipeline_state: &ProtocolObject<dyn MTLRenderPipelineState>,
    view_width: f32,
    view_height: f32,
    quads: &[Quad],
) -> Result<(), String> {
    if quads.is_empty() {
        return Ok(());
    }
    let vertices = build_scene_vertices(view_width, view_height, quads);
    let vertex_buffer = make_buffer(device, &vertices)?;
    encoder.setRenderPipelineState(pipeline_state);
    unsafe {
        encoder.setVertexBuffer_offset_atIndex(Some(&*vertex_buffer), 0, 0);
        encoder.drawPrimitives_vertexStart_vertexCount(
            MTLPrimitiveType::Triangle,
            0,
            vertices.len(),
        );
    }
    Ok(())
}

fn draw_text_pass(
    device: &ProtocolObject<dyn MTLDevice>,
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    pipeline_state: &ProtocolObject<dyn MTLRenderPipelineState>,
    sampler: &ProtocolObject<dyn MTLSamplerState>,
    atlas: &GlyphAtlas,
    view_width: f32,
    view_height: f32,
    instances: &[TextInstance],
) -> Result<(), String> {
    if instances.is_empty() {
        return Ok(());
    }

    let instance_buffer = make_buffer(device, instances)?;
    let uniforms = TextUniforms {
        view_size: [view_width, view_height],
    };

    encoder.setRenderPipelineState(pipeline_state);
    unsafe {
        encoder.setVertexBuffer_offset_atIndex(Some(&*instance_buffer), 0, 0);
        encoder.setVertexBytes_length_atIndex(
            NonNull::from(&uniforms).cast::<c_void>(),
            std::mem::size_of::<TextUniforms>(),
            1,
        );
        encoder.setFragmentTexture_atIndex(Some(atlas.texture()), 0);
        encoder.setFragmentSamplerState_atIndex(Some(sampler), 0);
        encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
            MTLPrimitiveType::Triangle,
            0,
            6,
            instances.len(),
        );
    }
    Ok(())
}

fn make_buffer<T>(
    device: &ProtocolObject<dyn MTLDevice>,
    data: &[T],
) -> Result<Retained<ProtocolObject<dyn objc2_metal::MTLBuffer>>, String> {
    let byte_len = std::mem::size_of_val(data);
    let bytes = NonNull::new(data.as_ptr().cast_mut().cast::<c_void>())
        .ok_or("failed to prepare Metal buffer bytes")?;
    unsafe {
        device.newBufferWithBytes_length_options(bytes, byte_len, MTLResourceOptions::empty())
    }
    .ok_or("failed to create Metal buffer".to_string())
}

fn build_scene_vertices(view_width: f32, view_height: f32, quads: &[Quad]) -> Vec<SolidVertex> {
    let mut vertices = Vec::with_capacity(quads.len() * 6);
    for quad in quads {
        push_rect(
            &mut vertices,
            quad.x,
            quad.y,
            quad.width,
            quad.height,
            view_width,
            view_height,
            quad.color.0,
        );
    }
    vertices
}

fn push_rect(
    vertices: &mut Vec<SolidVertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    view_width: f32,
    view_height: f32,
    color: [f32; 4],
) {
    let left = to_ndc_x(x, view_width);
    let right = to_ndc_x(x + width, view_width);
    let top = to_ndc_y(y, view_height);
    let bottom = to_ndc_y(y + height, view_height);

    vertices.extend_from_slice(&[
        SolidVertex {
            position: [left, top, 0.0, 1.0],
            color,
        },
        SolidVertex {
            position: [right, top, 0.0, 1.0],
            color,
        },
        SolidVertex {
            position: [left, bottom, 0.0, 1.0],
            color,
        },
        SolidVertex {
            position: [right, top, 0.0, 1.0],
            color,
        },
        SolidVertex {
            position: [right, bottom, 0.0, 1.0],
            color,
        },
        SolidVertex {
            position: [left, bottom, 0.0, 1.0],
            color,
        },
    ]);
}

fn to_ndc_x(x: f32, width: f32) -> f32 {
    (x / width) * 2.0 - 1.0
}

fn to_ndc_y(y: f32, height: f32) -> f32 {
    1.0 - (y / height) * 2.0
}
