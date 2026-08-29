fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon_with_id("assets/app-icon.ico", "1");
        resource
            .compile()
            .expect("failed to compile the Windows application icon");
    }
}
