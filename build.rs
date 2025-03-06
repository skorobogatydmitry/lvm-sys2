extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to tell rustc to link the system lvm2app
    // shared library.
    println!("cargo:rustc-link-lib=lvm2cmd");
    println!("cargo:rustc-link-lib=devmapper");

    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        .allowlist_function("dm_list_first")
        .allowlist_function("dm_list_next")
        .allowlist_function("dm_list_end")
        .allowlist_function("lvm.*")
        .allowlist_type("*list_t")
        .allowlist_type("dm_str_list")
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
