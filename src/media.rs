//! Owned, bounded static question images. The library stores identifiers only.
use crate::{knowledge::AppResult, tr};
use image::{ImageFormat, ImageReader, imageops::FilterType};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

pub const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 3 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 512 * 1024;
pub const MAX_BUNDLE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_BUNDLE_MEDIA_BYTES: usize = 16 * 1024 * 1024;

pub fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub struct NormalizedImage {
    pub id: String,
    pub png: Vec<u8>,
}

fn reader(bytes: &[u8], pixels: u64) -> AppResult<ImageReader<Cursor<&[u8]>>> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| tr!("图片无法读取，请选择 PNG 或 JPEG。"))?;
    if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
        return Err(tr!("图片仅支持 PNG 和 JPEG。").into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(pixels * 8 + 8 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader)
}

fn dimensions(bytes: &[u8], max_pixels: u64) -> AppResult<(u32, u32)> {
    let size = reader(bytes, max_pixels)?
        .into_dimensions()
        .map_err(|_| tr!("图片尺寸无法读取。"))?;
    if size.0 == 0
        || size.1 == 0
        || size.0 > 8192
        || size.1 > 8192
        || u64::from(size.0) * u64::from(size.1) > max_pixels
    {
        return Err(tr!("图片像素过大，请先裁剪或缩小。").into());
    }
    Ok(size)
}

pub fn normalize_file(path: &Path) -> AppResult<NormalizedImage> {
    normalize(&crate::storage::read_bounded(path, MAX_SOURCE_BYTES)?)
}

pub fn normalize(bytes: &[u8]) -> AppResult<NormalizedImage> {
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err(tr!("图片文件超过 8 MiB，请先缩小。").into());
    }
    let (width, height) = dimensions(bytes, 12_000_000)?;
    let image = reader(bytes, 12_000_000)?
        .decode()
        .map_err(|_| tr!("图片解码失败，请换一张图片。"))?;
    let scale = 1.0_f64
        .min(1200.0 / f64::from(width))
        .min(900.0 / f64::from(height))
        .min((MAX_PIXELS as f64 / (f64::from(width) * f64::from(height))).sqrt());
    let image = if scale < 1.0 {
        image.resize(
            (f64::from(width) * scale).floor().max(1.0) as u32,
            (f64::from(height) * scale).floor().max(1.0) as u32,
            FilterType::Triangle,
        )
    } else {
        image
    };
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|_| tr!("图片转换失败。"))?;
    let png = png.into_inner();
    if png.len() as u64 > MAX_IMAGE_BYTES {
        return Err(tr!("处理后的图片仍然过大，请简化图片。").into());
    }
    Ok(NormalizedImage {
        id: format!("{:x}", Sha256::digest(&png)),
        png,
    })
}

pub struct LoadedImage {
    pub image: slint::Image,
    pub width: u32,
    pub height: u32,
}

pub struct MediaStore {
    directory: PathBuf,
}

impl MediaStore {
    pub fn new(data_directory: &Path) -> Self {
        Self {
            directory: data_directory.join("media"),
        }
    }

