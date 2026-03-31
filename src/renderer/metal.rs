use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLClearColor, MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLResourceOptions, MTLStoreAction, MTLViewport,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use crate::renderer::atlas::GlyphAtlas;
use crate::renderer::cells::{Quad, RenderState, build_scene_quads, layout_metrics};

const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct Vertex {
    float4 position;
    float4 color;
};

struct RasterizerData {
    float4 position [[position]];
    float4 color;
};

vertex RasterizerData vertex_main(const device Vertex* vertices [[buffer(0)]], uint vertex_id [[vertex_id]]) {
    Vertex input_vertex = vertices[vertex_id];
    RasterizerData out;
    out.position = float4(input_vertex.position);
    out.color = input_vertex.color;
    return out;
}

fragment float4 fragment_main(RasterizerData in [[stage_in]]) {
    return in.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Vertex {
    position: [f32; 4],
    color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct RenderFrameInput {
    pub view_width: f64,
    pub view_height: f64,
    pub pixel_width: f64,
    pub pixel_height: f64,
    pub scale_factor: f64,
}

pub struct TerminalRenderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    layer: Retained<CAMetalLayer>,
    _atlas: GlyphAtlas,
    drawable_width: f64,
    drawable_height: f64,
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

        let vertex_name = NSString::from_str("vertex_main");
        let fragment_name = NSString::from_str("fragment_main");
        let vertex_function = library
            .newFunctionWithName(&vertex_name)
            .ok_or("missing Metal vertex function")?;
        let fragment_function = library
            .newFunctionWithName(&fragment_name)
            .ok_or("missing Metal fragment function")?;

        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&*vertex_function));
        descriptor.setFragmentFunction(Some(&*fragment_function));
        let attachments = descriptor.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);

        let pipeline_state = device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|error| format!("failed to create Metal pipeline state: {}", error))?;

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
            pipeline_state,
            layer,
            _atlas: GlyphAtlas,
            drawable_width: 0.0,
            drawable_height: 0.0,
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

        if (self.drawable_width - drawable_width).abs() < f64::EPSILON
            && (self.drawable_height - drawable_height).abs() < f64::EPSILON
        {
            return;
        }

        self.drawable_width = drawable_width;
        self.drawable_height = drawable_height;
        self.layer
            .setDrawableSize(objc2_foundation::NSSize::new(drawable_width, drawable_height));
    }

    pub fn render(&mut self, input: RenderFrameInput, state: &RenderState) -> Result<(), String> {
        self.resize(
            input.view_width,
            input.view_height,
            input.pixel_width,
            input.pixel_height,
            input.scale_factor,
        );

        let Some(drawable) = self.layer.nextDrawable() else {
            return Ok(());
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

        encoder.setRenderPipelineState(&*self.pipeline_state);
        encoder.setViewport(MTLViewport {
            originX: 0.0,
            originY: 0.0,
            width: self.drawable_width,
            height: self.drawable_height,
            znear: 0.0,
            zfar: 1.0,
        });

        let metrics = layout_metrics(input.view_width, input.view_height, state);
        let quads = build_scene_quads(metrics, state);
        let vertices = build_scene_vertices(metrics.view_width, metrics.view_height, &quads);
        let byte_len = vertices.len() * std::mem::size_of::<Vertex>();
        let bytes = NonNull::new(vertices.as_ptr().cast_mut().cast::<c_void>())
            .ok_or("failed to prepare Metal vertex bytes")?;
        let vertex_buffer = unsafe {
            self.device.newBufferWithBytes_length_options(
                bytes,
                byte_len,
                MTLResourceOptions::empty(),
            )
        }
        .ok_or("failed to create Metal vertex buffer")?;

        unsafe {
            encoder.setVertexBuffer_offset_atIndex(Some(&*vertex_buffer), 0, 0);
            encoder.drawPrimitives_vertexStart_vertexCount(
                MTLPrimitiveType::Triangle,
                0,
                vertices.len(),
            );
        }
        encoder.endEncoding();
        let _: () = unsafe { msg_send![&*command_buffer, presentDrawable: &*drawable] };
        command_buffer.commit();

        Ok(())
    }

    pub fn device_name(&self) -> String {
        self.device.name().to_string()
    }
}

fn build_scene_vertices(view_width: f32, view_height: f32, quads: &[Quad]) -> Vec<Vertex> {
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
    vertices: &mut Vec<Vertex>,
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
        Vertex {
            position: [left, top, 0.0, 1.0],
            color,
        },
        Vertex {
            position: [right, top, 0.0, 1.0],
            color,
        },
        Vertex {
            position: [left, bottom, 0.0, 1.0],
            color,
        },
        Vertex {
            position: [right, top, 0.0, 1.0],
            color,
        },
        Vertex {
            position: [right, bottom, 0.0, 1.0],
            color,
        },
        Vertex {
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
