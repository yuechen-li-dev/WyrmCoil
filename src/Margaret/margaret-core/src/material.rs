use crate::color::ColorRgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialDescription {
    pub id: MaterialId,
    pub name: String,
    pub kind: MaterialKind,
}

impl MaterialDescription {
    pub fn New(id: MaterialId, name: impl Into<String>, kind: MaterialKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
        }
    }

    pub fn DiffuseAlbedo(&self) -> ColorRgb {
        match self.kind {
            MaterialKind::Diffuse { albedo, .. } => albedo,
            MaterialKind::SpecularReflector { reflectance } => reflectance,
            MaterialKind::Dielectric { .. } => ColorRgb::WHITE,
        }
    }

    pub fn EmissiveRadiance(&self) -> ColorRgb {
        match self.kind {
            MaterialKind::Diffuse { emission, .. } => emission,
            MaterialKind::SpecularReflector { .. } | MaterialKind::Dielectric { .. } => {
                ColorRgb::BLACK
            }
        }
    }

    pub fn IsEmissive(&self) -> bool {
        self.EmissiveRadiance() != ColorRgb::BLACK
    }

    pub fn HasUnsupportedM3aDiffuseEmissionMix(&self) -> bool {
        match self.kind {
            MaterialKind::Diffuse { albedo, emission } => {
                albedo != ColorRgb::BLACK && emission != ColorRgb::BLACK
            }
            MaterialKind::SpecularReflector { .. } | MaterialKind::Dielectric { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MaterialKind {
    Diffuse {
        albedo: ColorRgb,
        emission: ColorRgb,
    },
    SpecularReflector {
        reflectance: ColorRgb,
    },
    Dielectric {
        refractive_index: f32,
    },
}
