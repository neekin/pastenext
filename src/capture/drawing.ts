import { paintNumberMark } from "../pins/annotations.ts";
export type Tool =
  | "crop"
  | "rectangle"
  | "ellipse"
  | "arrow"
  | "line"
  | "pen"
  | "highlight"
  | "text"
  | "number"
  | "mosaic"
  | "blur"
  | "sticker";
export interface Point {
  x: number;
  y: number;
}
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface Mark {
  tool: Tool;
  from: Point;
  to: Point;
  color: string;
  width: number;
  points?: Point[];
  text?: string;
  pinRadius?: number;
}
export function rectBetween(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  };
}
export function clampRect(r: Rect, width: number, height: number): Rect {
  const x = Math.max(0, Math.min(width - 1, Math.floor(r.x))),
    y = Math.max(0, Math.min(height - 1, Math.floor(r.y)));
  return {
    x,
    y,
    width: Math.max(1, Math.min(width - x, Math.ceil(r.x + r.width) - x)),
    height: Math.max(1, Math.min(height - y, Math.ceil(r.y + r.height) - y)),
  };
}
export function cropFor(marks: Mark[], width: number, height: number): Rect {
  const crop = [...marks].reverse().find((m) => m.tool === "crop");
  return crop
    ? clampRect(rectBetween(crop.from, crop.to), width, height)
    : { x: 0, y: 0, width, height };
}
// Three box passes approximate a Gaussian blur; works in WKWebView even when
// CanvasRenderingContext2D.filter is unavailable. Pixel alpha is retained.
export function blurPixels(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  radius: number,
) {
  const temp = new Uint8ClampedArray(data.length);
  for (let pass = 0; pass < 3; pass++) {
    for (let y = 0; y < height; y++)
      for (let c = 0; c < 4; c++) {
        let sum = 0;
        for (let k = -radius; k <= radius; k++)
          sum +=
            data[(y * width + Math.max(0, Math.min(width - 1, k))) * 4 + c];
        for (let x = 0; x < width; x++) {
          temp[(y * width + x) * 4 + c] = sum / (2 * radius + 1);
          sum +=
            data[(y * width + Math.min(width - 1, x + radius + 1)) * 4 + c] -
            data[(y * width + Math.max(0, x - radius)) * 4 + c];
        }
      }
    for (let x = 0; x < width; x++)
      for (let c = 0; c < 4; c++) {
        let sum = 0;
        for (let k = -radius; k <= radius; k++)
          sum +=
            temp[(Math.max(0, Math.min(height - 1, k)) * width + x) * 4 + c];
        for (let y = 0; y < height; y++) {
          data[(y * width + x) * 4 + c] = sum / (2 * radius + 1);
          sum +=
            temp[(Math.min(height - 1, y + radius + 1) * width + x) * 4 + c] -
            temp[(Math.max(0, y - radius) * width + x) * 4 + c];
        }
      }
  }
}
export function paint(
  ctx: CanvasRenderingContext2D,
  mark: Mark,
  draft = false,
) {
  const { tool, from: a, to: b, color, width: w } = mark;
  if (tool === "crop") return;
  ctx.save();
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = w;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const r = clampRect(rectBetween(a, b), ctx.canvas.width, ctx.canvas.height);
  if (tool === "mosaic" || tool === "blur") {
    if (draft) {
      ctx.fillStyle = "#818cf844";
      ctx.fillRect(r.x, r.y, r.width, r.height);
      ctx.strokeRect(r.x, r.y, r.width, r.height);
    } else if (tool === "blur") {
      const pixels = ctx.getImageData(r.x, r.y, r.width, r.height);
      blurPixels(pixels.data, r.width, r.height, Math.max(3, Math.round(w)));
      ctx.putImageData(pixels, r.x, r.y);
    } else {
      const small = document.createElement("canvas");
      const block = Math.max(8, w * 3);
      small.width = Math.max(1, Math.ceil(r.width / block));
      small.height = Math.max(1, Math.ceil(r.height / block));
      small
        .getContext("2d")!
        .drawImage(
          ctx.canvas,
          r.x,
          r.y,
          r.width,
          r.height,
          0,
          0,
          small.width,
          small.height,
        );
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(
        small,
        0,
        0,
        small.width,
        small.height,
        r.x,
        r.y,
        r.width,
        r.height,
      );
    }
  } else if (tool === "rectangle") ctx.strokeRect(r.x, r.y, r.width, r.height);
  else if (tool === "ellipse") {
    ctx.beginPath();
    ctx.ellipse(
      r.x + r.width / 2,
      r.y + r.height / 2,
      r.width / 2,
      r.height / 2,
      0,
      0,
      Math.PI * 2,
    );
    ctx.stroke();
  } else if (tool === "text" || tool === "sticker") {
    ctx.font = `${Math.max(18, w * 6)}px -apple-system, BlinkMacSystemFont, sans-serif`;
    ctx.textBaseline = "top";
    (mark.text || "")
      .split("\n")
      .forEach((line, i) =>
        ctx.fillText(line, a.x, a.y + i * Math.max(22, w * 7)),
      );
  } else if (tool === "number" && mark.pinRadius !== undefined) {
    paintNumberMark(ctx, { x: a.x, y: a.y, radius: mark.pinRadius }, Number(mark.text || "1") - 1);
  } else if (tool === "number") {
    const radius = Math.max(14, w * 4);
    ctx.beginPath();
    ctx.arc(a.x, a.y, radius, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "white";
    ctx.font = `bold ${radius}px sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(mark.text || "1", a.x, a.y);
  } else if (tool === "pen" || tool === "highlight") {
    if (tool === "highlight") {
      ctx.globalAlpha = 0.32;
      ctx.lineWidth = w * 6;
    }
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    for (const p of mark.points || [b]) ctx.lineTo(p.x, p.y);
    ctx.stroke();
  } else {
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    if (tool === "arrow") {
      const angle = Math.atan2(b.y - a.y, b.x - a.x),
        size = Math.max(14, w * 4);
      ctx.beginPath();
      ctx.moveTo(b.x, b.y);
      ctx.lineTo(
        b.x - size * Math.cos(angle - 0.45),
        b.y - size * Math.sin(angle - 0.45),
      );
      ctx.lineTo(
        b.x - size * Math.cos(angle + 0.45),
        b.y - size * Math.sin(angle + 0.45),
      );
      ctx.closePath();
      ctx.fill();
    }
  }
  ctx.restore();
}
export function renderImage(
  image: HTMLImageElement,
  marks: Mark[],
  crop = true,
): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(image, 0, 0);
  marks.forEach((m) => paint(ctx, m));
  if (!crop) return canvas;
  const r = cropFor(marks, canvas.width, canvas.height);
  const output = document.createElement("canvas");
  output.width = r.width;
  output.height = r.height;
  output
    .getContext("2d")!
    .drawImage(canvas, r.x, r.y, r.width, r.height, 0, 0, r.width, r.height);
  return output;
}
