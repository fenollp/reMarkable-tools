use std::env;

fn main() {
    // Prepend ./bin to $PATH so our `protoc` is found first
    let mut new_path: String = "./bin:".to_owned();
    new_path.push_str(&env::var("PATH").unwrap());
    env::set_var("PATH", new_path);

    let protos = vec!["proto/hypercard/whiteboard.proto"];

    for proto in protos {
        tonic_build::compile_protos(proto).unwrap();
    }
}
