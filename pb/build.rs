use std::{env /*path::PathBuf*/};

// use anyhow::{bail, Result};

// fn main() -> Result<()> {
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // prepend_path("./bin")?; // so our `protoc` is found first

    env::set_var("PROTOC", "./protoc");

    tonic_build::compile_protos("proto/whiteboard.proto")?;
    Ok(())
}

// fn prepend_path(dir: &str) -> Result<()> {
//     const PATH: &str = "PATH";
//     let Some(path) = env::var_os(PATH) else { bail!("No {PATH} env?") };
//     let paths = [PathBuf::from(dir)].into_iter().chain(env::split_paths(&path));
//     env::set_var(PATH, env::join_paths(paths)?);
//     Ok(())
// }
