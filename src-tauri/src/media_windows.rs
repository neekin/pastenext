//! Packaged, offline Windows media engine. All paths are passed as argv, never shell text.
use crate::capture::{CaptureOptions, NativeSelection};
use std::{
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Output},
};
const CREATE_NO_WINDOW: u32 = 0x08000000;
pub fn hidden(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}
fn command(resources: &Path) -> Result<Command, String> {
    let executable = resources.join("ffmpeg").join("ffmpeg.exe");
    if !executable.is_file() {
        return Err("Windows 媒体引擎缺失，请重新安装完整客户端".into());
    }
    let mut c = Command::new(executable);
    hidden(&mut c);
    c.args(["-hide_banner", "-loglevel", "error", "-nostats", "-y"]);
    Ok(c)
}
pub fn capture(
    resources: &Path,
    options: &CaptureOptions,
    selected: &NativeSelection,
    video: bool,
) -> Result<Command, String> {
    selected.validate()?;
    let r = selected.rect.as_ref().ok_or("没有选择截图范围")?;
    let mut c = command(resources)?;
    c.args([
        "-f",
        "gdigrab",
        "-framerate",
        if video { "30" } else { "1" },
        "-draw_mouse",
        if video || options.cursor { "1" } else { "0" },
    ]);
    if selected.mode.as_deref() == Some("window") && !video {
        c.arg("-i")
            .arg(format!("hwnd={}", selected.window_id.unwrap()));
    } else {
        c.arg("-offset_x")
            .arg(format!("{:.0}", r.x))
            .arg("-offset_y")
            .arg(format!("{:.0}", r.y))
            .arg("-video_size")
            .arg(format!("{:.0}x{:.0}", r.width, r.height))
            .args(["-i", "desktop"]);
    }
    if video {
        c.args([
            "-an",
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-c:v",
            "libopenh264",
            "-b:v",
            "8000000",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
        ]);
    } else {
        c.args(["-frames:v", "1", "-c:v", "png"]);
    }
    Ok(c)
}
fn input_duration(resources: &Path, input: &Path) -> Result<f64, String> {
    let mut c = command(resources)?;
    // ffmpeg prints container metadata and exits without decoding when no output is supplied.
    c.args(["-loglevel", "info", "-i"]).arg(input);
    let out = c.output().map_err(|e| e.to_string())?;
    let log = String::from_utf8_lossy(&out.stderr);
    let value = log
        .split("Duration: ")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .ok_or("无法读取视频时长")?;
    let parts: Vec<f64> = value
        .split(':')
        .map(str::parse::<f64>)
        .collect::<Result<_, _>>()
        .map_err(|_| "视频时长无效")?;
    if parts.len() != 3 {
        return Err("视频时长无效".into());
    }
    Ok(parts[0] * 3600. + parts[1] * 60. + parts[2])
}
pub fn export(
    resources: &Path,
    input: &Path,
    output: &Path,
    format: &str,
    quality: u8,
) -> Result<(), String> {
    if !["mp4", "mov", "gif"].contains(&format) {
        return Err("录屏请选择 MP4、MOV 或 GIF".into());
    }
    if format == "gif" && input_duration(resources, input)? > 120. {
        return Err("GIF 最长支持 120 秒，请改用 MP4 或 MOV 导出较长录屏".into());
    }
    let mut c = command(resources)?;
    c.arg("-i").arg(input);
    if format == "gif" {
        let side = if quality >= 90 { 960 } else { 640 };
        c.arg("-filter_complex").arg(format!("fps=10,scale={side}:{side}:force_original_aspect_ratio=decrease:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse")).args(["-loop","0","-f","gif"]);
    } else {
        c.args([
            "-an",
            "-c:v",
            "libopenh264",
            "-b:v",
            if quality >= 90 { "8000000" } else { "3000000" },
            "-pix_fmt",
            "yuv420p",
        ]);
        c.args([
            "-vf",
            if quality >= 90 {
                "pad=ceil(iw/2)*2:ceil(ih/2)*2"
            } else {
                "scale=1280:720:force_original_aspect_ratio=decrease:force_divisible_by=2"
            },
        ]);
        c.args(["-movflags", "+faststart", "-f", format]);
    }
    let result = c.arg(output).output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        let _ = std::fs::remove_file(output);
        return Err(error(&result));
    }
    Ok(())
}
pub fn error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    if message.is_empty() {
        "Windows 媒体引擎未完成操作，请检查选区或输出目录".into()
    } else {
        message.chars().take(4000).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resources() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }
    #[test]
    fn windows_media_engine_captures_and_exports_real_files() {
        let resources = resources();
        let root =
            std::env::temp_dir().join(format!("pastenext-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let input = root.join("fixture.mp4");
        let output = command(&resources)
            .unwrap()
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=10",
                "-t",
                "1.2",
                "-c:v",
                "libopenh264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", error(&output));
        for format in ["mp4", "mov", "gif"] {
            let path = root.join(format!("output.{format}"));
            export(&resources, &input, &path, format, 90).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.len() > 1000);
            if format == "gif" {
                assert!(bytes.starts_with(b"GIF"));
                use image::AnimationDecoder;
                let gif = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(
                    std::fs::File::open(path).unwrap(),
                ))
                .unwrap();
                assert!(gif.into_frames().collect_frames().unwrap().len() >= 10);
            } else {
                assert_eq!(&bytes[4..8], b"ftyp");
                if format == "mov" {
                    assert_eq!(&bytes[8..12], b"qt  ");
                }
            }
        }
        let selected:NativeSelection=serde_json::from_value(serde_json::json!({"mode":"region","display":1,"rect":{"x":0,"y":0,"width":160,"height":120}})).unwrap();
        let options = CaptureOptions {
            mode: "region".into(),
            display: 1,
            delay: 0,
            cursor: false,
            clicks: false,
        };
        let shot = root.join("desktop.png");
        let output = capture(&resources, &options, &selected, false)
            .unwrap()
            .arg(&shot)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", error(&output));
        let image = image::open(shot).unwrap();
        assert_eq!((image.width(), image.height()), (160, 120));
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn stopping_windows_recording_finalizes_the_movie() {
        use std::io::Write;
        use std::process::Stdio;
        let resources = resources();
        let path =
            std::env::temp_dir().join(format!("pastenext-stop-test-{}.mp4", std::process::id()));
        let selected:NativeSelection=serde_json::from_value(serde_json::json!({"mode":"region","display":1,"rect":{"x":0,"y":0,"width":320,"height":180}})).unwrap();
        let options = CaptureOptions {
            mode: "region".into(),
            display: 1,
            delay: 0,
            cursor: false,
            clicks: false,
        };
        let mut child = capture(&resources, &options, &selected, true)
            .unwrap()
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        child.stdin.as_mut().unwrap().write_all(b"q\n").unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{}", error(&out));
        assert!(input_duration(&resources, &path).unwrap() > 0.5);
        let _ = std::fs::remove_file(path);
    }
}
