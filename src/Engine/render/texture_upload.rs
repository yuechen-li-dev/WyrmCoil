#![allow(non_snake_case)]

use crate::Engine::asset::{AssetResult, DecodedImageAsset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexturePixelFormat {
    Rgba8UnormSrgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureUsageIntent {
    SampledColor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureUploadPlan {
    pub Label: String,
    pub SourceName: String,
    pub Width: u32,
    pub Height: u32,
    pub Format: TexturePixelFormat,
    pub Bytes: Vec<u8>,
    pub Usage: TextureUsageIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureUploadPlanOptions {
    pub Label: String,
    pub Usage: TextureUsageIntent,
    pub Format: TexturePixelFormat,
}

impl TextureUploadPlanOptions {
    pub fn New(label: &str) -> Self {
        Self {
            Label: label.to_string(),
            Usage: TextureUsageIntent::SampledColor,
            Format: TexturePixelFormat::Rgba8UnormSrgb,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureUploadPlanError {
    EmptyLabel,
    EmptySourceName,
    InvalidDimensions,
    ByteLengthMismatch { Expected: usize, Actual: usize },
    ByteLengthOverflow,
    ExpectedDecodedImage,
}

pub fn BuildTextureUploadPlanFromDecodedImage(
    image: &DecodedImageAsset,
    label: &str,
) -> Result<TextureUploadPlan, TextureUploadPlanError> {
    BuildTextureUploadPlan(image, TextureUploadPlanOptions::New(label))
}

pub fn BuildTextureUploadPlan(
    image: &DecodedImageAsset,
    options: TextureUploadPlanOptions,
) -> Result<TextureUploadPlan, TextureUploadPlanError> {
    if options.Label.trim().is_empty() {
        return Err(TextureUploadPlanError::EmptyLabel);
    }
    if image.SourceName.trim().is_empty() {
        return Err(TextureUploadPlanError::EmptySourceName);
    }
    if image.Width == 0 || image.Height == 0 {
        return Err(TextureUploadPlanError::InvalidDimensions);
    }

    let expected_byte_len = ComputeExpectedRgba8ByteLength(image.Width, image.Height)?;
    if image.Rgba8.len() != expected_byte_len {
        return Err(TextureUploadPlanError::ByteLengthMismatch {
            Expected: expected_byte_len,
            Actual: image.Rgba8.len(),
        });
    }

    Ok(TextureUploadPlan {
        Label: options.Label,
        SourceName: image.SourceName.clone(),
        Width: image.Width,
        Height: image.Height,
        Format: options.Format,
        Bytes: image.Rgba8.clone(),
        Usage: options.Usage,
    })
}

pub fn TryBuildTextureUploadPlanFromAssetResult(
    result: &AssetResult,
    options: TextureUploadPlanOptions,
) -> Result<TextureUploadPlan, TextureUploadPlanError> {
    match result {
        AssetResult::ImageDecoded(image) => BuildTextureUploadPlan(image, options),
        _ => Err(TextureUploadPlanError::ExpectedDecodedImage),
    }
}

fn ComputeExpectedRgba8ByteLength(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine::asset::{
        AssetRequestId, AssetResult, BytesLoadedAssetResult, DecodedImageAsset, ImageDecodeFailure,
        ImageDecodeFailureKind,
    };

    fn Image2x1() -> DecodedImageAsset {
        DecodedImageAsset {
            RequestId: AssetRequestId(7),
            SourceName: "seed.ppm".to_string(),
            Width: 2,
            Height: 1,
            Rgba8: vec![1, 2, 3, 255, 10, 20, 30, 255],
        }
    }

    #[test]
    fn BuildTextureUploadPlanFromDecodedImageSuccess() {
        let image = Image2x1();
        let plan = BuildTextureUploadPlanFromDecodedImage(&image, "DiffuseSeed")
            .expect("valid decoded image should produce a texture upload plan");

        assert_eq!(plan.Label, "DiffuseSeed", "label should be preserved");
        assert_eq!(
            plan.SourceName, image.SourceName,
            "source should be preserved"
        );
        assert_eq!(plan.Width, image.Width, "width should be preserved");
        assert_eq!(plan.Height, image.Height, "height should be preserved");
        assert_eq!(
            plan.Format,
            TexturePixelFormat::Rgba8UnormSrgb,
            "default format should be RGBA8 UNORM sRGB"
        );
        assert_eq!(
            plan.Usage,
            TextureUsageIntent::SampledColor,
            "M85 usage intent should be sampled color"
        );
        assert_eq!(
            plan.Bytes, image.Rgba8,
            "plan bytes should exactly preserve decoded RGBA8 ordering"
        );
    }

    #[test]
    fn BuildTextureUploadPlanRejectsInvalidInputs() {
        let mut image = Image2x1();
        assert_eq!(
            BuildTextureUploadPlanFromDecodedImage(&image, " ").unwrap_err(),
            TextureUploadPlanError::EmptyLabel,
            "empty labels should be rejected"
        );

        image.SourceName = " ".to_string();
        assert_eq!(
            BuildTextureUploadPlanFromDecodedImage(&image, "Label").unwrap_err(),
            TextureUploadPlanError::EmptySourceName,
            "empty source names should be rejected"
        );

        let mut zero_width = Image2x1();
        zero_width.Width = 0;
        assert_eq!(
            BuildTextureUploadPlanFromDecodedImage(&zero_width, "Label").unwrap_err(),
            TextureUploadPlanError::InvalidDimensions,
            "zero width should be rejected"
        );

        let mut zero_height = Image2x1();
        zero_height.Height = 0;
        assert_eq!(
            BuildTextureUploadPlanFromDecodedImage(&zero_height, "Label").unwrap_err(),
            TextureUploadPlanError::InvalidDimensions,
            "zero height should be rejected"
        );
    }

    #[test]
    fn BuildTextureUploadPlanRejectsByteLengthMismatchAndOverflow() {
        let mut wrong_len = Image2x1();
        wrong_len.Rgba8.pop();
        assert_eq!(
            BuildTextureUploadPlanFromDecodedImage(&wrong_len, "Label").unwrap_err(),
            TextureUploadPlanError::ByteLengthMismatch {
                Expected: 8,
                Actual: 7
            },
            "byte length mismatch should be structured"
        );

        let overflow = DecodedImageAsset {
            RequestId: AssetRequestId(9),
            SourceName: "overflow.ppm".to_string(),
            Width: u32::MAX,
            Height: u32::MAX,
            Rgba8: Vec::new(),
        };
        assert_eq!(
            BuildTextureUploadPlanFromDecodedImage(&overflow, "Label").unwrap_err(),
            TextureUploadPlanError::ByteLengthOverflow,
            "overflow dimensions should be rejected safely"
        );
    }

    #[test]
    fn TryBuildTextureUploadPlanFromAssetResultCoversKindsAndDeterminism() {
        let image = Image2x1();
        let decoded = AssetResult::ImageDecoded(image.clone());
        let options = TextureUploadPlanOptions::New("FromResult");
        let plan_a = TryBuildTextureUploadPlanFromAssetResult(&decoded, options.clone())
            .expect("decoded image result should build plan");
        let plan_b = TryBuildTextureUploadPlanFromAssetResult(&decoded, options)
            .expect("same inputs should produce deterministic equal plan");
        assert_eq!(
            plan_a, plan_b,
            "same image and options should produce identical upload plans"
        );

        let bytes_loaded = AssetResult::BytesLoaded(BytesLoadedAssetResult {
            RequestId: AssetRequestId(10),
            Path: "x.bin".to_string(),
            Bytes: vec![1],
        });
        assert_eq!(
            TryBuildTextureUploadPlanFromAssetResult(
                &bytes_loaded,
                TextureUploadPlanOptions::New("Invalid")
            )
            .unwrap_err(),
            TextureUploadPlanError::ExpectedDecodedImage,
            "non-decoded result should return ExpectedDecodedImage"
        );

        let decode_failed = AssetResult::DecodeFailed(ImageDecodeFailure {
            RequestId: AssetRequestId(11),
            SourceName: "bad.ppm".to_string(),
            Kind: ImageDecodeFailureKind::InvalidData,
        });
        assert_eq!(
            TryBuildTextureUploadPlanFromAssetResult(
                &decode_failed,
                TextureUploadPlanOptions::New("Invalid")
            )
            .unwrap_err(),
            TextureUploadPlanError::ExpectedDecodedImage,
            "decode failures should not build upload plans"
        );
    }
}
