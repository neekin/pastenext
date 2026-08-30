export function applyTheme(theme: string | undefined) {
  const t = theme ?? "system";
  const dark =
    t === "dark" || (t === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}
