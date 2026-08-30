//! 授权与试用管理。
//!
//! 设计要点:
//! 1. **离线校验**。序列号自带签名,本地即可验真,不需要联网服务器,
//!    也不需要在应用中内置用户名单。代价是无法抵御逆向 —— 这是所有
//!    离线授权方案的共同局限,目的是挡住随手伪造而非专业破解。
//! 2. **密钥不进仓库**。`SIGN_SECRET` / `MAIL_SECRET` 在**编译期**从环境变量
//!    注入(见 `build.rs`),仓库里只有开发用的占位值。正式发版必须在 CI 中
//!    设置 `PASTENEXT_SIGN_SECRET` / `PASTENEXT_MAIL_SECRET`,否则 `build.rs`
//!    会打印警告。
//! 3. **试用判定交给前端**。Rust 只提供原始时间戳,`src/license/useLicense.ts`
//!    用本地时区换算"剩余天数"和"今天是否已弹过窗",避免出现 UTC 凌晨换日
//!    导致的误差。
//!
//! 序列号结构(10 字节 → Base32 16 字符 → `XXXX-XXXX-XXXX-XXXX`):
//! ```text
//! [0]      version  1 字节,当前为 1
//! [1..5]   mail     4 字节,HMAC(MAIL_SECRET, 归一化邮箱) 截断
//! [5]      flags    1 字节,预留(授权类型 / 席位)
//! [6..10]  sig      4 字节,HMAC(SIGN_SECRET, 前 6 字节) 截断
//! ```
//!
//! 长度定为 10 字节是刻意的:10 × 8 = 80 bits,Base32 恰好 16 个字符,没有填充位。
//! 早先用 12 字节时编码出 20 个字符(100 bits),末尾 4 bits 是填充 —— 意味着
//! 最后一个字符只有 1 bit 有效,用户抄错末位照样能激活。宁可短一点也要干净。

use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------- 密钥与常量 ----------

/// 签名密钥。**正式发版必须通过环境变量注入**,否则只会用到下面的开发占位值。
const SIGN_SECRET: &str = match option_env!("PASTENEXT_SIGN_SECRET") {
    Some(s) if !s.is_empty() => s,
    _ => "dev-sign-secret-never-ship",
};

/// 邮箱绑定密钥,同上。
const MAIL_SECRET: &str = match option_env!("PASTENEXT_MAIL_SECRET") {
    Some(s) if !s.is_empty() => s,
    _ => "dev-mail-secret-never-ship",
};

/// 购买页地址。同样支持编译期覆盖。
pub const PURCHASE_URL: &str = match option_env!("PASTENEXT_PURCHASE_URL") {
    Some(s) if !s.is_empty() => s,
    _ => "https://github.com/neekin/pastenext#buy",
};

const KEY_VERSION: u8 = 1;
/// 解码后的 payload 字节数:ver(1) + mail(4) + flags(1) + sig(4)
const PAYLOAD_LEN: usize = 10;
/// 参与签名的字节数:ver + mail + flags
const SIGNED_LEN: usize = 6;
/// Base32 后的字符数:10 字节 = 80 bits ÷ 5 = 16,无填充
const KEY_CHARS: usize = 16;
/// 去掉了易混淆的 I / L / O / U
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

// ---------- settings 键名 ----------

pub const K_ACTIVATED: &str = "license_activated";
pub const K_EMAIL: &str = "license_email";
pub const K_KEY: &str = "license_key";
pub const K_ACTIVATED_AT: &str = "license_activated_at";
pub const K_FIRST_LAUNCH: &str = "first_launch_at";
pub const K_LAST_PROMPT: &str = "license_last_prompt_at";

// ---------- HMAC-SHA256 ----------

/// 手写 HMAC-SHA256。项目已依赖 `sha2`,为了这两处调用不值得再拉一个 crate。
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new().chain_update(ipad).chain_update(msg).finalize();
    let out = Sha256::new().chain_update(opad).chain_update(inner).finalize();
    let mut res = [0u8; 32];
    res.copy_from_slice(&out);
    res
}

