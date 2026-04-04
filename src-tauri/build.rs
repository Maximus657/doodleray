fn main() {
    // Embed Windows manifest requiring admin (UAC prompt on launch)
    // VPN mode uses TUN which needs elevated privileges
    #[cfg(windows)]
    {
        let mut res = tauri_build::WindowsAttributes::new();
        res = res.app_manifest(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#);
        tauri_build::try_build(
            tauri_build::Attributes::new().windows_attributes(res)
        ).expect("failed to run tauri_build");
    }

    #[cfg(not(windows))]
    {
        tauri_build::build()
    }
}
