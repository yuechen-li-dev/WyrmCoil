#![allow(non_snake_case)]

use crate::Engine::render::{ColorTargetFormat, MapColorTargetFormatToWgpu};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessRenderTargetDesc {
    pub Label: String,
    pub Width: u32,
    pub Height: u32,
    pub Format: ColorTargetFormat,
}

pub struct WgpuHeadlessRenderTarget {
    pub Label: String,
    pub Width: u32,
    pub Height: u32,
    pub Format: ColorTargetFormat,
    pub Texture: wgpu::Texture,
    pub View: wgpu::TextureView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlessRenderTargetError {
    EmptyLabel,
    InvalidWidth,
    InvalidHeight,
    UnsupportedFormat { Format: ColorTargetFormat },
}

pub fn BuildHeadlessRenderTargetDesc(
    label: &str,
    width: u32,
    height: u32,
    format: ColorTargetFormat,
) -> Result<HeadlessRenderTargetDesc, HeadlessRenderTargetError> {
    let desc = HeadlessRenderTargetDesc {
        Label: label.to_string(),
        Width: width,
        Height: height,
        Format: format,
    };
    ValidateHeadlessRenderTargetDesc(&desc)?;
    Ok(desc)
}

pub fn ValidateHeadlessRenderTargetDesc(
    desc: &HeadlessRenderTargetDesc,
) -> Result<(), HeadlessRenderTargetError> {
    if desc.Label.trim().is_empty() {
        return Err(HeadlessRenderTargetError::EmptyLabel);
    }
    if desc.Width == 0 {
        return Err(HeadlessRenderTargetError::InvalidWidth);
    }
    if desc.Height == 0 {
        return Err(HeadlessRenderTargetError::InvalidHeight);
    }

    match desc.Format {
        ColorTargetFormat::Bgra8UnormSrgb | ColorTargetFormat::Rgba8UnormSrgb => Ok(()),
    }
}

pub fn CreateWgpuHeadlessRenderTarget(
    device: &wgpu::Device,
    desc: &HeadlessRenderTargetDesc,
) -> Result<WgpuHeadlessRenderTarget, HeadlessRenderTargetError> {
    ValidateHeadlessRenderTargetDesc(desc)?;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&desc.Label),
        size: wgpu::Extent3d {
            width: desc.Width,
            height: desc.Height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: MapColorTargetFormatToWgpu(desc.Format),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    Ok(WgpuHeadlessRenderTarget {
        Label: desc.Label.clone(),
        Width: desc.Width,
        Height: desc.Height,
        Format: desc.Format,
        Texture: texture,
        View: view,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn BuildHeadlessRenderTargetDescAcceptsValidDescriptor() {
        let desc = BuildHeadlessRenderTargetDesc(
            "OffscreenProbe",
            16,
            16,
            ColorTargetFormat::Bgra8UnormSrgb,
        )
        .expect("valid headless render target descriptor should build");

        assert_eq!(desc.Label, "OffscreenProbe", "label should be preserved");
        assert_eq!(desc.Width, 16, "width should be preserved");
        assert_eq!(desc.Height, 16, "height should be preserved");
        assert_eq!(
            desc.Format,
            ColorTargetFormat::Bgra8UnormSrgb,
            "format should be preserved"
        );
    }

    #[test]
    fn ValidateHeadlessRenderTargetDescRejectsInvalidFields() {
        let empty = HeadlessRenderTargetDesc {
            Label: "  ".to_string(),
            Width: 1,
            Height: 1,
            Format: ColorTargetFormat::Bgra8UnormSrgb,
        };
        assert_eq!(
            ValidateHeadlessRenderTargetDesc(&empty).unwrap_err(),
            HeadlessRenderTargetError::EmptyLabel,
            "empty labels should be rejected"
        );

        let bad_width = HeadlessRenderTargetDesc {
            Label: "Probe".to_string(),
            Width: 0,
            Height: 1,
            Format: ColorTargetFormat::Bgra8UnormSrgb,
        };
        assert_eq!(
            ValidateHeadlessRenderTargetDesc(&bad_width).unwrap_err(),
            HeadlessRenderTargetError::InvalidWidth,
            "zero width should be rejected"
        );

        let bad_height = HeadlessRenderTargetDesc {
            Label: "Probe".to_string(),
            Width: 1,
            Height: 0,
            Format: ColorTargetFormat::Bgra8UnormSrgb,
        };
        assert_eq!(
            ValidateHeadlessRenderTargetDesc(&bad_height).unwrap_err(),
            HeadlessRenderTargetError::InvalidHeight,
            "zero height should be rejected"
        );
    }

    #[test]
    fn SupportedFormatsMapToExpectedWgpuTextureFormats() {
        assert_eq!(
            MapColorTargetFormatToWgpu(ColorTargetFormat::Bgra8UnormSrgb),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            "bgra color target format should map to bgra8 srgb texture format"
        );
        assert_eq!(
            MapColorTargetFormatToWgpu(ColorTargetFormat::Rgba8UnormSrgb),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "rgba color target format should map to rgba8 srgb texture format"
        );
    }

    #[test]
    fn BuildHeadlessRenderTargetDescIsDeterministic() {
        let a = BuildHeadlessRenderTargetDesc(
            "DeterministicProbe",
            32,
            24,
            ColorTargetFormat::Rgba8UnormSrgb,
        )
        .expect("first descriptor build should succeed");
        let b = BuildHeadlessRenderTargetDesc(
            "DeterministicProbe",
            32,
            24,
            ColorTargetFormat::Rgba8UnormSrgb,
        )
        .expect("second descriptor build should succeed");

        assert_eq!(
            a, b,
            "same descriptor inputs should produce equal descriptor output"
        );
    }
}