/// 常数时间比较,避免通过耗时侧信道逐字节爆破签名。
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        d |= x ^ y;
    }
    d == 0
}

// ---------- Base32(Crockford 变体) ----------

fn b32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let mut bits = 0u32;
    let mut nbits = 0;
    for &b in bytes {
        bits = (bits << 8) | b as u32;
        nbits += 8;
        while nbits >= 5 {
            nbits -= 5;
            out.push(ALPHABET[((bits >> nbits) & 0x1f) as usize] as char);
        }
    }
    if nbits > 0 {
        out.push(ALPHABET[((bits << (5 - nbits)) & 0x1f) as usize] as char);
    }
    out
}

fn b32_decode(s: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut nbits = 0;
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    for c in s.chars() {
        let idx = if c.is_ascii_digit() {
            (c as u8 - b'0') as u32
        } else if c.is_ascii_alphabetic() {
            // 容错:用户手抄时把 1/0 写成 I/L/O 也能通过
            let u = match c.to_ascii_uppercase() {
                'I' | 'L' => '1',
                'O' => '0',
                other => other,
            };
            match ALPHABET.iter().position(|&a| a == u as u8) {
                Some(i) => i as u32,
                None => return None,
            }
        } else {
            return None;
        };
        bits = (bits << 5) | idx;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push(((bits >> nbits) & 0xff) as u8);
        }
    }
    Some(out)
}

// ---------- 序列号生成 / 校验 ----------

