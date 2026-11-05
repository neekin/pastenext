import type { Mark } from "./drawing";
export interface EditHistory {
  marks: Mark[];
  redo: Mark[];
}
export type EditAction =
  | { type: "push"; mark: Mark }
  | { type: "undo" | "redo" }
  | { type: "restore"; history: EditHistory };
export function editHistory(
  state: EditHistory,
  action: EditAction,
): EditHistory {
  if (action.type === "restore") return action.history;
  if (action.type === "push")
    return { marks: [...state.marks, action.mark], redo: [] };
  if (action.type === "undo") {
    const last = state.marks[state.marks.length - 1];
    return last
      ? { marks: state.marks.slice(0, -1), redo: [...state.redo, last] }
      : state;
  }
  const last = state.redo[state.redo.length - 1];
  return last
    ? { marks: [...state.marks, last], redo: state.redo.slice(0, -1) }
    : state;
}
// Keep drafts across library navigation without retaining full-resolution bitmaps.
export const drafts = new Map<string, EditHistory>();
