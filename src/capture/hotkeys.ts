const isWindows = /Win/.test(navigator.platform);
const modifier = isWindows ? "Ctrl+Alt" : "CmdOrCtrl+Shift";
export const CAPTURE_HOTKEY = `${modifier}+A`;
export const RECORD_HOTKEY = `${modifier}+R`;
export const SCROLL_HOTKEY = `${modifier}+G`;