    fn path(&self, id: &str) -> AppResult<PathBuf> {
        if !valid_id(id) {
            return Err(tr!("图片资源标识无效。").into());
        }
        if let Ok(metadata) = fs::symlink_metadata(&self.directory)
            && (!metadata.is_dir() || metadata.file_type().is_symlink())
        {
            return Err(tr!("图片目录不是有效的本地目录。").into());
        }
        let path = self.directory.join(format!("{id}.png"));
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(tr!("图片资源不是有效的本地文件。").into());
        }
        Ok(path)
    }

    pub fn read(&self, id: &str) -> AppResult<Vec<u8>> {
        let bytes = crate::storage::read_bounded(&self.path(id)?, MAX_IMAGE_BYTES)?;
        if format!("{:x}", Sha256::digest(&bytes)) != id {
            return Err(tr!("图片资源校验失败，请重新导入图片。").into());
        }
        Ok(bytes)
    }

    pub fn size(&self, id: &str) -> AppResult<u64> {
        let size = fs::metadata(self.path(id)?)
            .map_err(|_| tr!("题面图片缺失，请重新添加或导入知识包。"))?
            .len();
        if size > MAX_IMAGE_BYTES {
            return Err(tr!("知识包图片资源过大，请拆分知识包。").into());
        }
        Ok(size)
    }

    pub fn install(&self, id: &str, png: &[u8]) -> AppResult<()> {
        let path = self.path(id)?;
        if png.len() as u64 > MAX_IMAGE_BYTES || format!("{:x}", Sha256::digest(png)) != id {
            return Err(tr!("图片资源校验失败，请重新导入图片。").into());
        }
        if image::guess_format(png).ok() != Some(ImageFormat::Png) {
            return Err(tr!("知识包图片必须是 PNG。").into());
        }
        dimensions(png, MAX_PIXELS)?;
        reader(png, MAX_PIXELS)?
            .decode()
            .map_err(|_| tr!("图片解码失败，请换一张图片。"))?;
        fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        if self.read(id).is_ok() {
            return Ok(());
        }
        crate::storage::write_atomic(&path, png)
    }

    pub fn load(&self, id: &str) -> AppResult<LoadedImage> {
        let bytes = self.read(id)?;
        let (width, height) = dimensions(&bytes, MAX_PIXELS)?;
        let image = reader(&bytes, MAX_PIXELS)?
            .decode()
            .map_err(|_| tr!("图片解码失败，请换一张图片。"))?;
        let rgba = image.into_rgba8();
        let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            rgba.as_raw(),
            width,
            height,
        );
        Ok(LoadedImage {
            image: slint::Image::from_rgba8(pixels),
            width,
            height,
        })
    }

    /// Only remove recognized, unreferenced regular files, never directories.
    pub fn collect(&self, keep: &BTreeSet<String>) {
        if self.path(&"0".repeat(64)).is_err() {
            return;
        }
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|s| s.strip_suffix(".png")) else {
                continue;
            };
            if valid_id(id)
                && !keep.contains(id)
                && entry
                    .file_type()
                    .is_ok_and(|t| t.is_file() && !t.is_symlink())
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(w, h)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }
    #[test]
    fn normalize_bounds_pixels_and_rejects_untrusted_input() {
        let normalized = normalize(&png(1600, 900)).unwrap();
        let size = dimensions(&normalized.png, MAX_PIXELS).unwrap();
        assert!(size.0 <= 1200 && size.1 <= 900);
        assert!(normalize(b"<svg></svg>").is_err());
        assert!(normalize(&vec![0; MAX_SOURCE_BYTES as usize + 1]).is_err());
        assert!(dimensions(&png(8193, 1), 12_000_000).is_err());
        let mut jpeg = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(30, 20)
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .unwrap();
        let converted = normalize(jpeg.get_ref()).unwrap();
        assert_eq!(
            image::guess_format(&converted.png).unwrap(),
            ImageFormat::Png
        );
        assert_eq!(dimensions(&converted.png, MAX_PIXELS).unwrap(), (30, 20));
        let mut alpha = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            4,
            image::Rgba([14, 100, 200, 80]),
        ))
        .write_to(&mut alpha, ImageFormat::Png)
        .unwrap();
        let preserved = normalize(alpha.get_ref()).unwrap();
        assert_eq!(
            image::load_from_memory(&preserved.png)
                .unwrap()
                .into_rgba8()
                .get_pixel(0, 0)
                .0,
            [14, 100, 200, 80]
        );
    }
    #[test]
    fn owned_images_survive_source_removal_and_collect_only_unreferenced_files() {
        let temp = tempfile::tempdir().unwrap();
        let store = MediaStore::new(temp.path());
        let image = normalize(&png(20, 10)).unwrap();
        store.install(&image.id, &image.png).unwrap();
        assert_eq!(store.load(&image.id).unwrap().image.size().width, 20);
        assert!(store.install("../escape", &image.png).is_err());
        assert!(store.install(&"0".repeat(64), &image.png).is_err());
        store.collect(&BTreeSet::from([image.id.clone()]));
        assert!(store.read(&image.id).is_ok());
        fs::write(temp.path().join("media/keep.txt"), b"keep").unwrap();
        store.collect(&BTreeSet::new());
        assert!(store.read(&image.id).is_err());
        assert!(temp.path().join("media/keep.txt").exists());
    }
}
