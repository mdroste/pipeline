// With titleBarStyle: Overlay (macOS) the app has no native title bar; the
// sidebar extends to the window top, the traffic lights float over its top
// padding, and an invisible strip there (data-tauri-drag-region in App.tsx)
// is the window drag handle. Other platforms keep their native title bars.
export const isMac = navigator.userAgent.includes("Mac");
