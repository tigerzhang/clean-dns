fn main() {
    prost_build::compile_protos(&["src/proto/router.proto"], &["src/proto"]).unwrap();
}
