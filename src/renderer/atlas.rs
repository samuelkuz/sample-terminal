use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{CGAffineTransform, CFString, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGGlyph, CGImageAlphaInfo,
};
use objc2_core_text::{CTFont, CTFontOrientation, CTFontOptions};
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLRegion, MTLResourceOptions, MTLSize, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage,
};

const ATLAS_WIDTH: usize = 512;
const ATLAS_PADDING: usize = 1;
const CELL_WIDTH: f32 = 16.0;
const CELL_HEIGHT: f32 = 24.0;
const FONT_NAME: &str = "Menlo";
const FONT_SIZE: f64 = 18.0;
const FALLBACK_CHAR: char = '?';

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FontMetrics {
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) leading: f32,
    pub(crate) baseline: f32,
    pub(crate) horizontal_inset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphMetadata {
    pub(crate) ch: char,
    pub(crate) glyph_id: u16,
    pub(crate) uv_origin: [f32; 2],
    pub(crate) uv_size: [f32; 2],
    pub(crate) bitmap_size: [f32; 2],
    pub(crate) offset: [f32; 2],
    pub(crate) advance: f32,
    pub(crate) atlas_origin: [usize; 2],
}

pub(crate) struct GlyphAtlas {
    texture: objc2::rc::Retained<ProtocolObject<dyn MTLTexture>>,
    glyphs: HashMap<char, GlyphMetadata>,
    fallback: GlyphMetadata,
    metrics: FontMetrics,
    atlas_size: [usize; 2],
}

impl GlyphAtlas {
    pub(crate) fn new(device: &ProtocolObject<dyn MTLDevice>) -> Result<Self, String> {
        let font = load_font();
        let metrics = compute_font_metrics(&font);
        let rasters = build_glyph_rasters(&font, metrics)?;
        let atlas_pack = pack_glyphs(&rasters, ATLAS_WIDTH)?;
        let texture = build_texture(device, &atlas_pack.bytes, atlas_pack.size)?;

        let mut glyphs = HashMap::with_capacity(rasters.len());
        for raster in rasters {
            let placement = atlas_pack
                .placements
                .get(&raster.ch)
                .ok_or_else(|| format!("missing atlas placement for {:?}", raster.ch))?;
            glyphs.insert(
                raster.ch,
                GlyphMetadata {
                    ch: raster.ch,
                    glyph_id: raster.glyph_id,
                    uv_origin: [
                        placement.x as f32 / atlas_pack.size[0] as f32,
                        placement.y as f32 / atlas_pack.size[1] as f32,
                    ],
                    uv_size: [
                        raster.bitmap_width as f32 / atlas_pack.size[0] as f32,
                        raster.bitmap_height as f32 / atlas_pack.size[1] as f32,
                    ],
                    bitmap_size: [raster.bitmap_width as f32, raster.bitmap_height as f32],
                    offset: [metrics.horizontal_inset + raster.bounds.origin.x as f32, metrics.baseline - raster.bounds.origin.y as f32 - raster.bounds.size.height as f32],
                    advance: raster.advance.width as f32,
                    atlas_origin: [placement.x, placement.y],
                },
            );
        }

        let fallback = glyphs
            .get(&FALLBACK_CHAR)
            .copied()
            .ok_or_else(|| "fallback glyph was not generated".to_string())?;

        Ok(Self {
            texture,
            glyphs,
            fallback,
            metrics,
            atlas_size: atlas_pack.size,
        })
    }

    pub(crate) fn texture(&self) -> &ProtocolObject<dyn MTLTexture> {
        &self.texture
    }

    pub(crate) fn font_metrics(&self) -> FontMetrics {
        self.metrics
    }

    pub(crate) fn glyph_for(&self, ch: char) -> GlyphMetadata {
        self.glyphs.get(&ch).copied().unwrap_or(self.fallback)
    }

    #[cfg(test)]
    pub(crate) fn atlas_size(&self) -> [usize; 2] {
        self.atlas_size
    }
}

