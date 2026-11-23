use std::collections::VecDeque;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ::gltf::{self, buffer, image as gltf_image};

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_BUFFERS: usize = 128 * 1024 * 1024;
const MAX_TEXTURES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub enum AssetStage {
    Resolve,
    Parse,
    Buffers,
    Textures,
    Geometry,
    Animation,
}

#[derive(Debug)]
pub struct AssetError {
    pub Path: PathBuf,
    pub Stage: AssetStage,
    pub Detail: String,
}

impl AssetError {
    pub(super) fn New(path: &Path, stage: AssetStage, detail: impl ToString) -> Self {
        Self {
            Path: path.to_path_buf(),
            Stage: stage,
            Detail: detail.to_string(),
        }
    }
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{:?}]: {}",
            self.Path.display(),
            self.Stage,
            self.Detail
        )
    }
}

impl std::error::Error for AssetError {}

impl From<AssetError> for String {
    fn from(error: AssetError) -> Self {
        error.to_string()
    }
}

#[derive(Default, Clone, Debug)]
pub struct ImportTimings {
    pub SourceRead: Duration,
    pub Parse: Duration,
    pub Buffers: Duration,
    pub Textures: Duration,
    pub Images: ImageTimings,
    pub Geometry: Duration,
    pub Skins: Duration,
    pub Animation: Duration,
}

#[derive(Default, Clone, Debug)]
pub struct ImageTimings {
    pub Resolve: Duration,
    pub ExternalRead: Duration,
    pub Extract: Duration,
    pub Identify: Duration,
    pub Lookup: Duration,
    pub Decode: Duration,
    pub Convert: Duration,
    pub Retain: Duration,
    pub EncodedBytes: usize,
    pub DecodedBytes: usize,
    pub Hits: usize,
    pub Misses: usize,
}

impl ImageTimings {
    pub(super) fn Add(&mut self, other: &Self) {
        self.Resolve += other.Resolve;
        self.ExternalRead += other.ExternalRead;
        self.Extract += other.Extract;
        self.Identify += other.Identify;
        self.Lookup += other.Lookup;
        self.Decode += other.Decode;
        self.Convert += other.Convert;
        self.Retain += other.Retain;
        self.EncodedBytes += other.EncodedBytes;
        self.DecodedBytes += other.DecodedBytes;
        self.Hits += other.Hits;
        self.Misses += other.Misses;
    }
}

#[derive(Debug)]
pub struct Texture {
    pub Width: u32,
    pub Height: u32,
    pub Rgba: Vec<u8>,
}

struct CachedImage {
    format: image::ImageFormat,
    encoded: Box<[u8]>,
    rgba8: Arc<Texture>,
}

/// One import's immutable RGBA8 cache. Exact source bytes and effective
/// decoder format are the identity, including for embedded GLB data.
pub(super) struct ImageCache {
    entries: VecDeque<CachedImage>,
    bytes: usize,
    limit: usize,
}

impl Default for ImageCache {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            limit: 64 * 1024 * 1024,
        }
    }
}

impl ImageCache {
    fn Load(
        &mut self,
        bytes: &[u8],
        mime: Option<&str>,
        timing: &mut ImageTimings,
    ) -> Result<Arc<Texture>, String> {
        timing.EncodedBytes += bytes.len();
        let start = Instant::now();
        let format = Identify(bytes, mime)?;
        timing.Identify += start.elapsed();
        let start = Instant::now();
        let hit = self
            .entries
            .iter()
            .find(|entry| entry.format == format && entry.encoded.as_ref() == bytes);
        timing.Lookup += start.elapsed();
        if let Some(entry) = hit {
            timing.Hits += 1;
            return Ok(entry.rgba8.clone());
        }

        timing.Misses += 1;
        let texture = Arc::new(Decode(bytes, format, timing)?);
        timing.DecodedBytes += texture.Rgba.len();
        let start = Instant::now();
        let size = bytes.len() + texture.Rgba.len();
        if size <= self.limit {
            while self.bytes + size > self.limit || self.entries.len() >= 128 {
                let entry = self.entries.pop_front().unwrap();
                self.bytes -= entry.encoded.len() + entry.rgba8.Rgba.len();
            }
            self.entries.push_back(CachedImage {
                format,
                encoded: bytes.into(),
                rgba8: texture.clone(),
            });
            self.bytes += size;
        }
        timing.Retain += start.elapsed();
        Ok(texture)
    }
}

pub(super) struct Source {
    pub document: gltf::Document,
    pub buffers: Vec<buffer::Data>,
    pub timings: ImportTimings,
}

pub(super) fn Resolve(root: &Path, directory: &Path, name: &Path) -> Result<PathBuf, AssetError> {
    if name.as_os_str().is_empty()
        || name
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return Err(AssetError::New(
            name,
            AssetStage::Resolve,
            "asset path must be relative",
        ));
    }
    let candidate = directory.join(name);
    let path = candidate
        .canonicalize()
        .map_err(|error| AssetError::New(&candidate, AssetStage::Resolve, error))?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(AssetError::New(
            name,
            AssetStage::Resolve,
            "asset must be a file inside the project",
        ));
    }
    Ok(path)
}

