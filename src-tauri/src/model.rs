use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipKind {
    Text,
    RichText,
    Image,
    Files,
}

impl ClipKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClipKind::Text => "text",
            ClipKind::RichText => "rich_text",
            ClipKind::Image => "image",
            ClipKind::Files => "files",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(ClipKind::Text),
            "rich_text" => Some(ClipKind::RichText),
            "image" => Some(ClipKind::Image),
            "files" => Some(ClipKind::Files),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i64,
    pub kind: ClipKind,
    pub text: Option<String>,
    pub html: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub source_app: Option<String>,
    /// 来源 App 图标缓存 key(对应 <data_dir>/app_icons/<key>.png),无图标时为 None
    pub source_app_key: Option<String>,
    /// 内容字节数:Text/RichText 为字符字节数,Image 为原始剪贴板字节数,
    /// Files 为所有文件的真实大小之和(捕获时 stat,取不到的不计)。
    pub byte_size: i64,
    pub note: String,
    pub created_at: i64,
    pub last_used_at: i64,
    pub use_count: i64,
    pub board_id: Option<i64>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub bundle: Option<String>,
    /// 可执行文件绝对路径:macOS 为 .app 包路径,Windows 为 exe 路径。
    /// 用于提取来源 App 图标(历史回填时也可凭它离线取图标)。
    pub exe_path: Option<String>,
}

/// 平台层读到的原始剪贴板内容(image 字段可能是 PNG 或 TIFF 字节,
/// 统一由 monitor 解码后归一化为 PNG 存储)
#[derive(Debug, Clone)]
pub enum RawContent {
    Text { text: String, html: Option<String> },
    Image { bytes: Vec<u8>, format: String },
    Files { paths: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct ClipInsert {
    pub kind: ClipKind,
    pub text: Option<String>,
    pub html: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub byte_size: i64,
    pub hash: String,
    pub source_app: Option<String>,
    pub source_app_key: Option<String>,
}
