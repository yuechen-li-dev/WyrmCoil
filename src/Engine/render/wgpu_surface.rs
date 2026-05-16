#![allow(non_snake_case)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSize {
    pub Width: u32,
    pub Height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgpuSurfaceConfigPreferences {
    pub PreferSrgb: bool,
    pub PresentMode: Option<wgpu::PresentMode>,
}

impl Default for WgpuSurfaceConfigPreferences {
    fn default() -> Self {
        Self {
            PreferSrgb: true,
            PresentMode: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgpuSurfaceCapabilitiesInfo {
    pub Formats: Vec<wgpu::TextureFormat>,
    pub PresentModes: Vec<wgpu::PresentMode>,
    pub AlphaModes: Vec<wgpu::CompositeAlphaMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgpuSurfaceConfigPlan {
    pub Width: u32,
    pub Height: u32,
    pub Format: wgpu::TextureFormat,
    pub PresentMode: wgpu::PresentMode,
    pub AlphaMode: wgpu::CompositeAlphaMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgpuSurfaceConfigError {
    InvalidWidth,
    InvalidHeight,
    NoSupportedFormats,
    NoSupportedPresentModes,
    NoSupportedAlphaModes,
    PreferredPresentModeUnsupported { Requested: wgpu::PresentMode },
}

pub fn BuildWgpuSurfaceCapabilitiesInfo(
    capabilities: &wgpu::SurfaceCapabilities,
) -> WgpuSurfaceCapabilitiesInfo {
    WgpuSurfaceCapabilitiesInfo {
        Formats: capabilities.formats.clone(),
        PresentModes: capabilities.present_modes.clone(),
        AlphaModes: capabilities.alpha_modes.clone(),
    }
}

pub fn BuildWgpuSurfaceConfigPlan(
    size: SurfaceSize,
    capabilities: &WgpuSurfaceCapabilitiesInfo,
    preferences: WgpuSurfaceConfigPreferences,
) -> Result<WgpuSurfaceConfigPlan, WgpuSurfaceConfigError> {
    if size.Width == 0 {
        return Err(WgpuSurfaceConfigError::InvalidWidth);
    }
    if size.Height == 0 {
        return Err(WgpuSurfaceConfigError::InvalidHeight);
    }
    if capabilities.Formats.is_empty() {
        return Err(WgpuSurfaceConfigError::NoSupportedFormats);
    }
    if capabilities.PresentModes.is_empty() {
        return Err(WgpuSurfaceConfigError::NoSupportedPresentModes);
    }
    if capabilities.AlphaModes.is_empty() {
        return Err(WgpuSurfaceConfigError::NoSupportedAlphaModes);
    }

    let format = SelectSurfaceFormat(&capabilities.Formats, preferences.PreferSrgb);

    let present_mode = if let Some(requested) = preferences.PresentMode {
        if capabilities.PresentModes.contains(&requested) {
            requested
        } else {
            return Err(WgpuSurfaceConfigError::PreferredPresentModeUnsupported {
                Requested: requested,
            });
        }
    } else if capabilities.PresentModes.contains(&wgpu::PresentMode::Fifo) {
        wgpu::PresentMode::Fifo
    } else {
        capabilities.PresentModes[0]
    };

    Ok(WgpuSurfaceConfigPlan {
        Width: size.Width,
        Height: size.Height,
        Format: format,
        PresentMode: present_mode,
        AlphaMode: capabilities.AlphaModes[0],
    })
}

fn SelectSurfaceFormat(formats: &[wgpu::TextureFormat], prefer_srgb: bool) -> wgpu::TextureFormat {
    if prefer_srgb {
        for format in formats {
            if format.is_srgb() {
                return *format;
            }
        }
    }

    formats[0]
}

pub fn BuildWgpuSurfaceConfiguration(plan: WgpuSurfaceConfigPlan) -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: plan.Format,
        width: plan.Width,
        height: plan.Height,
        present_mode: plan.PresentMode,
        desired_maximum_frame_latency: 2,
        alpha_mode: plan.AlphaMode,
        view_formats: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn BuildCaps(
        formats: Vec<wgpu::TextureFormat>,
        present_modes: Vec<wgpu::PresentMode>,
        alpha_modes: Vec<wgpu::CompositeAlphaMode>,
    ) -> WgpuSurfaceCapabilitiesInfo {
        WgpuSurfaceCapabilitiesInfo {
            Formats: formats,
            PresentModes: present_modes,
            AlphaModes: alpha_modes,
        }
    }

    #[test]
    fn RejectsInvalidWidth() {
        let caps = BuildCaps(
            vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            vec![wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let result = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 0,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences::default(),
        );
        assert_eq!(
            result.unwrap_err(),
            WgpuSurfaceConfigError::InvalidWidth,
            "width=0 should be rejected"
        );
    }

    #[test]
    fn RejectsInvalidHeight() {
        let caps = BuildCaps(
            vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            vec![wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let result = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 0,
            },
            &caps,
            WgpuSurfaceConfigPreferences::default(),
        );
        assert_eq!(
            result.unwrap_err(),
            WgpuSurfaceConfigError::InvalidHeight,
            "height=0 should be rejected"
        );
    }

    #[test]
    fn RejectsWhenNoFormatsSupported() {
        let caps = BuildCaps(
            vec![],
            vec![wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let result = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences::default(),
        );
        assert_eq!(
            result.unwrap_err(),
            WgpuSurfaceConfigError::NoSupportedFormats,
            "empty formats should be rejected"
        );
    }

    #[test]
    fn PrefersSrgbFormatWhenAvailable() {
        let caps = BuildCaps(
            vec![
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            ],
            vec![wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let plan = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences::default(),
        )
        .expect("valid caps should build");
        assert_eq!(
            plan.Format,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            "first srgb format should be selected when preferred"
        );
    }

    #[test]
    fn ChoosesFirstFormatWhenNoSrgbExists() {
        let caps = BuildCaps(
            vec![
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Rgba16Float,
            ],
            vec![wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let plan = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences::default(),
        )
        .expect("valid caps should build");
        assert_eq!(
            plan.Format,
            wgpu::TextureFormat::Rgba8Unorm,
            "first format should be selected when no srgb exists"
        );
    }

    #[test]
    fn UsesExplicitPresentModeWhenSupported() {
        let caps = BuildCaps(
            vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            vec![wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let plan = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences {
                PreferSrgb: true,
                PresentMode: Some(wgpu::PresentMode::Immediate),
            },
        )
        .expect("supported explicit mode should be selected");
        assert_eq!(
            plan.PresentMode,
            wgpu::PresentMode::Immediate,
            "explicit supported present mode should be selected"
        );
    }

    #[test]
    fn RejectsUnsupportedExplicitPresentMode() {
        let caps = BuildCaps(
            vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            vec![wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let result = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences {
                PreferSrgb: true,
                PresentMode: Some(wgpu::PresentMode::Immediate),
            },
        );
        assert_eq!(
            result.unwrap_err(),
            WgpuSurfaceConfigError::PreferredPresentModeUnsupported {
                Requested: wgpu::PresentMode::Immediate
            },
            "unsupported explicit mode should return structured error"
        );
    }

    #[test]
    fn DefaultsToFifoWhenAvailable() {
        let caps = BuildCaps(
            vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            vec![wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let plan = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences::default(),
        )
        .expect("valid caps should build");
        assert_eq!(
            plan.PresentMode,
            wgpu::PresentMode::Fifo,
            "default present mode should be fifo when supported"
        );
    }

    #[test]
    fn ChoosesFirstAlphaMode() {
        let caps = BuildCaps(
            vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            vec![wgpu::PresentMode::Fifo],
            vec![
                wgpu::CompositeAlphaMode::PostMultiplied,
                wgpu::CompositeAlphaMode::Opaque,
            ],
        );
        let plan = BuildWgpuSurfaceConfigPlan(
            SurfaceSize {
                Width: 10,
                Height: 10,
            },
            &caps,
            WgpuSurfaceConfigPreferences::default(),
        )
        .expect("valid caps should build");
        assert_eq!(
            plan.AlphaMode,
            wgpu::CompositeAlphaMode::PostMultiplied,
            "first alpha mode should be chosen"
        );
    }

    #[test]
    fn SelectionIsDeterministicForSameCapabilities() {
        let caps = BuildCaps(
            vec![
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            ],
            vec![wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo],
            vec![wgpu::CompositeAlphaMode::Opaque],
        );
        let size = SurfaceSize {
            Width: 300,
            Height: 200,
        };
        let prefs = WgpuSurfaceConfigPreferences::default();
        let a = BuildWgpuSurfaceConfigPlan(size, &caps, prefs).expect("first plan should build");
        let b = BuildWgpuSurfaceConfigPlan(size, &caps, prefs).expect("second plan should build");
        assert_eq!(
            a, b,
            "same capability/prefs input should produce deterministic plan selection"
        );
    }
}