fn BoundedRead(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    if size > MAX_FILE {
        return Err(format!("{} exceeds the 64 MiB file limit", path.display()));
    }
    let mut bytes = Vec::with_capacity((size + 1) as usize);
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(format!("{} exceeds the 64 MiB file limit", path.display()));
    }
    Ok(bytes)
}

fn UriPath(root: &Path, directory: &Path, uri: &str) -> Result<PathBuf, String> {
    let decoded = urlencoding::decode(uri).map_err(|error| error.to_string())?;
    if decoded.contains([':', '?', '#', '\0']) {
        return Err(format!("unsupported external asset URI '{uri}'"));
    }
    Ok(Resolve(root, directory, Path::new(decoded.as_ref()))?)
}

fn UriBytes(root: &Path, directory: &Path, uri: &str) -> Result<Vec<u8>, String> {
    if uri.starts_with("data:") {
        if !uri.contains(";base64,") {
            return Err("only base64 data URIs are supported".into());
        }
        return buffer::Data::from_source(buffer::Source::Uri(uri), None)
            .map(|data| data.0)
            .map_err(|error| error.to_string());
    }
    BoundedRead(&UriPath(root, directory, uri)?)
}

impl Source {
    pub fn Read(root: &Path, path: &Path) -> Result<Self, AssetError> {
        let now = Instant::now();
        let bytes =
            BoundedRead(path).map_err(|error| AssetError::New(path, AssetStage::Parse, error))?;
        let mut timings = ImportTimings {
            SourceRead: now.elapsed(),
            ..Default::default()
        };
        let now = Instant::now();
        let gltf::Gltf { document, blob } = gltf::Gltf::from_slice(&bytes)
            .map_err(|error| AssetError::New(path, AssetStage::Parse, error))?;
        if let Some(extension) = document.extensions_used().next() {
            return Err(AssetError::New(
                path,
                AssetStage::Parse,
                format!("unsupported extension {extension}"),
            ));
        }
        timings.Parse = now.elapsed();
        let now = Instant::now();
        let buffers = ReadBuffers(&document, blob, root, path.parent().unwrap())
            .map_err(|error| AssetError::New(path, AssetStage::Buffers, error))?;
        timings.Buffers = now.elapsed();
        Ok(Self {
            document,
            buffers,
            timings,
        })
    }

    pub fn DecodeTextures(
        &mut self,
        root: &Path,
        path: &Path,
        cache: &mut ImageCache,
    ) -> Result<Vec<Arc<Texture>>, AssetError> {
        let now = Instant::now();
        let mut images = Vec::new();
        let mut usage = 0;
        for image in self.document.images() {
            let texture = match image.source() {
                gltf_image::Source::View { view, mime_type } => {
                    let start = Instant::now();
                    let offset = view.offset();
                    let bytes =
                        &self.buffers[view.buffer().index()].0[offset..offset + view.length()];
                    self.timings.Images.Extract += start.elapsed();
                    cache.Load(bytes, Some(mime_type), &mut self.timings.Images)
                }
                gltf_image::Source::Uri { uri, mime_type } => DecodeUriTexture(
                    root,
                    path.parent().unwrap(),
                    uri,
                    mime_type,
                    cache,
                    &mut self.timings.Images,
                ),
            }
            .map_err(|error| {
                let index = image.index();
                AssetError::New(
                    path,
                    AssetStage::Textures,
                    format!("image {index}: {error}"),
                )
            })?;
            usage += texture.Rgba.len();
            if usage > MAX_TEXTURES {
                return Err(AssetError::New(
                    path,
                    AssetStage::Textures,
                    "decoded textures exceed the 128 MiB asset budget",
                ));
            }
            images.push(texture);
        }
        self.timings.Textures = now.elapsed();
        Ok(images)
    }
}

fn DecodeUriTexture(
    root: &Path,
    directory: &Path,
    uri: &str,
    mime: Option<&str>,
    cache: &mut ImageCache,
    timing: &mut ImageTimings,
) -> Result<Arc<Texture>, String> {
    let start = Instant::now();
    if let Some(data) = uri.strip_prefix("data:") {
        let bytes = UriBytes(root, directory, uri)?;
        let declared = data.split_once(';').map(|(mime, _)| mime);
        timing.Extract += start.elapsed();
        return cache.Load(&bytes, mime.or(declared), timing);
    }
    let path = UriPath(root, directory, uri)?;
    timing.Resolve += start.elapsed();
    let start = Instant::now();
    let bytes = BoundedRead(&path)?;
    timing.ExternalRead += start.elapsed();
    cache.Load(&bytes, mime, timing)
}

