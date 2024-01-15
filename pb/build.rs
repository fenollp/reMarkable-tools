use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Prepend ./bin to $PATH so our `protoc` is found first
    let mut new_path: String = "./bin:".to_owned();
    new_path.push_str(&env::var("PATH").unwrap());
    env::set_var("PATH", new_path);

    tonic_build::configure().build_server(false).compile(&["proto/whiteboard.proto"], &["."])?;
    Ok(())
}
