use std::{env, path::PathBuf};

use anyhow::{bail, Result};

fn main() -> Result<()> {
    prepend_path("./bin")?; // so our `protoc` is found first
    tonic_build::compile_protos("proto/whiteboard.proto")?;
    Ok(())
}

fn prepend_path(dir: &str) -> Result<()> {
    const PATH: &str = "PATH";
    let Some(path) = env::var_os(PATH) else { bail!("No {PATH} env?") };
    let paths = env::split_paths(&path).chain([PathBuf::from(dir)]);
    env::set_var(PATH, env::join_paths(paths)?);
    Ok(())
}
