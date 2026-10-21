use image::RgbaImage;
use std::collections::HashSet;
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::UNIX_EPOCH;

const CACHE_MAGIC: &[u8; 4] = b"SWTH";

pub struct LoadedImage {
    pub index: usize,
    pub result: Result<(RgbaImage, u32, u32), String>,
}

fn get_cache_dir() -> Option<PathBuf> {
    let dir = if let Ok(cache) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(cache).join("swall/thumbnails")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache/swall/thumbnails")
    } else {
        return None;
    };
    let _ = fs::create_dir_all(&dir);
    Some(dir)
}

fn compute_cache_key(path: &Path, mtime_nanos: u128, file_size: u64, max_dim: u32) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    mtime_nanos.hash(&mut hasher);
    file_size.hash(&mut hasher);
    max_dim.hash(&mut hasher);
    format!("{:016x}.thumb", hasher.finish())
}

pub fn save_thumbnail_cache(
    cache_path: &Path,
    img: &RgbaImage,
    orig_w: u32,
    orig_h: u32,
) -> std::io::Result<()> {
    let tmp_path = cache_path.with_extension("tmp");
    let mut file = File::create(&tmp_path)?;
    file.write_all(CACHE_MAGIC)?;
    file.write_all(&img.width().to_le_bytes())?;
    file.write_all(&img.height().to_le_bytes())?;
    file.write_all(&orig_w.to_le_bytes())?;
    file.write_all(&orig_h.to_le_bytes())?;
    file.write_all(img.as_raw())?;
    file.flush()?;
    fs::rename(tmp_path, cache_path)?;
    Ok(())
}

pub fn load_thumbnail_cache(cache_path: &Path) -> std::io::Result<(RgbaImage, u32, u32)> {
    let mut file = File::open(cache_path)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != CACHE_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid cache magic",
        ));
    }
    let mut num_buf = [0u8; 4];

    file.read_exact(&mut num_buf)?;
    let w = u32::from_le_bytes(num_buf);

    file.read_exact(&mut num_buf)?;
    let h = u32::from_le_bytes(num_buf);

    file.read_exact(&mut num_buf)?;
    let orig_w = u32::from_le_bytes(num_buf);

    file.read_exact(&mut num_buf)?;
    let orig_h = u32::from_le_bytes(num_buf);

    let expected_len = (w as usize) * (h as usize) * 4;
    let mut data = vec![0u8; expected_len];
    file.read_exact(&mut data)?;

    let img = RgbaImage::from_raw(w, h, data).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Corrupt raw image in cache",
        )
    })?;

    Ok((img, orig_w, orig_h))
}

pub fn load_image_thumbnail(
    path: &Path,
    max_dim: u32,
    use_cache: bool,
) -> Result<(RgbaImage, u32, u32), String> {
    let cache_file = if use_cache {
        if let (Some(cache_dir), Ok(meta)) = (get_cache_dir(), fs::metadata(path)) {
            let mtime_nanos = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let key = compute_cache_key(path, mtime_nanos, meta.len(), max_dim);
            let file = cache_dir.join(key);
            if file.exists()
                && let Ok(cached) = load_thumbnail_cache(&file)
            {
                return Ok(cached);
            }
            Some(file)
        } else {
            None
        }
    } else {
        None
    };

    // Decode original image
    let reader = image::ImageReader::open(path)
        .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to guess format for {}: {}", path.display(), e))?;

    let img = reader
        .decode()
        .map_err(|e| format!("Failed to decode {}: {}", path.display(), e))?;

    let (orig_w, orig_h) = (img.width(), img.height());

    let (thumb_rgba, _w, _h) = if orig_w > max_dim || orig_h > max_dim {
        let thumb = img.thumbnail(max_dim, max_dim);
        let tw = thumb.width();
        let th = thumb.height();
        (thumb.to_rgba8(), tw, th)
    } else {
        (img.to_rgba8(), orig_w, orig_h)
    };

    if let Some(cf) = cache_file {
        let _ = save_thumbnail_cache(&cf, &thumb_rgba, orig_w, orig_h);
    }

    Ok((thumb_rgba, orig_w, orig_h))
}

pub struct AsyncImageLoader {
    request_tx: Sender<usize>,
    result_rx: Receiver<LoadedImage>,
    requested: HashSet<usize>,
}

impl AsyncImageLoader {
    pub fn new(wallpapers: Vec<PathBuf>, max_dim: u32, use_cache: bool) -> Self {
        let (request_tx, request_rx) = channel::<usize>();
        let (result_tx, result_rx) = channel::<LoadedImage>();

        thread::spawn(move || {
            while let Ok(index) = request_rx.recv() {
                if let Some(path) = wallpapers.get(index) {
                    let result = load_image_thumbnail(path, max_dim, use_cache);
                    let _ = result_tx.send(LoadedImage { index, result });
                }
            }
        });

        Self {
            request_tx,
            result_rx,
            requested: HashSet::new(),
        }
    }

    pub fn request(&mut self, index: usize) {
        if self.requested.insert(index) {
            let _ = self.request_tx.send(index);
        }
    }

    pub fn try_recv(&self) -> Option<LoadedImage> {
        self.result_rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_cache_roundtrip() {
        let temp_dir =
            std::env::temp_dir().join(format!("swall_thumb_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let cache_file = temp_dir.join("test.thumb");

        let mut img = RgbaImage::new(160, 90);
        img.put_pixel(10, 10, image::Rgba([255, 128, 64, 255]));

        save_thumbnail_cache(&cache_file, &img, 1920, 1080).unwrap();
        let (loaded, ow, oh) = load_thumbnail_cache(&cache_file).unwrap();

        assert_eq!(loaded.width(), 160);
        assert_eq!(loaded.height(), 90);
        assert_eq!(ow, 1920);
        assert_eq!(oh, 1080);
        assert_eq!(loaded.get_pixel(10, 10).0, [255, 128, 64, 255]);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
