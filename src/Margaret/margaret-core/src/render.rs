#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Debug(RenderDebugMode),
    Lit,
}

impl RenderMode {
    pub const fn AsStr(self) -> &'static str {
        match self {
            Self::Debug(mode) => mode.AsStr(),
            Self::Lit => "lit",
        }
    }

    pub fn Parse(name: &str) -> Option<Self> {
        if name == "lit" {
            return Some(Self::Lit);
        }

        RenderDebugMode::Parse(name).map(Self::Debug)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderDebugMode {
    GeometricNormals,
    FlatAlbedo,
    Depth,
}

impl RenderDebugMode {
    pub const fn AsStr(self) -> &'static str {
        match self {
            Self::GeometricNormals => "normals",
            Self::FlatAlbedo => "albedo",
            Self::Depth => "depth",
        }
    }

    pub fn Parse(name: &str) -> Option<Self> {
        match name {
            "normals" => Some(Self::GeometricNormals),
            "albedo" => Some(Self::FlatAlbedo),
            "depth" => Some(Self::Depth),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderSettings {
    pub mode: RenderMode,
    pub depth_max_distance: f32,
}

impl RenderSettings {
    pub const fn New(mode: RenderMode, depth_max_distance: f32) -> Self {
        Self {
            mode,
            depth_max_distance,
        }
    }
}
