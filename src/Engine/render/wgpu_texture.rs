#![allow(non_snake_case)]

use crate::Engine::render::texture_upload::{
    TexturePixelFormat, TextureUploadPlan, TextureUploadPlanError, TextureUsageIntent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuTextureUploadDesc {
    pub Label: String,
    pub Width: u32,
    pub Height: u32,
    pub Format: wgpu::TextureFormat,
    pub Usage: wgpu::TextureUsages,
    pub Dimension: wgpu::TextureDimension,
    pub MipLevelCount: u32,
    pub SampleCount: u32,
    pub Bytes: Vec<u8>,
    pub BytesPerRow: u32,
    pub RowsPerImage: u32,
}

pub struct WgpuTextureResource {
    pub Texture: wgpu::Texture,
    pub View: wgpu::TextureView,
    pub Label: String,
    pub Width: u32,
    pub Height: u32,
    pub Format: wgpu::TextureFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuTextureResourceError {
    InvalidPlan(TextureUploadPlanError),
    UnsupportedFormat,
    UnsupportedUsage,
    ByteLayoutOverflow,
    EmptyUpload,
}

pub fn BuildWgpuTextureUploadDesc(
    plan: &TextureUploadPlan,
) -> Result<WgpuTextureUploadDesc, WgpuTextureResourceError> {
    ValidateTextureUploadPlan(plan).map_err(WgpuTextureResourceError::InvalidPlan)?;

    if plan.Bytes.is_empty() {
        return Err(WgpuTextureResourceError::EmptyUpload);
    }

    let format = MapTexturePixelFormat(plan.Format)?;
    let usage = MapTextureUsageIntent(plan.Usage)?;

    let bytes_per_row = plan
        .Width
        .checked_mul(4)
        .ok_or(WgpuTextureResourceError::ByteLayoutOverflow)?;
    let rows_per_image = plan.Height;

    let expected_bytes = ComputeExpectedRgba8ByteLength(plan.Width, plan.Height)?;
    if plan.Bytes.len() != expected_bytes {
        return Err(WgpuTextureResourceError::InvalidPlan(
            TextureUploadPlanError::ByteLengthMismatch {
                Expected: expected_bytes,
                Actual: plan.Bytes.len(),
            },
        ));
    }

    Ok(WgpuTextureUploadDesc {
        Label: plan.Label.clone(),
        Width: plan.Width,
        Height: plan.Height,
        Format: format,
        Usage: usage,
        Dimension: wgpu::TextureDimension::D2,
        MipLevelCount: 1,
        SampleCount: 1,
        Bytes: plan.Bytes.clone(),
        BytesPerRow: bytes_per_row,
        RowsPerImage: rows_per_image,
    })
}

pub fn CreateWgpuTextureResource(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    plan: &TextureUploadPlan,
) -> Result<WgpuTextureResource, WgpuTextureResourceError> {
    let desc = BuildWgpuTextureUploadDesc(plan)?;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&desc.Label),
        size: wgpu::Extent3d {
            width: desc.Width,
            height: desc.Height,
            depth_or_array_layers: 1,
        },
        mip_level_count: desc.MipLevelCount,
        sample_count: desc.SampleCount,
        dimension: desc.Dimension,
        format: desc.Format,
        usage: desc.Usage,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &desc.Bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(desc.BytesPerRow),
            rows_per_image: Some(desc.RowsPerImage),
        },
        wgpu::Extent3d {
            width: desc.Width,
            height: desc.Height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    Ok(WgpuTextureResource {
        Texture: texture,
        View: view,
        Label: desc.Label,
        Width: desc.Width,
        Height: desc.Height,
        Format: desc.Format,
    })
}

fn ValidateTextureUploadPlan(plan: &TextureUploadPlan) -> Result<(), TextureUploadPlanError> {
    if plan.Label.trim().is_empty() {
        return Err(TextureUploadPlanError::EmptyLabel);
    }
    if plan.SourceName.trim().is_empty() {
        return Err(TextureUploadPlanError::EmptySourceName);
    }
    if plan.Width == 0 || plan.Height == 0 {
        return Err(TextureUploadPlanError::InvalidDimensions);
    }

    let expected_byte_len = ComputeExpectedRgba8ByteLengthPlan(plan.Width, plan.Height)?;
    if plan.Bytes.len() != expected_byte_len {
        return Err(TextureUploadPlanError::ByteLengthMismatch {
            Expected: expected_byte_len,
            Actual: plan.Bytes.len(),
        });
    }

    Ok(())
}

fn ComputeExpectedRgba8ByteLengthPlan(
    width: u32,
    height: u32,
) -> Result<usize, TextureUploadPlanError> {
    let pixel_count = width
        .checked_mul(height)
        .ok_or(TextureUploadPlanError::ByteLengthOverflow)?;
    let rgba_bytes = pixel_count
        .checked_mul(4)
        .ok_or(TextureUploadPlanError::ByteLengthOverflow)?;

    usize::try_from(rgba_bytes).map_err(|_| TextureUploadPlanError::ByteLengthOverflow)
}

fn ComputeExpectedRgba8ByteLength(
    width: u32,
    height: u32,
) -> Result<usize, WgpuTextureResourceError> {
    let pixel_count = width
        .checked_mul(height)
        .ok_or(WgpuTextureResourceError::ByteLayoutOverflow)?;
    let rgba_bytes = pixel_count
        .checked_mul(4)
        .ok_or(WgpuTextureResourceError::ByteLayoutOverflow)?;

    usize::try_from(rgba_bytes).map_err(|_| WgpuTextureResourceError::ByteLayoutOverflow)
}

fn MapTexturePixelFormat(
    format: TexturePixelFormat,
) -> Result<wgpu::TextureFormat, WgpuTextureResourceError> {
    match format {
        TexturePixelFormat::Rgba8UnormSrgb => Ok(wgpu::TextureFormat::Rgba8UnormSrgb),
    }
}

fn MapTextureUsageIntent(
    usage: TextureUsageIntent,
) -> Result<wgpu::TextureUsages, WgpuTextureResourceError> {
    match usage {
        TextureUsageIntent::SampledColor => {
            Ok(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::render::texture_upload::{
        TexturePixelFormat, TextureUploadPlan, TextureUsageIntent,
    };

    fn Plan2x1() -> TextureUploadPlan {
        TextureUploadPlan {
            Label: "DiffuseSeed".to_string(),
            SourceName: "seed.ppm".to_string(),
            Width: 2,
            Height: 1,
            Format: TexturePixelFormat::Rgba8UnormSrgb,
            Bytes: vec![1, 2, 3, 255, 10, 20, 30, 255],
            Usage: TextureUsageIntent::SampledColor,
        }
    }

    #[test]
    fn BuildWgpuTextureUploadDescMapsDeterministicFields() {
        let plan = Plan2x1();
        let desc = BuildWgpuTextureUploadDesc(&plan)
            .expect("valid texture upload plan should map to wgpu upload descriptor");

        assert_eq!(desc.Label, plan.Label, "label should be preserved");
        assert_eq!(desc.Width, plan.Width, "width should be preserved");
        assert_eq!(desc.Height, plan.Height, "height should be preserved");
        assert_eq!(
            desc.Format,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "Rgba8UnormSrgb plans should map to wgpu Rgba8UnormSrgb"
        );
        assert!(
            desc.Usage.contains(wgpu::TextureUsages::TEXTURE_BINDING),
            "sampled-color usage should include TEXTURE_BINDING"
        );
        assert!(
            desc.Usage.contains(wgpu::TextureUsages::COPY_DST),
            "sampled-color usage should include COPY_DST"
        );
        assert_eq!(desc.BytesPerRow, 8, "bytes-per-row should be width * 4");
        assert_eq!(desc.RowsPerImage, 1, "rows-per-image should equal height");
        assert_eq!(
            desc.Bytes, plan.Bytes,
            "raw bytes should be preserved exactly"
        );
        assert_eq!(
            desc.Dimension,
            wgpu::TextureDimension::D2,
            "M86 dimension should be D2"
        );
        assert_eq!(desc.MipLevelCount, 1, "M86 mip-level count should be 1");
        assert_eq!(desc.SampleCount, 1, "M86 sample-count should be 1");
    }

    #[test]
    fn BuildWgpuTextureUploadDescRejectsInvalidPlan() {
        let mut invalid = Plan2x1();
        invalid.Label = " ".to_string();

        assert_eq!(
            BuildWgpuTextureUploadDesc(&invalid).unwrap_err(),
            WgpuTextureResourceError::InvalidPlan(TextureUploadPlanError::EmptyLabel),
            "empty labels should be rejected through InvalidPlan"
        );
    }

    #[test]
    fn BuildWgpuTextureUploadDescRejectsLayoutOverflow() {
        let overflow = TextureUploadPlan {
            Label: "Overflow".to_string(),
            SourceName: "overflow.ppm".to_string(),
            Width: u32::MAX,
            Height: 2,
            Format: TexturePixelFormat::Rgba8UnormSrgb,
            Bytes: Vec::new(),
            Usage: TextureUsageIntent::SampledColor,
        };

        assert_eq!(
            BuildWgpuTextureUploadDesc(&overflow).unwrap_err(),
            WgpuTextureResourceError::InvalidPlan(TextureUploadPlanError::ByteLengthOverflow),
            "overflow width should return a structured byte-layout overflow"
        );
    }

    #[test]
    fn BuildWgpuTextureUploadDescIsDeterministic() {
        let plan = Plan2x1();
        let a = BuildWgpuTextureUploadDesc(&plan).expect("first descriptor build should succeed");
        let b = BuildWgpuTextureUploadDesc(&plan).expect("second descriptor build should succeed");
        assert_eq!(a, b, "repeated builds should be bit-for-bit deterministic");
    }
}
