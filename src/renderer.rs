use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::msg_send;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLClearColor, MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLStoreAction, MTLViewport,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

const OUTER_PADDING_X: f64 = 26.0;
const OUTER_PADDING_Y: f64 = 24.0;
const HEADER_HEIGHT: f64 = 36.0;
const GRID_PADDING_X: f64 = 18.0;
const GRID_PADDING_Y: f64 = 18.0;
const CELL_WIDTH: f64 = 16.0;
const CELL_HEIGHT: f64 = 24.0;
    
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
    // padding: [f32; 2],
    color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct RenderFrameInput {
    pub view_width: f64,
    pub view_height: f64,
    pub pixel_width: f64,
    pub pixel_height: f64,
    pub scale_factor: f64,
    pub terminal_cols: u16,
    pub terminal_rows: u16,
    pub pty_activity: u64,
}

pub struct TerminalRenderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    layer: Retained<CAMetalLayer>,
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

    pub fn render(&mut self, input: RenderFrameInput) -> Result<(), String> {
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

        let vertices = build_scene_vertices(input);
        let byte_len = vertices.len() * std::mem::size_of::<Vertex>();
        let bytes = NonNull::new(vertices.as_ptr().cast_mut().cast::<c_void>())
            .ok_or("failed to prepare Metal vertex bytes")?;

        unsafe {
            encoder.setVertexBytes_length_atIndex(
                bytes,
                byte_len,
                0,
            );
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

pub fn terminal_grid_size(view_width: f64, view_height: f64) -> (u16, u16) {
    let inner_width = (view_width - (OUTER_PADDING_X * 2.0) - (GRID_PADDING_X * 2.0)).max(CELL_WIDTH);
    let inner_height = (view_height
        - (OUTER_PADDING_Y * 2.0)
        - HEADER_HEIGHT
        - (GRID_PADDING_Y * 2.0))
        .max(CELL_HEIGHT);

    let cols = (inner_width / CELL_WIDTH).floor().max(8.0) as u16;
    let rows = (inner_height / CELL_HEIGHT).floor().max(6.0) as u16;
    (cols, rows)
}

fn build_scene_vertices(input: RenderFrameInput) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    let width = input.view_width.max(1.0) as f32;
    let height = input.view_height.max(1.0) as f32;
    let padding_x = OUTER_PADDING_X as f32;
    let padding_y = OUTER_PADDING_Y as f32;
    let header_height = HEADER_HEIGHT as f32;
    let grid_padding_x = GRID_PADDING_X as f32;
    let grid_padding_y = GRID_PADDING_Y as f32;
    let cell_width = CELL_WIDTH as f32;
    let cell_height = CELL_HEIGHT as f32;
    let terminal_x = padding_x;
    let terminal_y = padding_y;
    let terminal_width = (width - (padding_x * 2.0)).max(180.0);
    let terminal_height = (height - (padding_y * 2.0)).max(160.0);
    let content_x = terminal_x + grid_padding_x;
    let content_y = terminal_y + header_height + grid_padding_y;
    let content_width = (input.terminal_cols as f32) * cell_width;
    let content_height = (input.terminal_rows as f32) * cell_height;
    let activity_phase = (input.pty_activity % 24) as usize;

    push_rect(
        &mut vertices,
        terminal_x,
        terminal_y,
        terminal_width,
        terminal_height,
        width,
        height,
        [0.08, 0.09, 0.12, 1.0],
    );

    push_rect(
        &mut vertices,
        terminal_x - 1.0,
        terminal_y - 1.0,
        terminal_width + 2.0,
        terminal_height + 2.0,
        width,
        height,
        [0.16, 0.18, 0.23, 1.0],
    );

    push_rect(
        &mut vertices,
        terminal_x,
        terminal_y,
        terminal_width,
        header_height,
        width,
        height,
        [0.12, 0.14, 0.19, 1.0],
    );

    push_rect(
        &mut vertices,
        content_x,
        content_y,
        content_width,
        content_height,
        width,
        height,
        [0.05, 0.07, 0.10, 1.0],
    );

    let demo_cols = input.terminal_cols.min(6) as usize;
    let demo_rows = input.terminal_rows.min(4) as usize;
    let tile_width = (cell_width - 4.0).max(6.0);
    let tile_height = (cell_height - 4.0).max(8.0);

    for row in 0..demo_rows {
        for col in 0..demo_cols {
            let palette_index = (row * demo_cols + col + activity_phase) % 6;
            let color = match palette_index {
                0 => [0.29, 0.58, 0.78, 1.0],
                1 => [0.34, 0.74, 0.50, 1.0],
                2 => [0.92, 0.68, 0.26, 1.0],
                3 => [0.56, 0.66, 0.82, 1.0],
                4 => [0.72, 0.56, 0.82, 1.0],
                _ => [0.88, 0.44, 0.37, 1.0],
            };
            let tile_x = content_x + (col as f32 * cell_width) + 2.0;
            let tile_y = content_y + (row as f32 * cell_height) + 2.0;

            push_rect(
                &mut vertices,
                tile_x,
                tile_y,
                tile_width,
                tile_height,
                width,
                height,
                [0.13, 0.16, 0.20, 1.0],
            );
            push_rect(
                &mut vertices,
                tile_x + 1.0,
                tile_y + 1.0,
                tile_width - 2.0,
                tile_height - 2.0,
                width,
                height,
                color,
            );
        }
    }

    let cursor_col = activity_phase % demo_cols.max(1);
    let cursor_row = (activity_phase / demo_cols.max(1)) % demo_rows.max(1);
    push_rect(
        &mut vertices,
        content_x + (cursor_col as f32 * cell_width) + 1.0,
        content_y + (cursor_row as f32 * cell_height) + 1.0,
        cell_width - 2.0,
        cell_height - 2.0,
        width,
        height,
        [0.95, 0.96, 0.98, 0.18],
    );

    let indicator_width = (demo_cols as f32 * cell_width).max(48.0);
    push_rect(
        &mut vertices,
        content_x,
        content_y + (demo_rows as f32 * cell_height) + 12.0,
        indicator_width,
        6.0,
        width,
        height,
        [0.16, 0.20, 0.25, 1.0],
    );
    push_rect(
        &mut vertices,
        content_x,
        content_y + (demo_rows as f32 * cell_height) + 12.0,
        indicator_width * (((activity_phase % 12) as f32 + 1.0) / 12.0),
        6.0,
        width,
        height,
        [0.34, 0.74, 0.50, 1.0],
    );

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
            // padding: [0.0, 0.0],
            color,
        },
        Vertex {
            position: [right, top, 0.0, 1.0],
            // padding: [0.0, 0.0],
            color,
        },
        Vertex {
            position: [left, bottom, 0.0, 1.0],
            // padding: [0.0, 0.0],
            color,
        },
        Vertex {
            position: [right, top, 0.0, 1.0],
            // padding: [0.0, 0.0],
            color,
        },
        Vertex {
            position: [right, bottom, 0.0, 1.0],
            // padding: [0.0, 0.0],
            color,
        },
        Vertex {
            position: [left, bottom, 0.0, 1.0],
            // padding: [0.0, 0.0],
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
