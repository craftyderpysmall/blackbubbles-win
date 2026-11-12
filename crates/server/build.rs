//! On macOS, embed an Info.plist in the `bbd` binary. Command-line tools can only ask
//! for Photos/Calendar access if their usage descriptions are present this way.

fn main() {
    println!("cargo:rerun-if-changed=macos/Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("macos/Info.plist");
        println!("cargo:rustc-link-arg-bin=bbd=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
}
