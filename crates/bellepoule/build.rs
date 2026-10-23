//! Icône ManageComp de l'exécutable Windows (`resources/branding/managecomp.ico`).
fn main() {
    let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/branding/managecomp.ico");
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon);
        if let Err(e) = res.compile() {
            println!("cargo:warning=icône de l'exécutable non intégrée : {e}");
        }
    }
}
