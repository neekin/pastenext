import type { NumberMark } from "../pins/annotations.ts";
import type { Mark } from "./drawing.ts";

/** Keep legacy pin numbers as editable studio marks, in original-image pixels. */
export function marksFromPin(marks: NumberMark[]): Mark[] {
  return marks.map((mark, index) => ({
    tool: "number", from: { x: mark.x, y: mark.y }, to: { x: mark.x, y: mark.y },
    color: "#ef4444", width: mark.radius / 4, pinRadius: mark.radius, text: String(index + 1),
  }));
}
