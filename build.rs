use chrono::{Datelike, Utc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rustc-env=SANNYGIT_BUILD_YEAR={}", Utc::now().year());
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-env-changed=SANNYGIT_GITHUB_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=SANNYGIT_GITHUB_CLIENT_SECRET");
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
