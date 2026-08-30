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
  note: string;
  createdAt: number;
  lastUsedAt: number;
  useCount: number;
  boardId: number | null;
  tags: Tag[];
}
