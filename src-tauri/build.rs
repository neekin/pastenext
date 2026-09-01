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

    tauri_build::build()
}

/// 用 xcrun swiftc 把 ocr/ocr.swift 编译成 ocr_helper。失败不致命:
/// 只是图片 OCR 不可用,其余功能照常。产物已存在且比源码新则跳过以加速重建。
#[cfg(target_os = "macos")]
fn build_ocr_helper() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ocr/ocr.swift");

    let out = std::path::Path::new("ocr_helper");
    let src = std::path::Path::new("ocr/ocr.swift");
    let need = match (out.exists(), src.exists()) {
        (false, true) => true,
        (true, true) => {
            let om = std::fs::metadata(out)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let sm = std::fs::metadata(src)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            om < sm
        }
        _ => false,
    };
    if !need {
        return;
    }

    eprintln!("[build] 编译 OCR 辅助二进制 (Swift / Vision) …");
    let status = std::process::Command::new("xcrun")
        .args([
            "swiftc",
            "-O",
            "-framework",
            "Vision",
            "-framework",
            "Foundation",
            "-framework",
            "ImageIO",
            "ocr/ocr.swift",
            "-o",
            "ocr_helper",
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            // 与主程序一起做 ad-hoc 签名,避免受 Hardened Runtime 影响无法调用
            let _ = std::process::Command::new("codesign")
                .args(["--force", "--sign", "-", "ocr_helper"])
                .status();
        }
        _ => {
            println!("cargo:warning=OCR 辅助二进制编译失败,图片 OCR 将不可用(应用其余功能正常)");
        }
    }
}
