// With titleBarStyle: Overlay (macOS) the app has no native title bar; a slim
// strip at the top of the window (rendered in App.tsx, marked
// data-tauri-drag-region) hosts the traffic lights and is the only window
// drag surface. Other platforms keep their native title bars.
export const isMac = navigator.userAgent.includes("Mac");
