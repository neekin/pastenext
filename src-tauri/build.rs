fn main() {
    // 授权密钥在编译期注入(见 src/license.rs)。正式发版必须设置这两个环境变量,
    // 否则应用会用仓库里的开发占位密钥,任何人都能自己签发序列号。
    let released = std::env::var("PASTENEXT_RELEASE").is_ok();

    // 必须显式声明,否则改了环境变量 Cargo 不会重跑本脚本,
    // 发版时的密钥检查会被上一次的缓存结果跳过。
    println!("cargo:rerun-if-env-changed=PASTENEXT_RELEASE");
    println!("cargo:rerun-if-env-changed=PASTENEXT_PURCHASE_URL");

    for var in ["PASTENEXT_SIGN_SECRET", "PASTENEXT_MAIL_SECRET"] {
        println!("cargo:rerun-if-env-changed={var}");
        match std::env::var(var) {
            Ok(v) if !v.is_empty() => {
                println!("cargo:rustc-env={var}={v}");
            }
            _ => {
                if released {
                    panic!(
                        "\n\n{var} 未设置。PASTENEXT_RELEASE 已开启,拒绝用开发占位密钥发版。\n\
                         在 CI 中配置该环境变量后重新构建。\n"
                    );
                }
                println!("cargo:warning=未设置 {var},将使用开发占位密钥 —— 不要用于正式发版");
            }
        }
    }
    if let Ok(url) = std::env::var("PASTENEXT_PURCHASE_URL") {
        if !url.is_empty() {
            println!("cargo:rustc-env=PASTENEXT_PURCHASE_URL={url}");
        }
    }

    // 编译 macOS OCR 辅助二进制(Swift + Vision 框架),仅在 macOS 上执行。
    // 产物 ocr_helper 放在 src-tauri 根目录,开发期直接被 resource_dir() 找到,
    // 打包时由 tauri.conf.json 的 bundle.resources 拷进 Contents/Resources。
    #[cfg(target_os = "macos")]
    build_ocr_helper();
    #[cfg(target_os = "macos")]
    build_media_helper();
    #[cfg(target_os = "macos")]
    build_helper(
        "capture/selection.swift",
        "capture_selector",
        &["AppKit", "CoreGraphics"],
    );

    #[cfg(target_os = "windows")]
    build_windows_selector();

    tauri_build::build();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Tauri links the Common Controls v6 manifest only into application
        // binaries. GUI examples need it too, or TaskDialogIndirect cannot be
        // resolved from the default Common Controls DLL before main starts.
        let resource = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("resource.lib");
        println!("cargo:rustc-link-arg-examples={}", resource.display());
    }
}

#[cfg(target_os = "macos")]
fn build_helper(source: &str, output: &str, frameworks: &[&str]) {
    println!("cargo:rerun-if-changed={source}");
    // Cargo TARGET, not host architecture: Intel releases may be built on Apple Silicon.
    let arch = if std::env::var("TARGET")
        .unwrap_or_default()
        .starts_with("x86_64")
    {
        "x86_64"
    } else {
        "arm64"
    };
    let target = format!("{arch}-apple-macosx11.0");
    let mut command = std::process::Command::new("xcrun");
    command.args(["swiftc", "-O", "-target", &target]);
    if source == "capture/selection.swift" {
        command.arg("-parse-as-library");
    }
    for framework in frameworks {
        command.args(["-framework", framework]);
    }
    let status = command
        .args([source, "-o", output])
        .status()
        .expect("Unable to run Swift compiler");
    assert!(
        status.success(),
        "Native helper compilation failed: {source}"
    );
    let status = std::process::Command::new("codesign")
        .args(["--force", "--sign", "-", output])
        .status()
        .expect("Unable to sign helper");
    assert!(status.success(), "Unable to sign native helper: {output}");
}
#[cfg(target_os = "macos")]
fn build_ocr_helper() {
    build_helper(
        "ocr/ocr.swift",
        "ocr_helper",
        &["Vision", "Foundation", "ImageIO"],
    );
}
#[cfg(target_os = "macos")]
fn build_media_helper() {
    build_helper(
        "media/export.swift",
        "media_export",
        &["AVFoundation", "ImageIO"],
    );
}

#[cfg(target_os = "windows")]
fn build_windows_selector() {
    println!("cargo:rerun-if-changed=capture/selection_windows.cpp");
    let compiler = cc::Build::new().cpp(true).static_crt(true).get_compiler();
    assert!(
        compiler.is_like_msvc(),
        "Windows capture selector requires MSVC"
    );
    let status = compiler
        .to_command()
        .args([
            "/nologo",
            "/std:c++17",
            "/O2",
            "/EHsc",
            "/utf-8",
            "/DUNICODE",
            "/D_UNICODE",
            "capture/selection_windows.cpp",
            "/Fe:capture_selector.exe",
            "/link",
            "user32.lib",
            "gdi32.lib",
            "msimg32.lib",
            "dwmapi.lib",
            "shcore.lib",
        ])
        .status()
        .expect("Unable to compile Windows capture selector");
    assert!(
        status.success(),
        "Windows capture selector compilation failed"
    );
}
