fn main() {
    // Match Tauri's default Windows manifest (the Common-Controls dependency
    // that native dialogs need) and additionally opt the exe into long-path
    // awareness — still gated on the OS LongPathsEnabled policy — so deep
    // per-step artifact directories under %USERPROFILE%\.pipeline\runs\…
    // cannot hit MAX_PATH. Ignored on other platforms.
    const WINDOWS_APP_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">
      <longPathAware>true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;
    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(WINDOWS_APP_MANIFEST),
        ),
    )
    .expect("failed to run tauri-build");
}
