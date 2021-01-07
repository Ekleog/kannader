use std::{path::Path, rc::Rc};

use anyhow::{anyhow, Context};

pub mod setup {
    kannader_config_types::implement_host!();
}

pub mod server_config {
    kannader_config_types::server_config_implement_host_client!();
}

pub struct WasmConfig {
    pub server_config: server_config::HostSide,
}

impl WasmConfig {
    /// Links and sets up a wasm blob for usage
    ///
    /// `cfg` is the path to the configuration of the wasm blob. `engine` and
    /// `module` are the pre-built wasm blob.
    pub fn new(
        cfg: &Path,
        engine: &wasmtime::Engine,
        module: &wasmtime::Module,
    ) -> anyhow::Result<WasmConfig> {
        let store = wasmtime::Store::new(engine);
        let instance = wasmtime::Instance::new(&store, module, &[])
            .context("Instantiating the wasm configuration blob")?;

        macro_rules! get_func {
            ($getter:ident, $function:expr) => {
                instance
                    .get_func($function)
                    .ok_or_else(|| anyhow!("Failed to find function export ‘{}’", $function))?
                    .$getter()
                    .with_context(|| format!("Checking the type of ‘{}’", $function))?
            };
        }

        // Parameter: size of the block to allocate
        // Return: address of the allocated block
        let allocate = Rc::new(get_func!(get1, "allocate"));

        // Parameters: (address, size) of the block to deallocate
        let deallocate = Rc::new(get_func!(get2, "deallocate"));

        let res = WasmConfig {
            server_config: server_config::build_host_side(&instance, allocate.clone(), deallocate)
                .context("Getting server configuration")?,
        };

        setup::setup(cfg, &instance, allocate).context("Running the setup hook")?;

        Ok(res)
    }
}
