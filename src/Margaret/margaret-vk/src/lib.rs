use margaret_core::image::ImageSize;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VulkanRendererBackend;

impl VulkanRendererBackend {
    pub const fn New() -> Self {
        Self
    }

    pub const fn BackendName(&self) -> &'static str {
        "vulkan"
    }

    pub fn SupportsSize(&self, image_size: ImageSize) -> bool {
        image_size.width > 0 && image_size.height > 0
    }
}

#[cfg(test)]
mod tests {
    use super::VulkanRendererBackend;
    use margaret_core::image::ImageSize;

    #[test]
    fn scaffold_accepts_non_zero_image_sizes() {
        let backend = VulkanRendererBackend::New();

        assert!(backend.SupportsSize(ImageSize::New(640, 480)));
        assert!(!backend.SupportsSize(ImageSize::New(0, 480)));
    }
}