#[derive(Clone, Debug)]
struct RasterizedGlyph {
    ch: char,
    glyph_id: u16,
    bounds: CGRect,
    advance: CGSize,
    bitmap_width: usize,
    bitmap_height: usize,
    bitmap: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AtlasPlacement {
    x: usize,
    y: usize,
}

#[derive(Clone, Debug)]
struct AtlasPack {
    size: [usize; 2],
    bytes: Vec<u8>,
    placements: HashMap<char, AtlasPlacement>,
}

fn load_font() -> objc2_core_foundation::CFRetained<CTFont> {
    let name = CFString::from_str(FONT_NAME);
    unsafe {
        CTFont::with_name_and_options(
            &name,
            FONT_SIZE,
            std::ptr::null(),
            CTFontOptions::PreferSystemFont,
        )
    }
}

fn compute_font_metrics(font: &CTFont) -> FontMetrics {
    let ascent = unsafe { font.ascent() as f32 };
    let descent = unsafe { font.descent() as f32 };
    let leading = unsafe { font.leading() as f32 };
    let advance = glyph_advance(font, 'M');
    let horizontal_inset = ((CELL_WIDTH - advance.width as f32) * 0.5).max(0.0).floor();
    let baseline = (1.0 + ascent).min(CELL_HEIGHT - 2.0);

    FontMetrics {
        ascent,
        descent,
        leading,
        baseline,
        horizontal_inset,
    }
}

fn build_glyph_rasters(font: &CTFont, metrics: FontMetrics) -> Result<Vec<RasterizedGlyph>, String> {
    let mut rasters = Vec::new();
    for ch in supported_chars() {
        let glyph_id = glyph_for_char(font, ch)?;
        let (bounds, advance) = glyph_metrics(font, glyph_id);
        let (bitmap_width, bitmap_height) = bitmap_dimensions(bounds);
        let bitmap = if ch == ' ' || bitmap_width == 0 || bitmap_height == 0 {
            Vec::new()
        } else {
            rasterize_glyph(font, glyph_id, bounds, bitmap_width, bitmap_height)?
        };

        let _ = metrics;
        rasters.push(RasterizedGlyph {
            ch,
            glyph_id,
            bounds,
            advance,
            bitmap_width,
            bitmap_height,
            bitmap,
        });
    }
    Ok(rasters)
}

fn glyph_for_char(font: &CTFont, ch: char) -> Result<u16, String> {
    let mut code_units = [ch as u32 as u16];
    let mut glyphs = [0u16; 1];
    let did_map = unsafe {
        font.glyphs_for_characters(
            NonNull::new(code_units.as_mut_ptr()).expect("single code unit pointer"),
            NonNull::new(glyphs.as_mut_ptr().cast::<CGGlyph>()).expect("single glyph pointer"),
            1,
        )
    };
    if did_map && glyphs[0] != 0 {
        Ok(glyphs[0])
    } else {
        Err(format!("font {FONT_NAME} could not map {:?}", ch))
    }
}

fn glyph_metrics(font: &CTFont, glyph_id: u16) -> (CGRect, CGSize) {
    let mut glyph = [glyph_id];
    let mut bounds = [CGRect::ZERO];
    let mut advances = [CGSize::ZERO];
    unsafe {
        font.bounding_rects_for_glyphs(
            CTFontOrientation::Horizontal,
            NonNull::new(glyph.as_mut_ptr().cast::<CGGlyph>()).expect("glyph pointer"),
            bounds.as_mut_ptr(),
            1,
        );
        font.advances_for_glyphs(
            CTFontOrientation::Horizontal,
            NonNull::new(glyph.as_mut_ptr().cast::<CGGlyph>()).expect("glyph pointer"),
            advances.as_mut_ptr(),
            1,
        );
    }
    (bounds[0], advances[0])
}

fn glyph_advance(font: &CTFont, ch: char) -> CGSize {
    let glyph_id = glyph_for_char(font, ch).unwrap_or(0);
    let (_, advance) = glyph_metrics(font, glyph_id);
    advance
}

fn bitmap_dimensions(bounds: CGRect) -> (usize, usize) {
    let width = bounds.size.width.ceil().max(0.0) as usize;
    let height = bounds.size.height.ceil().max(0.0) as usize;
    (width, height)
}

fn rasterize_glyph(
    font: &CTFont,
    glyph_id: u16,
    bounds: CGRect,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, String> {
    let color_space =
        CGColorSpace::new_device_gray().ok_or_else(|| "failed to create gray color space".to_string())?;
    let mut bitmap = vec![0u8; width * height];
    let context = unsafe {
        CGBitmapContextCreate(
            bitmap.as_mut_ptr().cast::<c_void>(),
            width,
            height,
            8,
            width,
            Some(&color_space),
            CGImageAlphaInfo::None.0,
        )
    }
    .ok_or_else(|| format!("failed to create bitmap context for glyph {}", glyph_id))?;

    CGContext::set_should_antialias(Some(&context), true);
    CGContext::set_allows_antialiasing(Some(&context), true);
    CGContext::set_should_smooth_fonts(Some(&context), false);
    CGContext::set_should_subpixel_position_fonts(Some(&context), false);
    CGContext::set_should_subpixel_quantize_fonts(Some(&context), false);
    CGContext::set_text_drawing_mode(Some(&context), objc2_core_graphics::CGTextDrawingMode::Fill);
    CGContext::set_gray_fill_color(Some(&context), 1.0, 1.0);
    CGContext::clear_rect(
        Some(&context),
        CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64)),
    );
    CGContext::translate_ctm(Some(&context), 0.0, height as f64);
    CGContext::scale_ctm(Some(&context), 1.0, -1.0);
    CGContext::translate_ctm(Some(&context), -bounds.origin.x, -bounds.origin.y);
    CGContext::set_text_matrix(
        Some(&context),
        CGAffineTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        },
    );

    let path = unsafe { font.path_for_glyph(glyph_id, std::ptr::null()) }
        .ok_or_else(|| format!("failed to build path for glyph {}", glyph_id))?;
    CGContext::add_path(Some(&context), Some(&path));
    CGContext::fill_path(Some(&context));
    CGContext::flush(Some(&context));

    Ok(bitmap)
}

