export type ClipKind = "text" | "rich_text" | "image" | "files";

export interface Tag {
  id: number;
  name: string;
  count?: number;
}

export interface Board {
  id: number;
  name: string;
  position: number;
}

export interface Clip {
  id: number;
  kind: ClipKind;
  text: string | null;
  html: string | null;
  imagePath: string | null;
  filePaths: string[] | null;
  sourceApp: string | null;
  /** 来源 App 图标缓存 key(对应后端 app_icons/<key>.png),无图标时为 null */
  sourceAppKey: string | null;
  /** 内容字节数:Text/RichText 为文本字节数,Image 为原始剪贴板字节数,Files 为文件真实大小之和 */
  byteSize: number;
  note: string;
  createdAt: number;
  lastUsedAt: number;
  useCount: number;
  boardId: number | null;
  tags: Tag[];
}
