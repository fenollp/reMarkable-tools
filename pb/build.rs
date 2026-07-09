fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Uses `protoc` from $PATH (or $PROTOC if set). Requires protobuf-compiler >= 3.6.1.
    tonic_build::compile_protos("proto/whiteboard.proto")?;
    Ok(())
}
