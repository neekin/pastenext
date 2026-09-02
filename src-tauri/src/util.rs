use sha2::{Digest, Sha256};

pub fn sha_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn hash_text(t: &str) -> String {
    format!("t:{}", sha_hex(t.as_bytes()))
}

/// 图片按解码后的 RGBA 像素做指纹,这样同一路内容经过
/// PNG→剪贴板→TIFF→PNG 的往返后哈希仍一致,不会产生重复条目
pub fn hash_rgba(w: u32, h: u32, rgba: &[u8]) -> String {
    format!("i:{w}x{h}:{}", sha_hex(rgba))
}

pub fn hash_files(paths: &[String]) -> String {
    let mut sorted = paths.to_vec();
    sorted.sort();
    format!("f:{}", sha_hex(sorted.join("\n").as_bytes()))
}
