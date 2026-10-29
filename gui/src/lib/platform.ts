// With titleBarStyle: Overlay (macOS) the app has no native title bar; the
// sidebar extends to the window top and the traffic lights float over its top
// padding. The window drags from the sidebar's top strip, a thin strip along
// the window's top edge, and the backgrounds of top bars marked
// data-tauri-drag-region="deep" (their controls still take clicks). Other
// platforms keep their native title bars.
export const isMac = navigator.userAgent.includes("Mac");
