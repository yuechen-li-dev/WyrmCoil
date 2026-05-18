#![allow(non_snake_case)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFilterMode {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureAddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamplerPlan {
    pub Label: String,
    pub MagFilter: TextureFilterMode,
    pub MinFilter: TextureFilterMode,
    pub MipmapFilter: TextureFilterMode,
    pub AddressU: TextureAddressMode,
    pub AddressV: TextureAddressMode,
    pub AddressW: TextureAddressMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SamplerPlanError {
    EmptyLabel,
}

impl SamplerPlan {
    pub fn DefaultColor(label: &str) -> Result<Self, SamplerPlanError> {
        Self::LinearClamp(label)
    }

    pub fn NearestClamp(label: &str) -> Result<Self, SamplerPlanError> {
        Self::Build(
            label,
            TextureFilterMode::Nearest,
            TextureFilterMode::Nearest,
            TextureFilterMode::Nearest,
            TextureAddressMode::ClampToEdge,
            TextureAddressMode::ClampToEdge,
            TextureAddressMode::ClampToEdge,
        )
    }

    pub fn LinearClamp(label: &str) -> Result<Self, SamplerPlanError> {
        Self::Build(
            label,
            TextureFilterMode::Linear,
            TextureFilterMode::Linear,
            TextureFilterMode::Nearest,
            TextureAddressMode::ClampToEdge,
            TextureAddressMode::ClampToEdge,
            TextureAddressMode::ClampToEdge,
        )
    }

    pub fn PixelArt(label: &str) -> Result<Self, SamplerPlanError> {
        Self::NearestClamp(label)
    }

    pub fn LinearRepeat(label: &str) -> Result<Self, SamplerPlanError> {
        Self::Build(
            label,
            TextureFilterMode::Linear,
            TextureFilterMode::Linear,
            TextureFilterMode::Nearest,
            TextureAddressMode::Repeat,
            TextureAddressMode::Repeat,
            TextureAddressMode::Repeat,
        )
    }

    pub fn Build(
        label: &str,
        mag_filter: TextureFilterMode,
        min_filter: TextureFilterMode,
        mipmap_filter: TextureFilterMode,
        address_u: TextureAddressMode,
        address_v: TextureAddressMode,
        address_w: TextureAddressMode,
    ) -> Result<Self, SamplerPlanError> {
        let plan = Self {
            Label: label.to_string(),
            MagFilter: mag_filter,
            MinFilter: min_filter,
            MipmapFilter: mipmap_filter,
            AddressU: address_u,
            AddressV: address_v,
            AddressW: address_w,
        };
        ValidateSamplerPlan(&plan)?;
        Ok(plan)
    }
}

pub fn ValidateSamplerPlan(plan: &SamplerPlan) -> Result<(), SamplerPlanError> {
    if plan.Label.trim().is_empty() {
        return Err(SamplerPlanError::EmptyLabel);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn SamplerPlanPresetsProduceExpectedModes() {
        let default_color = SamplerPlan::DefaultColor("ColorSampler")
            .expect("default color sampler should construct successfully");
        assert_eq!(
            default_color.MagFilter,
            TextureFilterMode::Linear,
            "default color should use linear mag filter"
        );
        assert_eq!(
            default_color.MinFilter,
            TextureFilterMode::Linear,
            "default color should use linear min filter"
        );
        assert_eq!(
            default_color.MipmapFilter,
            TextureFilterMode::Nearest,
            "default color mipmap filter should remain nearest metadata until mipmaps are added"
        );
        assert_eq!(
            default_color.AddressU,
            TextureAddressMode::ClampToEdge,
            "default color U address should clamp"
        );
        assert_eq!(
            default_color.AddressV,
            TextureAddressMode::ClampToEdge,
            "default color V address should clamp"
        );
        assert_eq!(
            default_color.AddressW,
            TextureAddressMode::ClampToEdge,
            "default color W address should clamp"
        );

        let nearest = SamplerPlan::NearestClamp("Nearest")
            .expect("nearest clamp sampler should construct successfully");
        assert_eq!(
            nearest.MagFilter,
            TextureFilterMode::Nearest,
            "nearest clamp should use nearest mag filter"
        );
        assert_eq!(
            nearest.MinFilter,
            TextureFilterMode::Nearest,
            "nearest clamp should use nearest min filter"
        );
        assert_eq!(
            nearest.MipmapFilter,
            TextureFilterMode::Nearest,
            "nearest clamp should use nearest mipmap filter"
        );

        let pixel_art = SamplerPlan::PixelArt("PixelArt")
            .expect("pixel-art sampler should construct successfully");
        assert_eq!(
            pixel_art.MagFilter, nearest.MagFilter,
            "pixel-art should match nearest-clamp mag filter"
        );
        assert_eq!(
            pixel_art.MinFilter, nearest.MinFilter,
            "pixel-art should match nearest-clamp min filter"
        );
        assert_eq!(
            pixel_art.MipmapFilter, nearest.MipmapFilter,
            "pixel-art should match nearest-clamp mipmap filter"
        );
        assert_eq!(
            pixel_art.AddressU, nearest.AddressU,
            "pixel-art should match nearest-clamp U address mode"
        );
        assert_eq!(
            pixel_art.AddressV, nearest.AddressV,
            "pixel-art should match nearest-clamp V address mode"
        );
        assert_eq!(
            pixel_art.AddressW, nearest.AddressW,
            "pixel-art should match nearest-clamp W address mode"
        );
    }

    #[test]
    fn SamplerPlanRejectsEmptyLabel() {
        assert_eq!(
            SamplerPlan::DefaultColor(" ").unwrap_err(),
            SamplerPlanError::EmptyLabel,
            "empty labels should be rejected"
        );
    }

    #[test]
    fn SamplerPlanConstructionIsDeterministic() {
        let a = SamplerPlan::LinearRepeat("Repeatable")
            .expect("first linear-repeat construction should succeed");
        let b = SamplerPlan::LinearRepeat("Repeatable")
            .expect("second linear-repeat construction should succeed");
        assert_eq!(a, b, "repeated constructions should be deterministic");
    }
}
