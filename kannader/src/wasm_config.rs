use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, Context};
use wasmtime_wasi::{ambient_authority, Dir};

pub struct WasmState {
    wasi: wasmtime_wasi::WasiCtx,
    // Parameter: size of the block to allocate
    // Return: address of the allocated block
    alloc: Option<wasmtime::TypedFunc<u32, u32>>,
    // Parameters: (address, size) of the block to deallocate
    dealloc: Option<wasmtime::TypedFunc<(u32, u32), ()>>,
}

pub mod setup {
    use super::WasmState;
    kannader_config_macros::implement_host!();
}

pub mod client_config {
    use super::WasmState;
    kannader_config_macros::client_config_implement_host_client!(WasmFuncs);
}

pub mod queue_config {
    use super::WasmState;
    kannader_config_macros::queue_config_implement_host_client!(WasmFuncs);
}

pub mod server_config {
    use super::WasmState;
    kannader_config_macros::server_config_implement_host_client!(WasmFuncs);
}

pub struct WasmConfig {
    pub client_config: client_config::WasmFuncs,
    pub queue_config: queue_config::WasmFuncs,
    pub server_config: server_config::WasmFuncs,
    pub store: wasmtime::Store<WasmState>,
}

impl WasmConfig {
    /// Links and sets up a wasm blob for usage
    ///
    /// `cfg` is the path to the configuration of the wasm blob. `engine` and
    /// `module` are the pre-built wasm blob.
    pub fn new(
        dirs: &[(PathBuf, PathBuf)],
        cfg: &Path,
        engine: &wasmtime::Engine,
        module: &wasmtime::Module,
    ) -> anyhow::Result<WasmConfig> {
        let mut b = wasmtime_wasi::WasiCtxBuilder::new();
        for (guest, host) in dirs {
            // TODO: this is bad! replace with something that only
            // adds the necessary stuff
            // TODO: this should be async files, but let's keep
            // that for the day async wasi is implemented upstream
            b.preopened_dir(
                Dir::open_ambient_dir(&host, ambient_authority())
                    .with_context(|| format!("Preopening ‘{}’ for the guest", host.display()))?,
                guest,
            );
        }

        let store = wasmtime::Store::new(engine, WasmState {
            wasi: b.build(),
            alloc: None,
            dealloc: None,
        });
        let mut linker = wasmtime::Linker::new(&engine);

        wasmtime_wasi::add_to_linker(&mut linker, |state: &mut WasmState| &mut state.wasi)
            .context("Adding WASI exports to the linker")?;

        let tracing_serv = Arc::new(TracingServer);
        tracing_serv
            .add_to_linker(&mut store, &mut linker)
            .context("Adding ‘tracing’ module to the linker")?;

        linker
            .module(&mut store, "config", module)
            .context("Instantiating the wasm configuration blob")?;

        macro_rules! get_func {
            ($function:expr) => {
                linker
                    .get(&mut store, "config", $function)
                    .ok_or_else(|| anyhow!("No export for ‘{}’", $function))?
                    .into_func()
                    .ok_or_else(|| anyhow!("Export for ‘{}’ is not a function", $function))?
                    .typed(&mut store)
                    .with_context(|| format!("Checking the type of ‘{}’", $function))?
            };
        }

        store.data_mut().alloc = Some(get_func!("allocate"));
        store.data_mut().dealloc = Some(get_func!("deallocate"));

        let res = WasmConfig {
            client_config: client_config::WasmFuncs::build(&mut store, &linker)
                .context("Getting client configuration")?,
            queue_config: queue_config::WasmFuncs::build(&mut store, &linker)
                .context("Getting queue configuration")?,
            server_config: server_config::WasmFuncs::build(&mut store, &linker)
                .context("Getting server configuration")?,
            store,
        };

        setup::setup(cfg, &mut res.store, &linker).context("Running the setup hook")?;

        Ok(res)
    }
}

// TODO: have a proper tracing bridge, not some half-baked thing, once
// tracing supports this use case (tracing 0.2?
// https://github.com/tokio-rs/tracing/issues/1170#issuecomment-754304416)
struct TracingServer;

kannader_config_macros::tracing_implement_trait!();

impl TracingConfig for TracingServer {
    fn trace(self: Arc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        if meta.is_empty() {
            tracing::trace!("{}", msg);
        } else {
            tracing::trace!(?meta, "{}", msg);
        }
    }

    fn debug(self: Arc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        if meta.is_empty() {
            tracing::debug!("{}", msg);
        } else {
            tracing::debug!(?meta, "{}", msg);
        }
    }

    fn info(self: Arc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        if meta.is_empty() {
            tracing::info!("{}", msg);
        } else {
            tracing::info!(?meta, "{}", msg);
        }
    }

    fn warn(self: Arc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        if meta.is_empty() {
            tracing::warn!("{}", msg);
        } else {
            tracing::warn!(?meta, "{}", msg);
        }
    }

    fn error(self: Arc<Self>, meta: std::collections::HashMap<String, String>, msg: String) {
        if meta.is_empty() {
            tracing::error!("{}", msg);
        } else {
            tracing::error!(?meta, "{}", msg);
        }
    }
}

kannader_config_macros::tracing_implement_host_server!(TracingServer);
