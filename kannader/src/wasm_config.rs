use std::{cell::RefCell, path::Path, rc::Rc};

use anyhow::{anyhow, Context};

pub mod setup {
    kannader_config_macros::implement_host!();
}

pub mod server_config {
    kannader_config_macros::server_config_implement_host_client!(WasmFuncs);
}

pub struct WasmConfig {
    pub server_config: server_config::WasmFuncs,
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
        // Variables used to refer to allocator / deallocator while
        // they aren't ready yet. The RefCell's will be filled once a
        // bit below in this function, and then never changed again.
        let early_alloc = Rc::new(RefCell::new(None));
        let early_dealloc = Rc::new(RefCell::new(None));

        let store = wasmtime::Store::new(engine);
        let mut linker = wasmtime::Linker::new(&store);

        let tracing_serv = Rc::new(TracingServer);
        tracing_serv
            .add_to_linker(early_alloc.clone(), early_dealloc.clone(), &mut linker)
            .context("Adding ‘tracing’ module to the linker")?;

        let instance = linker
            .instantiate(&module)
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
        *early_alloc.borrow_mut() = Some(get_func!(get1, "allocate"));

        // Parameters: (address, size) of the block to deallocate
        let deallocate = Rc::new(get_func!(get2, "deallocate"));
        *early_dealloc.borrow_mut() = Some(get_func!(get2, "deallocate"));

        let res = WasmConfig {
            server_config: server_config::WasmFuncs::build(&instance, allocate.clone(), deallocate)
                .context("Getting server configuration")?,
        };

        setup::setup(cfg, &instance, allocate).context("Running the setup hook")?;

        Ok(res)
    }
}

// TODO: have a proper tracing bridge, not some half-baked thing, once
// tracing supports this use case (tracing 0.2?
// https://github.com/tokio-rs/tracing/issues/1170#issuecomment-754304416)
struct TracingServer;

kannader_config_macros::tracing_implement_trait!();

impl TracingConfig for TracingServer {
    fn trace(self: Rc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        tracing::trace!(?meta, "{}", msg);
    }

    fn debug(self: Rc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        tracing::debug!(?meta, "{}", msg);
    }

    fn info(self: Rc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        tracing::info!(?meta, "{}", msg);
    }

    fn warn(self: Rc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        tracing::warn!(?meta, "{}", msg);
    }

    fn error(self: Rc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        tracing::error!(?meta, "{}", msg);
    }
}

kannader_config_macros::tracing_implement_host_server!(TracingServer);