fn build_texture(
    device: &ProtocolObject<dyn MTLDevice>,
    atlas_bytes: &[u8],
    atlas_size: [usize; 2],
) -> Result<objc2::rc::Retained<ProtocolObject<dyn MTLTexture>>, String> {
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::R8Unorm,
            atlas_size[0],
            atlas_size[1],
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::ShaderRead);
    descriptor.setResourceOptions(MTLResourceOptions::StorageModeShared);
    let texture = device
        .newTextureWithDescriptor(&descriptor)
        .ok_or("failed to create atlas texture")?;

    let region = MTLRegion {
        origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width: atlas_size[0],
            height: atlas_size[1],
            depth: 1,
        },
    };

    let bytes = NonNull::new(atlas_bytes.as_ptr().cast_mut().cast::<c_void>())
        .ok_or("failed to prepare atlas bytes")?;
    unsafe {
        texture.replaceRegion_mipmapLevel_withBytes_bytesPerRow(region, 0, bytes, atlas_size[0]);
    }
    Ok(texture)
}

fn pack_glyphs(glyphs: &[RasterizedGlyph], atlas_width: usize) -> Result<AtlasPack, String> {
    let mut placements = HashMap::with_capacity(glyphs.len());
    let mut cursor_x = ATLAS_PADDING;
    let mut cursor_y = ATLAS_PADDING;
    let mut row_height = 0usize;
    let mut atlas_height = ATLAS_PADDING;

    for glyph in glyphs {
        if glyph.bitmap_width == 0 || glyph.bitmap_height == 0 {
            placements.insert(glyph.ch, AtlasPlacement { x: 0, y: 0 });
            continue;
        }

        let required_width = glyph.bitmap_width + ATLAS_PADDING;
        if cursor_x + required_width > atlas_width {
            cursor_x = ATLAS_PADDING;
            cursor_y += row_height + ATLAS_PADDING;
            row_height = 0;
        }

        placements.insert(
            glyph.ch,
            AtlasPlacement {
                x: cursor_x,
                y: cursor_y,
            },
        );
        cursor_x += glyph.bitmap_width + ATLAS_PADDING;
        row_height = row_height.max(glyph.bitmap_height);
        atlas_height = atlas_height.max(cursor_y + glyph.bitmap_height + ATLAS_PADDING);
    }

    let mut bytes = vec![0u8; atlas_width * atlas_height.max(1)];
    for glyph in glyphs {
        if glyph.bitmap_width == 0 || glyph.bitmap_height == 0 {
            continue;
        }
        let placement = placements
            .get(&glyph.ch)
            .ok_or_else(|| format!("missing atlas placement for {:?}", glyph.ch))?;
        for row in 0..glyph.bitmap_height {
            // Core Graphics writes bitmap rows bottom-up for this context setup,
            // while the atlas/UV code treats row 0 as the top row.
            let src_row = glyph.bitmap_height - 1 - row;
            let src_start = src_row * glyph.bitmap_width;
            let dst_start = (placement.y + row) * atlas_width + placement.x;
            bytes[dst_start..dst_start + glyph.bitmap_width]
                .copy_from_slice(&glyph.bitmap[src_start..src_start + glyph.bitmap_width]);
        }
    }

    Ok(AtlasPack {
        size: [atlas_width, atlas_height.max(1)],
        bytes,
        placements,
    })
}

