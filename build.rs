//! Puts the game's icon in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/robigo.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/icons/robigo.ico")
        .compile()
        .expect("the icon compiles into the executable");
}
