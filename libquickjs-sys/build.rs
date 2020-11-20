use std::path::{Path, PathBuf};

use std::env;

fn exists(path: impl AsRef<Path>) -> bool {
    PathBuf::from(path.as_ref()).exists()
}

const LIB_NAME: &str = "quickjs";

#[cfg(all(not(feature = "system"), not(feature = "bundled")))]
fn main() {
    panic!("Invalid config for crate libquickjs-sys: must enable either the 'bundled' or the 'system' feature");
}

extern crate bindgen;

#[cfg(feature = "system")]
fn main() {
    #[cfg(not(feature = "bindgen"))]
    panic!("Invalid configuration for libquickjs-sys: Must either enable the bundled or the bindgen feature");

    #[cfg(feature = "patched")]
    panic!("Invalid configuration for libquickjs-sys: the patched feature is incompatible with the system feature");

    // compile statics
    cc::Build::new()
        .file("static-functions.c")
        .compile("libquickjs-static-functions.a");

    let lib: std::borrow::Cow<str> = if let Ok(lib) = env::var("QUICKJS_LIBRARY_PATH") {
        lib.into()
    } else if cfg!(unix) {
        if exists(format!("/usr/lib/quickjs/{}.a", LIB_NAME)) {
            "/usr/lib/quickjs".into()
        } else if exists("/usr/local/lib/quickjs") {
            "/usr/local/lib/quickjs".into()
        } else {
            panic!("quickjs library could not be found. Try setting the QUICKJS_LIBRARY_PATH env variable");
        }
    } else {
        panic!("quickjs error: Windows is not supported yet");
    };

    // Generate bindings.
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    // Instruct cargo to statically link quickjs.
    println!("cargo:rustc-link-search=native={}", lib);
    println!("cargo:rustc-link-lib=static={}", LIB_NAME);
}

#[cfg(not(target_env = "msvc"))]
#[derive(Debug)]
struct IgnoreMacros(std::collections::HashSet<String>);

#[cfg(not(target_env = "msvc"))]
impl bindgen::callbacks::ParseCallbacks for IgnoreMacros {
    fn will_parse_macro(&self, name: &str) -> bindgen::callbacks::MacroParsingBehavior {
        if self.0.contains(name) {
            bindgen::callbacks::MacroParsingBehavior::Ignore
        } else {
            bindgen::callbacks::MacroParsingBehavior::Default
        }
    }
}

#[cfg(not(target_env = "msvc"))]
#[cfg(feature = "bundled")]
fn main() {
    // compile statics
    cc::Build::new()
        .file("static-functions.c")
        .compile("libquickjs-static-functions.a");

    let embed_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("embed");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    let code_dir = out_path.join("quickjs");
    if exists(&code_dir) {
        std::fs::remove_dir_all(&code_dir).unwrap();
    }
    copy_dir::copy_dir(embed_path.join("quickjs"), &code_dir)
        .expect("Could not copy quickjs directory");

    #[cfg(feature = "patched")]
    apply_patches(&code_dir);

    eprintln!("Compiling quickjs...");
    cc::Build::new()
        .files(
            [
                "cutils.c",
                "libbf.c",
                "libregexp.c",
                "libunicode.c",
                "quickjs.c",
            ]
            .iter()
            .map(|f| code_dir.join(f)),
        )
        .define("_GNU_SOURCE", None)
        .define("CONFIG_BIGNUM", None)
        // The below flags are used by the official Makefile.
        .flag_if_supported("-Wchar-subscripts")
        .flag_if_supported("-Wno-array-bounds")
        .flag_if_supported("-Wno-format-truncation")
        .flag_if_supported("-Wno-missing-field-initializers")
        .flag_if_supported("-Wno-sign-compare")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wundef")
        .flag_if_supported("-Wuninitialized")
        .flag_if_supported("-Wunused")
        .flag_if_supported("-Wwrite-strings")
        .flag_if_supported("-funsigned-char")
        // Below flags are added to supress warnings that appear on some
        // platforms.
        .flag_if_supported("-Wno-cast-function-type")
        .flag_if_supported("-Wno-implicit-fallthrough")
        // cc uses the OPT_LEVEL env var by default, but we hardcode it to -O2
        // since release builds use -O3 which might be problematic for quickjs,
        // and debug builds only happen once anyway so the optimization slowdown
        // is fine.
        .opt_level(2)
        .compile(LIB_NAME);

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    let ignored_macros = IgnoreMacros(
        vec![
            "FP_INFINITE".into(),
            "FP_NAN".into(),
            "FP_NORMAL".into(),
            "FP_SUBNORMAL".into(),
            "FP_ZERO".into(),
        ]
        .into_iter()
        .collect(),
    );

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .parse_callbacks(Box::new(ignored_macros))
        .clang_arg("-I".to_owned() + out_path.to_str().unwrap())
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

#[cfg(target_env = "msvc")]
#[cfg(feature = "bundled")]
fn main() {
    // compile statics
    cc::Build::new()
        .file("static-functions.c")
        // JS_STRICT_NAN_BOXING required for MSVC build
        .define("JS_STRICT_NAN_BOXING", None)
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .flag_if_supported("/std:c++latest")
        .compile("quickjs-static-functions.lib");

    let embed_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("embed");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    let code_dir = out_path.join("quickjs");
    if exists(&code_dir) {
        std::fs::remove_dir_all(&code_dir).unwrap();
    }
    copy_dir::copy_dir(embed_path.join("quickjs"), &code_dir)
        .expect("Could not copy quickjs directory");

    // Patch command generally unavailable on Windows
    // #[cfg(feature = "patched")]
    // apply_patches(&code_dir);

    eprintln!("Compiling quickjs...");
    cc::Build::new()
        .files(
            [
                "cutils.c",
                "libbf.c",
                "libregexp.c",
                "libunicode.c",
                "quickjs.c",
            ]
            .iter()
            .map(|f| code_dir.join(f)),
        )
        // JS_STRICT_NAN_BOXING required for MSVC build
        .define("JS_STRICT_NAN_BOXING", None)
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .define("CONFIG_BIGNUM", None)
        .flag_if_supported("/std:c++latest")
        // c-smile/quickjspp does not build with opt_level(2)!
        .opt_level(1)
        .compile(LIB_NAME);

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .clang_arg("-I".to_owned() + out_path.to_str().unwrap())
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

#[cfg(feature = "patched")]
fn apply_patches(code_dir: &PathBuf) {
    use std::fs;

    eprintln!("Applying patches...");
    let embed_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("embed");
    let patches_path = embed_path.join("patches");
    for patch in fs::read_dir(patches_path).expect("Could not open patches directory") {
        let patch = patch.expect("Could not open patch");
        eprintln!("Applying {:?}...", patch.file_name());
        let status = std::process::Command::new("patch")
            .current_dir(&code_dir)
            .arg("-i")
            .arg(fs::canonicalize(patch.path()).expect("Cannot canonicalize patch path"))
            .spawn()
            .expect("Could not apply patches")
            .wait()
            .expect("Could not apply patches");
        assert!(
            status.success(),
            "Patch command returned non-zero exit code"
        );
    }
}
