//! Embeds the icon in the Windows .exe. If that fails (no resource compiler), the program
//! still builds; only the file icon in Explorer is missing. The window and tray icons are set
//! by the program itself.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.set("ProductName", "jAyDHD-Grammar-Checker");
        resource.set("FileDescription", "jAyDHD-Grammar-Checker");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=could not embed the icon in the .exe: {error}");
        }
    }
}
