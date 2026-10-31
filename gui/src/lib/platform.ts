// With titleBarStyle: Overlay (macOS) the app has no native title bar; the
// sidebar extends to the window top and the traffic lights float over its top
// padding. The window drags from the sidebar's top strip, a thin strip along
// the window's top edge, and the backgrounds of top bars marked
// data-tauri-drag-region="deep" (their controls still take clicks).
//
// Windows builds are undecorated (tauri.windows.conf.json), so the sidebar
// extends to the window top there too. WindowControls draws the minimize,
// maximize, and close buttons in the top-right corner, and the window drags
// from the sidebar's top strip and a strip above the main content. The window's
// very top edge stays a native resize handle.
//
// Other platforms keep their native title bars.
export const isMac = navigator.userAgent.includes("Mac");
export const isWindows = navigator.userAgent.includes("Windows");
