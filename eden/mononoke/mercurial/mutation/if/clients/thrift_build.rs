// @generated by autocargo

use std::env;
use std::fs;
use std::path::Path;
use thrift_compiler::Config;
use thrift_compiler::GenContext;
const CRATEMAP: &str = "\
eden/mononoke/mercurial/mutation/if/hg_mutation_entry.thrift crate //eden/mononoke/mercurial/mutation/if:hg_mutation_entry_thrift-rust
eden/mononoke/mercurial/types/if/mercurial_thrift.thrift mercurial_thrift //eden/mononoke/mercurial/types/if:mercurial-thrift-rust
eden/mononoke/mononoke_types/serialization/blame.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/bonsai.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/bssm.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/ccsm.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/changeset_info.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/content.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/content_manifest.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/data.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/deleted_manifest.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/fastlog.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/fsnodes.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/id.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/path.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/raw_bundle2.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/redaction.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/sharded_map.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/skeleton_manifest.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/test_manifest.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/time.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
eden/mononoke/mononoke_types/serialization/unodes.thrift mercurial_thrift->mononoke_types_serialization //eden/mononoke/mononoke_types/serialization:mononoke_types_serialization-rust
thrift/annotation/rust.thrift rust //thrift/annotation:rust-rust
thrift/annotation/scope.thrift rust->scope //thrift/annotation:scope-rust
";
#[rustfmt::skip]
fn main() {
    println!("cargo:rerun-if-changed=thrift_build.rs");
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR env not provided");
    let cratemap_path = Path::new(&out_dir).join("cratemap");
    fs::write(cratemap_path, CRATEMAP).expect("Failed to write cratemap");
    Config::from_env(GenContext::Clients)
        .expect("Failed to instantiate thrift_compiler::Config")
        .base_path("../../../../../..")
        .types_crate("hg_mutation_entry_thrift__types")
        .clients_crate("hg_mutation_entry_thrift__clients")
        .run(["../hg_mutation_entry.thrift"])
        .expect("Failed while running thrift compilation");
}
