fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg=/EXPORT:RTSSHooksCompatibility");
    }
}