fn ReadBuffers(
    document: &gltf::Document,
    mut blob: Option<Vec<u8>>,
    root: &Path,
    directory: &Path,
) -> Result<Vec<buffer::Data>, String> {
    let mut buffers = Vec::new();
    let mut usage = 0;
    for source in document.buffers() {
        let mut bytes = match source.source() {
            buffer::Source::Bin => blob.take().ok_or("GLB is missing its BIN chunk")?,
            buffer::Source::Uri(uri) => UriBytes(root, directory, uri)?,
        };
        if bytes.len() < source.length() {
            return Err(format!("buffer {} is truncated", source.index()));
        }
        // Use the declared length for validating view offsets. Ignore GLB padding.
        bytes.truncate(source.length());
        usage += bytes.len();
        if usage > MAX_BUFFERS {
            return Err("buffers exceed the 128 MiB asset budget".into());
        }
        buffers.push(buffer::Data(bytes));
    }
    for view in document.views() {
        if view
            .offset()
            .checked_add(view.length())
            .is_none_or(|end| end > buffers[view.buffer().index()].0.len())
        {
            return Err(format!("buffer view {} is out of range", view.index()));
        }
    }
    for accessor in document.accessors() {
        let Some(view) = accessor.view() else {
            return Err(format!("accessor {} has no buffer view", accessor.index()));
        };
        if accessor.sparse().is_some() {
            return Err(format!(
                "accessor {} uses unsupported sparse data",
                accessor.index()
            ));
        }
        let size = accessor.size();
        let stride = view.stride().unwrap_or(size);
        if accessor.count() == 0
            || stride < size
            || (accessor.count() - 1)
                .checked_mul(stride)
                .and_then(|len| len.checked_add(size))
                .and_then(|len| len.checked_add(accessor.offset()))
                .is_none_or(|end| end > view.length())
        {
            return Err(format!("accessor {} is out of range", accessor.index()));
        }
    }
    Ok(buffers)
}

fn Identify(bytes: &[u8], mime: Option<&str>) -> Result<image::ImageFormat, String> {
    let format = match mime {
        Some("image/png") => image::ImageFormat::Png,
        Some("image/jpeg") => image::ImageFormat::Jpeg,
        Some(other) => return Err(format!("unsupported image type '{other}'")),
        None => image::guess_format(bytes).map_err(|error| error.to_string())?,
    };
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err("glTF base color supports PNG and JPEG".into());
    }
    Ok(format)
}

fn Decode(
    bytes: &[u8],
    format: image::ImageFormat,
    timing: &mut ImageTimings,
) -> Result<Texture, String> {
    let start = Instant::now();
    let mut decoder = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    decoder.limits(limits);
    let image = decoder.decode().map_err(|error| error.to_string())?;
    timing.Decode += start.elapsed();
    let start = Instant::now();
    let image = image.into_rgba8();
    let result = Texture {
        Width: image.width(),
        Height: image.height(),
        Rgba: image.into_raw(),
    };
    timing.Convert += start.elapsed();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

    fn Png(color: ExtendedColorType, pixels: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        PngEncoder::new(&mut encoded)
            .write_image(pixels, 2, 1, color)
            .unwrap();
        encoded
    }

    #[test]
    fn image_identity_and_rgba_lanes() {
        let rgba = [11, 21, 31, 0, 128, 132, 246, 63];
        let bytes = Png(ExtendedColorType::Rgba8, &rgba);
        let mut cache = ImageCache::default();
        let mut timing = ImageTimings::default();
        let image = cache.Load(&bytes, None, &mut timing).unwrap();
        assert_eq!((image.Width, image.Height), (2, 1));
        assert_eq!(image.Rgba, rgba);
        let duplicate = cache.Load(&bytes, Some("image/png"), &mut timing).unwrap();
        assert!(Arc::ptr_eq(&image, &duplicate));
        let edited = Png(ExtendedColorType::Rgb8, &[11, 21, 31, 128, 132, 246]);
        let opaque = cache.Load(&edited, None, &mut timing).unwrap();
        assert_eq!(opaque.Rgba, [11, 21, 31, 255, 128, 132, 246, 255]);
        assert!(!Arc::ptr_eq(&image, &opaque));
        assert_eq!(
            (timing.Hits, timing.Misses, timing.DecodedBytes),
            (1, 2, 16)
        );
    }

    #[test]
    fn corrupted_input_never_becomes_a_hit_and_cache_stays_bounded() {
        let good = Png(ExtendedColorType::Rgba8, &[20; 8]);
        let second = Png(ExtendedColorType::Rgba8, &[40; 8]);
        let mut corrupt = good.clone();
        corrupt.truncate(35);
        let mut cache = ImageCache {
            limit: good.len().max(second.len()) + 8,
            ..Default::default()
        };
        let mut timing = ImageTimings::default();
        assert!(
            cache
                .Load(&corrupt, Some("image/png"), &mut timing)
                .is_err()
        );
        assert!(cache.Load(&good, Some("image/jpeg"), &mut timing).is_err());
        assert_eq!(cache.entries.len(), 0);
        let live = cache.Load(&good, None, &mut timing).unwrap();
        cache.Load(&second, None, &mut timing).unwrap();
        assert_eq!(cache.entries.len(), 1);
        assert!(cache.bytes <= cache.limit);
        assert_eq!(live.Rgba, [20; 8]);
        let reloaded = cache.Load(&good, None, &mut timing).unwrap();
        assert!(!Arc::ptr_eq(&live, &reloaded));
        assert_eq!(live.Rgba, reloaded.Rgba);
    }
}
