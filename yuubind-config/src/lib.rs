use std::mem;

use static_assertions::const_assert_eq;

const_assert_eq!(mem::size_of::<usize>(), mem::size_of::<u32>());

#[doc(hidden)]
pub use {capnp, yuubind_rpc};

#[macro_export]
macro_rules! implement {
    () => {
        use std::alloc::{self, Layout};

        use yuubind_config::{capnp, yuubind_rpc};

        use yuubind_rpc::types_capnp;

        // TODO: make the caller able to specify the callbacks
        #[no_mangle]
        pub unsafe extern "C" fn allocate(size: usize) -> usize {
            // Note: always allocate with alignment of 8 so that capnp's
            // read_message_from_flat_slice is happy
            // TODO: handle alloc error (ie. null return) properly
            unsafe { alloc::alloc(Layout::from_size_align_unchecked(size, 8)) as usize }
        }

        #[no_mangle]
        pub unsafe extern "C" fn deallocate(ptr: usize, size: usize) {
            unsafe { alloc::dealloc(ptr as *mut u8, Layout::from_size_align_unchecked(size, 8)) }
        }

        // TODO: actually call the callback instead of having this test impl
        fn client_config_must_do_tls_impl(
            arg: capnp::message::Reader<capnp::serialize::SliceSegments>,
        ) -> capnp::message::Builder<capnp::message::HeapAllocator> {
            let mut res = capnp::message::Builder::new_default();
            {
                let mut res = res.init_root::<types_capnp::bool::Builder>();
                res.set_false(());
            }
            res
        }

        // TODO: use the same boilerplate for all callbacks
        // TODO: handle errors properly (but what does “properly” exactly mean here?
        // anyway, probably not `.unwrap()` / `assert!`...)
        #[no_mangle]
        pub unsafe extern "C" fn client_config_must_do_tls(arg_ptr: usize, arg_size: usize) -> u64 {
            // Deserialize from the argument slice
            let mut arg_slice = std::slice::from_raw_parts(arg_ptr as *const u8, arg_size);
            let arg = capnp::serialize::read_message_from_flat_slice(
                &mut arg_slice,
                capnp::message::ReaderOptions::new(),
            )
            .unwrap();
            assert!(arg_slice.len() == 0);

            // Call the callback
            let res = client_config_must_do_tls_impl(arg);

            // Allocate return buffer
            // TODO: use constant once it's there https://github.com/capnproto/capnproto-rust/issues/217
            let ret_size: usize = 8 * capnp::serialize::compute_serialized_size_in_words(&res);
            let ret_ptr: usize = allocate(ret_size);
            let ret_slice = std::slice::from_raw_parts_mut(ret_ptr as *mut u8, ret_size);

            // Serialize the result to the return buffer
            capnp::serialize::write_message(ret_slice, &res).unwrap();

            // We know that usize is u32 thanks to the above const_assert
            ((ret_size as u64) << 32) | (ret_ptr as u64)
        }
    };
}
