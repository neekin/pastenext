//! 敏感内容检测:纯本地启发式,无网络、无第三方依赖。
//!
//! 设计原则:只抓「高置信」模式。检测结果仅影响预览打码(默认)或跳过入库,
//! 误杀的代价是用户多点一次"显示",漏报的代价是凭据躺在明文历史里 —— 因此
//! 规则偏保守地覆盖最常见的凭据形态:云厂商/API 令牌前缀、JWT、PEM 私钥、
//! 连续长随机串。密码管理器等敏感应用则靠「排除应用」规则(见 monitor.rs)。

/// 判断文本是否疑似敏感凭据。文本/富文本直接判;图片的 OCR 识别文本同用此函数。
pub fn is_sensitive(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 8 {
        return false;
    }
    token_prefix(t) || jwt(t) || private_key(t) || long_random(t)
}

/// 一键启用的密码管理器排除包:这些应用中复制的内容完全不记录。
/// 匹配走 excluded_apps 的「应用名/Bundle ID 包含、忽略大小写」逻辑。
pub const PASSWORD_MANAGER_PACK: &[&str] = &[
    "1Password",
    "Bitwarden",
    "KeePassXC",
    "KeePass",
    "钥匙串访问",
    "Keychain Access",
    "Passwords",
    "Dashlane",
    "Enpass",
    "Proton Pass",
    "LastPass",
    "Keeper Password Manager",
];

fn token_prefix(t: &str) -> bool {
    const PREFIXES: &[&str] = &[
        // GitHub
        "ghp_", "gho_", "ghu_", "ghs_", "github_pat_",
        // OpenAI 兼容 / Stripe
        "sk-", "rk_live_", "sk_live_",
        // AWS
        "AKIA", "ASIA",
        // Slack
        "xoxb-", "xoxp-", "xoxa-", "xoxs-",
        // GitLab / npm / Google
        "glpat-", "npm_", "AIza", "ya29.",
    ];
    PREFIXES.iter().any(|p| t.contains(p))
}

/// JWT 三段式特征:首段以 eyJ(Base64url 的 {")开头,且至少两个分隔点
fn jwt(t: &str) -> bool {
    t.contains("eyJ") && t.matches('.').count() >= 2
}

/// PEM 私钥
fn private_key(t: &str) -> bool {
    t.contains("-----BEGIN") && t.contains("PRIVATE KEY-----")
}

/// 无空白、≥32 字符、同时含大小写与数字 → 高概率是密钥/哈希/恢复码。
/// 有空白或过短直接放行,避免误伤普通句子与普通 ID。
fn long_random(t: &str) -> bool {
    let s = t.trim();
    if s.len() < 32 || s.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let (mut up, mut low, mut dig) = (false, false, false);
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            up = true;
        } else if c.is_ascii_lowercase() {
            low = true;
        } else if c.is_ascii_digit() {
            dig = true;
        }
    }
    up && low && dig
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_tokens() {
        assert!(is_sensitive("my key is ghp_16Characters7Cp9pT"));
        assert!(is_sensitive("sk-proj-abcdef1234567890"));
        assert!(is_sensitive("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn detects_jwt_and_keys() {
        assert!(is_sensitive("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.SflKxwRJSMeK"));
        assert!(is_sensitive("-----BEGIN RSA PRIVATE KEY-----\nMIIEow...\n-----END RSA PRIVATE KEY-----"));
    }

    #[test]
    fn allows_normal_text() {
        assert!(!is_sensitive("hello world, this is a normal sentence about cats"));
        assert!(!is_sensitive("订单号 20260905"));
        assert!(!is_sensitive("https://example.com/some/path"));
    }
}
