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

    tauri_build::build()
}
