#![allow(non_snake_case)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStageVisibility {
    Vertex,
    Pixel,
    VertexPixel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureBindingDimension {
    D2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureSampleKind {
    FloatFilterable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureSamplerBindingLayoutPlan {
    pub Label: String,
    pub TextureBinding: u32,
    pub SamplerBinding: u32,
    pub Visibility: ShaderStageVisibility,
    pub TextureDimension: TextureBindingDimension,
    pub SampleKind: TextureSampleKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureSamplerBindingLayoutPlanError {
    EmptyLabel,
    DuplicateBinding { Binding: u32 },
    UnsupportedTextureDimension,
    UnsupportedSampleKind,
}

impl TextureSamplerBindingLayoutPlan {
    pub fn SampledColor2D(
        label: &str,
        texture_binding: u32,
        sampler_binding: u32,
        visibility: ShaderStageVisibility,
    ) -> Result<Self, TextureSamplerBindingLayoutPlanError> {
        let plan = Self {
            Label: label.to_string(),
            TextureBinding: texture_binding,
            SamplerBinding: sampler_binding,
            Visibility: visibility,
            TextureDimension: TextureBindingDimension::D2,
            SampleKind: TextureSampleKind::FloatFilterable,
        };
        ValidateTextureSamplerBindingLayoutPlan(&plan)?;
        Ok(plan)
    }

    pub fn DefaultSampledColor2D(
        label: &str,
    ) -> Result<Self, TextureSamplerBindingLayoutPlanError> {
        Self::SampledColor2D(label, 0, 1, ShaderStageVisibility::Pixel)
    }
}

pub fn ValidateTextureSamplerBindingLayoutPlan(
    plan: &TextureSamplerBindingLayoutPlan,
) -> Result<(), TextureSamplerBindingLayoutPlanError> {
    if plan.Label.trim().is_empty() {
        return Err(TextureSamplerBindingLayoutPlanError::EmptyLabel);
    }

    if plan.TextureBinding == plan.SamplerBinding {
        return Err(TextureSamplerBindingLayoutPlanError::DuplicateBinding {
            Binding: plan.TextureBinding,
        });
    }

    match plan.TextureDimension {
        TextureBindingDimension::D2 => {}
    }

    match plan.SampleKind {
        TextureSampleKind::FloatFilterable => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn SampledColor2DPlanValidates() {
        let plan = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "ColorBinding",
            2,
            3,
            ShaderStageVisibility::Pixel,
        )
        .expect("sampled-color 2D plan should construct");

        assert_eq!(plan.Label, "ColorBinding", "label should be preserved");
        assert_eq!(
            plan.TextureBinding, 2,
            "texture binding should be preserved"
        );
        assert_eq!(
            plan.SamplerBinding, 3,
            "sampler binding should be preserved"
        );
        assert_eq!(
            plan.Visibility,
            ShaderStageVisibility::Pixel,
            "visibility should be preserved"
        );
        assert_eq!(
            plan.TextureDimension,
            TextureBindingDimension::D2,
            "M88 supports D2 sampled textures"
        );
        assert_eq!(
            plan.SampleKind,
            TextureSampleKind::FloatFilterable,
            "M88 supports float filterable sample kind"
        );
    }

    #[test]
    fn SampledColor2DPlanRejectsInvalidInputs() {
        assert_eq!(
            TextureSamplerBindingLayoutPlan::SampledColor2D(
                " ",
                0,
                1,
                ShaderStageVisibility::Pixel
            )
            .unwrap_err(),
            TextureSamplerBindingLayoutPlanError::EmptyLabel,
            "empty labels should be rejected"
        );

        assert_eq!(
            TextureSamplerBindingLayoutPlan::SampledColor2D(
                "DupBinding",
                4,
                4,
                ShaderStageVisibility::Pixel
            )
            .unwrap_err(),
            TextureSamplerBindingLayoutPlanError::DuplicateBinding { Binding: 4 },
            "duplicate texture/sampler bindings should be rejected"
        );
    }

    #[test]
    fn DefaultSampledColor2DUsesCommonBindingPair() {
        let plan = TextureSamplerBindingLayoutPlan::DefaultSampledColor2D("DefaultBinding")
            .expect("default sampled-color 2D plan should construct");

        assert_eq!(
            plan.TextureBinding, 0,
            "default texture binding should be 0"
        );
        assert_eq!(
            plan.SamplerBinding, 1,
            "default sampler binding should be 1"
        );
        assert_eq!(
            plan.Visibility,
            ShaderStageVisibility::Pixel,
            "default visibility should be pixel"
        );
    }

    #[test]
    fn SampledColor2DPlanIsDeterministic() {
        let a = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "StableBinding",
            7,
            9,
            ShaderStageVisibility::VertexPixel,
        )
        .expect("first plan should construct");
        let b = TextureSamplerBindingLayoutPlan::SampledColor2D(
            "StableBinding",
            7,
            9,
            ShaderStageVisibility::VertexPixel,
        )
        .expect("second plan should construct");

        assert_eq!(a, b, "repeated plan construction should be deterministic");
    }
}
