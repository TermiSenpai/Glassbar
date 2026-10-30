fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/Glassbar.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set_manifest_file("assets/Glassbar.manifest");
        res.set("ProductName", "Glassbar");
        res.set("FileDescription", "Glassbar");
        res.compile().expect("resource compilation failed");
    }
}
