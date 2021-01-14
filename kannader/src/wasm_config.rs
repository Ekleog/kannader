use std::{cell::RefCell, path::Path, rc::Rc};

use anyhow::{anyhow, Context};
use wasmtime_wasi::WasiDir;

pub mod setup {
    kannader_config_macros::implement_host!();
}

pub mod client_config {
    kannader_config_macros::client_config_implement_host_client!(WasmFuncs);
}

pub mod queue_config {
    kannader_config_macros::queue_config_implement_host_client!(WasmFuncs);
}

pub mod server_config {
    kannader_config_macros::server_config_implement_host_client!(WasmFuncs);
}

pub struct WasmConfig {
    pub client_config: client_config::WasmFuncs,
    pub queue_config: queue_config::WasmFuncs,
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

        wasmtime_wasi::Wasi::new(
            &store,
            wasmtime_wasi::WasiCtx::builder()
                // TODO: this is bad! replace with something that only
                // adds the necessary stuff
                .preopened_dir(Box::new(AsyncDir::new(".")?), ".")
                .context("Adding async dir")?
                .build()
                .context("Preparing WASI context")?,
        )
        .add_to_linker(&mut linker)
        .context("Adding WASI exports to the linker")?;

        let tracing_serv = Rc::new(TracingServer);
        tracing_serv
            .add_to_linker(early_alloc.clone(), early_dealloc.clone(), &mut linker)
            .context("Adding ‘tracing’ module to the linker")?;

        linker
            .module("config", &module)
            .context("Instantiating the wasm configuration blob")?;

        macro_rules! get_func {
            ($getter:ident, $function:expr) => {
                linker
                    .get_one_by_name("config", Some($function))
                    .with_context(|| format!("Looking for an export for ‘{}’", $function))?
                    .into_func()
                    .ok_or_else(|| anyhow!("Export for ‘{}’ is not a function", $function))?
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
            client_config: client_config::WasmFuncs::build(
                &linker,
                allocate.clone(),
                deallocate.clone(),
            )
            .context("Getting client configuration")?,
            queue_config: queue_config::WasmFuncs::build(
                &linker,
                allocate.clone(),
                deallocate.clone(),
            )
            .context("Getting queue configuration")?,
            server_config: server_config::WasmFuncs::build(&linker, allocate.clone(), deallocate)
                .context("Getting server configuration")?,
        };

        setup::setup(cfg, &linker, allocate).context("Running the setup hook")?;

        Ok(res)
    }
}

struct AsyncDir(Box<dyn WasiDir>);

impl AsyncDir {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<AsyncDir> {
        unsafe {
            Ok(AsyncDir(Box::new(cap_std::fs::Dir::from_std_file(
                std::fs::File::open(path.as_ref())
                    .with_context(|| format!("Opening ‘{}’", path.as_ref().display()))?,
            ))))
        }
    }
}

// TODO: make all these impls defer to unblock
impl WasiDir for AsyncDir {
    fn as_any(&self) -> &dyn std::any::Any {
        tracing::warn!("Operating");
        self
    }

    fn open_file(
        &self,
        symlink_follow: bool,
        path: &str,
        oflags: wasmtime_wasi::OFlags,
        caps: wasmtime_wasi::FileCaps,
        fdflags: wasmtime_wasi::FdFlags,
    ) -> Result<Box<dyn wasmtime_wasi::WasiFile>, wasmtime_wasi::Error> {
        tracing::warn!("Opening file ‘{}’", path);
        // TODO: make this be an AsyncFile
        self.0
            .open_file(symlink_follow, path, oflags, caps, fdflags)
    }

    fn open_dir(
        &self,
        symlink_follow: bool,
        path: &str,
    ) -> Result<Box<dyn WasiDir>, wasmtime_wasi::Error> {
        tracing::warn!("Opening dir ‘{}’", path);
        self.0
            .open_dir(symlink_follow, path)
            .map(|d| Box::new(AsyncDir(d)) as _)
    }

    fn create_dir(&self, path: &str) -> Result<(), wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.create_dir(path)
    }

    fn readdir(
        &self,
        cursor: wasmtime_wasi::ReaddirCursor,
    ) -> Result<
        Box<
            dyn Iterator<
                Item = Result<(wasmtime_wasi::ReaddirEntity, String), wasmtime_wasi::Error>,
            >,
        >,
        wasmtime_wasi::Error,
    > {
        tracing::warn!("Operating");
        // TODO: make async
        self.0.readdir(cursor)
    }

    fn symlink(&self, old_path: &str, new_path: &str) -> Result<(), wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.symlink(old_path, new_path)
    }

    fn remove_dir(&self, path: &str) -> Result<(), wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.remove_dir(path)
    }

    fn unlink_file(&self, path: &str) -> Result<(), wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.unlink_file(path)
    }

    fn read_link(&self, path: &str) -> Result<std::path::PathBuf, wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.read_link(path)
    }

    fn get_filestat(&self) -> Result<wasmtime_wasi::Filestat, wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.get_filestat()
    }

    fn get_path_filestat(
        &self,
        path: &str,
    ) -> Result<wasmtime_wasi::Filestat, wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.get_path_filestat(path)
    }

    fn rename(
        &self,
        path: &str,
        dest_dir: &dyn WasiDir,
        dest_path: &str,
    ) -> Result<(), wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.rename(path, dest_dir, dest_path)
    }

    fn hard_link(
        &self,
        path: &str,
        symlink_follow: bool,
        target_dir: &dyn WasiDir,
        target_path: &str,
    ) -> Result<(), wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0
            .hard_link(path, symlink_follow, target_dir, target_path)
    }

    fn set_times(
        &self,
        path: &str,
        atime: Option<wasmtime_wasi::SystemTimeSpec>,
        mtime: Option<wasmtime_wasi::SystemTimeSpec>,
    ) -> Result<(), wasmtime_wasi::Error> {
        tracing::warn!("Operating");
        self.0.set_times(path, atime, mtime)
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
