/** Project corner movement onto the image diagonal to preserve its aspect ratio. */
export function draggedZoom(zoom: number, width: number, height: number, dx: number, dy: number) {
  const diagonal = width * width + height * height;
  if (!Number.isFinite(diagonal) || diagonal <= 0) return zoom;
  return zoom * Math.max(0.01, 1 + (width * dx + height * dy) / diagonal);
}
