// @generated by autocargo

use std::env;
use std::fs;
use std::path::Path;
use thrift_compiler::Config;
use thrift_compiler::GenContext;
const CRATEMAP: &str = "\
configerator/structs/scm/hg/hgclientconf/hgclient.thrift crate //configerator/structs/scm/hg/hgclientconf:config-rust
thrift/annotation/cpp.thrift cpp //thrift/annotation:cpp-rust
thrift/annotation/rust.thrift rust //thrift/annotation:rust-rust
thrift/annotation/scope.thrift cpp->scope //thrift/annotation:scope-rust
thrift/annotation/thrift.thrift cpp->thrift //thrift/annotation:thrift-rust
";
#[rustfmt::skip]
fn main() {
    println!("cargo:rerun-if-changed=thrift_build.rs");
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR env not provided");
    let cratemap_path = Path::new(&out_dir).join("cratemap");
    fs::write(cratemap_path, CRATEMAP).expect("Failed to write cratemap");
    Config::from_env(GenContext::Types)
        .expect("Failed to instantiate thrift_compiler::Config")
        .base_path("../../../../..")
        .types_crate("config__types")
        .clients_crate("config__clients")
        .options("serde")
        .run(["hgclient.thrift"])
        .expect("Failed while running thrift compilation");
}