fn supported_chars() -> Vec<char> {
    (0x20u32..=0x7eu32)
        .chain(0xa0u32..=0xffu32)
        .filter_map(char::from_u32)
        .collect()
}

#[cfg(test)]
mod tests {
    use objc2_metal::MTLCreateSystemDefaultDevice;

    use super::{build_glyph_rasters, compute_font_metrics, load_font, pack_glyphs, AtlasPlacement, RasterizedGlyph};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use std::collections::HashSet;

    fn glyph(ch: char, width: usize, height: usize) -> RasterizedGlyph {
        RasterizedGlyph {
            ch,
            glyph_id: ch as u16,
            bounds: CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64)),
            advance: CGSize::new(width as f64, 0.0),
            bitmap_width: width,
            bitmap_height: height,
            bitmap: vec![255; width * height],
        }
    }

    #[test]
    fn atlas_packing_stays_in_bounds_and_non_overlapping() {
        let glyphs = vec![glyph('A', 8, 12), glyph('B', 10, 14), glyph('C', 12, 10)];
        let pack = pack_glyphs(&glyphs, 32).expect("pack succeeds");
        let mut occupied = HashSet::new();

        for glyph in glyphs {
            let AtlasPlacement { x, y } = pack.placements[&glyph.ch];
            assert!(x + glyph.bitmap_width <= pack.size[0]);
            assert!(y + glyph.bitmap_height <= pack.size[1]);
            for row in 0..glyph.bitmap_height {
                for col in 0..glyph.bitmap_width {
                    assert!(occupied.insert((x + col, y + row)));
                }
            }
        }
    }

    #[test]
    fn atlas_builds_with_metrics_and_size() {
        let device = MTLCreateSystemDefaultDevice().expect("default metal device");
        let atlas = super::GlyphAtlas::new(&device).expect("glyph atlas");
        let metrics = atlas.font_metrics();

        assert!(metrics.ascent > 0.0);
        assert!(metrics.baseline > 0.0);
        assert!(atlas.atlas_size()[0] > 0);
        assert!(atlas.atlas_size()[1] > 0);
    }

    #[test]
    fn representative_glyph_has_nonzero_coverage() {
        let font = load_font();
        let metrics = compute_font_metrics(&font);
        let rasters = build_glyph_rasters(&font, metrics).expect("glyph rasters");
        let glyph = rasters.iter().find(|glyph| glyph.ch == 'A').expect("A glyph");

        assert!(glyph.bitmap_width > 0);
        assert!(glyph.bitmap_height > 0);
        assert!(glyph.bitmap.iter().any(|value| *value > 0));
    }
}
