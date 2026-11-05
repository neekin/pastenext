//! Conservative vertical scroll stitching. Ambiguous matches require user adjustment.
use crate::{
    capture::{self, MediaAsset},
    util,
};
use image::{GenericImage, RgbaImage};
use std::{collections::HashSet, io::Cursor};
use tauri::AppHandle;

fn score(a: &RgbaImage, b: &RgbaImage, overlap: u32) -> (f64, f64) {
    let mut error = 0.;
    let mut sum = 0.;
    let mut square = 0.;
    let mut n = 0.;
    for iy in 0..24 {
        let y = iy * (overlap - 1) / 23;
        for ix in 0..40 {
            let x = (a.width() / 20 + ix * (a.width() * 17 / 20) / 39).min(a.width() - 1);
            let p = a.get_pixel(x, a.height() - overlap + y);
            let q = b.get_pixel(x, y);
            for c in 0..3 {
                let v = q[c] as f64;
                error += (p[c] as f64 - v).abs();
                sum += v;
                square += v * v;
                n += 1.;
            }
        }
    }
    (error / n, (square / n - (sum / n).powi(2)).max(0.))
}
fn overlap(a: &RgbaImage, b: &RgbaImage) -> Option<u32> {
    if a.width() != b.width() || a.height().min(b.height()) < 64 {
        return None;
    }
    let limit = a.height().min(b.height()) - 16;
    // Search every pixel: skipping odd offsets loses matches for sharp text.
    // Require texture and a distinct minimum.
    let mut candidates: Vec<_> = (32..=limit)
        .map(|n| {
            let (e, v) = score(a, b, n);
            (n, e, v)
        })
        .collect();
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (n, error, variance) = candidates[0];
    let next = candidates
        .iter()
        .find(|c| c.0.abs_diff(n) > 8)
        .map(|c| c.1)
        .unwrap_or(255.);
    if error <= 8. && variance >= 100. && next > error + 3. {
        Some(n)
    } else {
        None
    }
}
fn join(frames: &[RgbaImage], overlaps: &[u32]) -> Result<RgbaImage, String> {
    if frames.len() < 2 || overlaps.len() != frames.len() - 1 {
        return Err("至少需要两帧，并确认每处接缝".into());
    }
    let width = frames[0].width();
    let mut height = frames[0].height() as u64;
    for (i, frame) in frames.iter().enumerate().skip(1) {
        let n = overlaps[i - 1];
        if frame.width() != width {
            return Err("各帧宽度不同，请截取同一宽度的滚动区域".into());
        }
        if n >= frame.height() || n > frames[i - 1].height() {
            return Err("重叠高度必须小于下一帧高度".into());
        }
        height += (frame.height() - n) as u64;
    }
    if height * width as u64 > 64_000_000 || height > 32767 {
        return Err("长图超过 6400 万像素或 32767 像素高度，请分段截取".into());
    }
    let mut out = RgbaImage::new(width, height as u32);
    out.copy_from(&frames[0], 0, 0).map_err(|e| e.to_string())?;
    let mut y = frames[0].height();
    for (frame, n) in frames.iter().skip(1).zip(overlaps) {
        let tail = image::imageops::crop_imm(frame, 0, *n, width, frame.height() - n);
        out.copy_from(&*tail, 0, y).map_err(|e| e.to_string())?;
        y += frame.height() - n;
    }
    Ok(out)
}
fn load(app: &AppHandle, ids: &[String]) -> Result<Vec<RgbaImage>, String> {
    if !(2..=12).contains(&ids.len()) {
        return Err("请选择 2 至 12 帧".into());
    }
    if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        return Err("不能重复添加同一帧".into());
    }
    let mut pixels = 0u64;
    let mut hashes = HashSet::new();
    let mut frames: Vec<RgbaImage> = Vec::new();
    for id in ids {
        let item = capture::asset(app, id)?;
        if item.kind != "image" {
            return Err("长图仅支持图片".into());
        }
        pixels += item.width as u64 * item.height as u64;
        if pixels > 64_000_000 {
            return Err("所有帧合计不能超过 6400 万像素，请分段拼接".into());
        }
        let frame = image::open(item.path)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        if !hashes.insert(util::hash_rgba(
            frame.width(),
            frame.height(),
            frame.as_raw(),
        )) {
            return Err("检测到重复画面，请滚动后再添加下一帧".into());
        }
        if !frames.is_empty() && frames[0].width() != frame.width() {
            return Err("各帧宽度不同，请截取同一宽度的滚动区域".into());
        }
        frames.push(frame);
    }
    Ok(frames)
}
#[tauri::command]
pub async fn media_stitch_analyze(
    app: AppHandle,
    ids: Vec<String>,
) -> Result<Vec<Option<u32>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let frames = load(&app, &ids)?;
        Ok(frames.windows(2).map(|p| overlap(&p[0], &p[1])).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn media_stitch(
    app: AppHandle,
    ids: Vec<String>,
    overlaps: Vec<u32>,
) -> Result<MediaAsset, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let frames = load(&app, &ids)?;
        let output = join(&frames, &overlaps)?;
        let mut bytes = Cursor::new(Vec::new());
        output
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        capture::import_image_bytes(&app, bytes.get_ref(), "长截图")
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    fn page() -> RgbaImage {
        RgbaImage::from_fn(120, 480, |x, y| {
            let v =
                (x.wrapping_mul(747796405) ^ y.wrapping_mul(2891336453)).wrapping_mul(277803737);
            image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
        })
    }
    #[test]
    fn reconstructs_scroll_without_duplicate_rows() {
        let p = page();
        let a = image::imageops::crop_imm(&p, 0, 0, 120, 300).to_image();
        let b = image::imageops::crop_imm(&p, 0, 180, 120, 300).to_image();
        assert_eq!(overlap(&a, &b), Some(120));
        assert_eq!(join(&[a, b], &[120]).unwrap(), p);
    }
    #[test]
    fn recognizes_odd_pixel_scroll() {
        let p = page();
        let a = image::imageops::crop_imm(&p, 0, 0, 120, 300).to_image();
        let b = image::imageops::crop_imm(&p, 0, 179, 120, 300).to_image();
        assert_eq!(overlap(&a, &b), Some(121));
    }
    #[test]
    fn ambiguous_blank_frames_require_manual_alignment() {
        let a = RgbaImage::from_pixel(100, 200, image::Rgba([255, 255, 255, 255]));
        assert_eq!(overlap(&a, &a), None);
    }
    #[test]
    fn refuses_invalid_seams_and_widths() {
        let a = page();
        let b = RgbaImage::new(121, 480);
        assert!(join(&[a.clone(), b], &[100]).is_err());
        assert!(join(&[a.clone(), a], &[480]).is_err());
    }
}