/// 去掉分隔符与空白,统一大写。用户从邮件里复制时带上连字符也没关系。
pub fn normalize_key(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// 为一个邮箱生成序列号。生产环境请用 `scripts/gen-license.mjs`,
/// 这里保留同名实现是为了让算法有两份独立参照,便于交叉验证。
#[allow(dead_code)]
pub fn generate_key(email: &str) -> String {
    let mail = &hmac_sha256(MAIL_SECRET.as_bytes(), normalize_email(email).as_bytes())[..4];
    let mut payload = [0u8; SIGNED_LEN];
    payload[0] = KEY_VERSION;
    payload[1..5].copy_from_slice(mail);
    payload[5] = 0; // flags 预留
    let sig = &hmac_sha256(SIGN_SECRET.as_bytes(), &payload)[..4];
    let mut full = [0u8; PAYLOAD_LEN];
    full[..SIGNED_LEN].copy_from_slice(&payload);
    full[SIGNED_LEN..].copy_from_slice(sig);
    group(b32_encode(&full))
}

/// 每 4 个字符一段,用连字符分隔,方便肉眼核对
fn group(s: String) -> String {
    s.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join("-")
}

/// 校验「邮箱 + 序列号」是否匹配。
///
/// 返回 `Ok(())` 表示激活成功;`Err` 的文案直接展示给用户,所以必须写清楚原因。
pub fn verify_key(email: &str, raw_key: &str) -> Result<(), String> {
    let s = normalize_key(raw_key);
    if s.len() != KEY_CHARS {
        return Err(format!("序列号长度不对,应为 {} 位字符", KEY_CHARS));
    }
    let bytes = b32_decode(&s).ok_or("序列号含有无效字符")?;
    if bytes.len() != PAYLOAD_LEN {
        return Err("序列号格式不对".into());
    }
    if bytes[0] != KEY_VERSION {
        return Err("序列号版本不匹配".into());
    }
    let (payload, sig) = bytes.split_at(SIGNED_LEN);
    let expect = &hmac_sha256(SIGN_SECRET.as_bytes(), payload)[..4];
    if !ct_eq(sig, expect) {
        return Err("序列号无效".into());
    }
    let mail = &hmac_sha256(MAIL_SECRET.as_bytes(), normalize_email(email).as_bytes())[..4];
    if !ct_eq(&payload[1..5], mail) {
        return Err("序列号与该邮箱不匹配".into());
    }
    Ok(())
}

// ---------- 时间工具 ----------

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 已激活时把序列号打码,便于在设置页展示又不必担心被旁观者抄走。
pub fn mask_key(raw: &str) -> String {
    let s = normalize_key(raw);
    if s.len() != KEY_CHARS {
        return String::new();
    }
    // 首尾各留 4 位,中间 8 位打码
    format!("{}••••••••{}", &s[..4], &s[12..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_roundtrip() {
        let k = generate_key("Someone@Example.COM");
        assert_eq!(k.len(), 19); // 16 个字符 + 3 个连字符
        assert_eq!(normalize_key(&k).len(), KEY_CHARS);
        // 大小写与前后空格都不影响校验
        assert!(verify_key("someone@example.com", &k).is_ok());
        assert!(verify_key("someone@example.com", &k.to_lowercase()).is_ok());
        assert!(verify_key("someone@example.com", &format!("  {k}  ")).is_ok());
    }

    #[test]
    fn key_binds_to_email() {
        let k = generate_key("a@example.com");
        assert!(verify_key("a@example.com", &k).is_ok());
        assert!(verify_key("b@example.com", &k).is_err());
    }

    #[test]
    fn rejects_tampered_key() {
        let k = generate_key("a@example.com");
        // 最后一个字符位于签名区内,改它必须验不过
        let mut chars: Vec<char> = normalize_key(&k).chars().collect();
        let last = chars.pop().unwrap();
        chars.push(if last == '2' { '3' } else { '2' });
        assert!(verify_key("a@example.com", &chars.into_iter().collect::<String>()).is_err());
    }

    /// 回归测试:80 bits 必须正好编成 16 个字符,不留填充位。
    ///
    /// 早先用 12 字节(96 bits)时编码出 20 个字符,末尾 4 bits 是填充,
    /// 导致最后一个字符只有 1 bit 有效 —— 抄错末位照样能激活。
    #[test]
    fn every_character_is_significant() {
        let k = generate_key("pad@example.com");
        let s = normalize_key(&k);
        let mut accepted = 0;
        let mut rejected = 0;
        for i in 0..s.len() {
            for c in ALPHABET {
                let mut chars: Vec<char> = s.chars().collect();
                if chars[i] == *c as char {
                    continue;
                }
                chars[i] = *c as char;
                let variant: String = chars.into_iter().collect();
                if verify_key("pad@example.com", &variant).is_ok() {
                    accepted += 1;
                } else {
                    rejected += 1;
                }
            }
        }
        // 每个位置都只在「原字符」这一种情况下通过,其余 31 种全被拒
        assert_eq!(accepted, 0, "存在改了仍能通过的字符位置,说明有填充位");
        assert_eq!(rejected, KEY_CHARS * 31);
    }

    /// 与 `scripts/gen-license.mjs` 的交叉验证。
    /// 这个号由 Node 侧用开发占位密钥签发,Rust 侧必须能验过 —— 只要有一侧改了
    /// 算法(HMAC 截断长度、Base32 表、字段顺序)这条就会红。
    #[test]
    fn matches_node_generator() {
        // 正式发版时密钥来自环境变量,这条断言不再成立,直接跳过
        if option_env!("PASTENEXT_SIGN_SECRET").unwrap_or("").is_empty() {
            assert!(verify_key("Neekin@Example.com", "04SZ-2MQZ-0025-1XGA").is_ok());
            assert!(verify_key("other@example.com", "04SZ-2MQZ-0025-1XGA").is_err());
        }
    }

    #[test]
    fn mask_hides_middle() {
        let k = generate_key("a@example.com");
        let m = mask_key(&k);
        assert_eq!(m.chars().filter(|c| *c == '•').count(), 8);
        assert_eq!(m.chars().count(), KEY_CHARS); // 与原始序列号等长,便于对齐展示
        assert!(m.starts_with(&normalize_key(&k)[..4]));
    }
}
