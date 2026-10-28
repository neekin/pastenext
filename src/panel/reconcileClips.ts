import type { Clip } from "../types";

function sameClip(a: Clip, b: Clip): boolean {
  return (Object.keys(b) as (keyof Clip)[]).every((key) => {
    if (key === "tags") {
      return a.tags.length === b.tags.length && a.tags.every((tag, i) =>
        tag.id === b.tags[i].id && tag.name === b.tags[i].name && tag.count === b.tags[i].count
      );
    }
    if (key === "filePaths") {
      return a.filePaths === b.filePaths || (!!a.filePaths && !!b.filePaths &&
        a.filePaths.length === b.filePaths.length && a.filePaths.every((path, i) => path === b.filePaths![i]));
    }
    return a[key] === b[key];
  });
}

/** IPC 返回新对象时复用未变条目，避免每次唤醒重绘所有预览。 */
export function reconcileClips(previous: Clip[], incoming: Clip[]): Clip[] {
  const byId = new Map(previous.map((clip) => [clip.id, clip]));
  const next = incoming.map((clip) => {
    const old = byId.get(clip.id);
    return old && sameClip(old, clip) ? old : clip;
  });
  return next.length === previous.length && next.every((clip, i) => clip === previous[i]) ? previous : next;
}
