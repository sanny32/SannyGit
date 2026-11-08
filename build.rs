fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets/app/sannygit.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/app/sannygit.ico")
            .set("ProductName", "SannyGit")
            .set("FileDescription", "SannyGit desktop Git client")
            .compile()?;
    }
    Ok(())
}
