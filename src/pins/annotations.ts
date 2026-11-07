export interface NumberMark { x: number; y: number; radius: number }
export interface ImageBounds { width: number; height: number }

/** Translate a click through object-fit:contain into original-image pixels. */
export function imagePoint(x: number, y: number, box: ImageBounds, image: ImageBounds) {
  if (box.width <= 0 || box.height <= 0 || image.width <= 0 || image.height <= 0) return null;
  const scale = Math.min(box.width / image.width, box.height / image.height);
  const left = (box.width - image.width * scale) / 2;
  const top = (box.height - image.height * scale) / 2;
  const px = (x - left) / scale;
  const py = (y - top) / scale;
  if (px < 0 || py < 0 || px > image.width || py > image.height) return null;
  const radius = Math.min(13 / scale, image.width / 2, image.height / 2);
  return {
    x: Math.max(radius, Math.min(image.width - radius, px)),
    y: Math.max(radius, Math.min(image.height - radius, py)),
    radius,
  };
}

/** Flatten at original resolution; display zoom and opacity do not degrade the copy. */
export function renderAnnotatedImage(image: HTMLImageElement, marks: NumberMark[]): string {
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Image canvas is unavailable");
  ctx.drawImage(image, 0, 0);
  marks.forEach((mark, index) => {
    paintNumberMark(ctx, mark, index);
  });
  return canvas.toDataURL("image/png");
}

export function paintNumberMark(ctx: CanvasRenderingContext2D, mark: NumberMark, index: number) {
    ctx.beginPath();
    ctx.arc(mark.x, mark.y, mark.radius, 0, Math.PI * 2);
    ctx.fillStyle = "#ef4444";
    ctx.fill();
    ctx.strokeStyle = "#ffffff";
    ctx.lineWidth = mark.radius * 0.12;
    ctx.stroke();
    ctx.fillStyle = "#ffffff";
    ctx.font = `700 ${mark.radius * (index >= 99 ? 0.9 : 1.15)}px -apple-system, BlinkMacSystemFont, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(String(index + 1), mark.x, mark.y + mark.radius * 0.06);
}
