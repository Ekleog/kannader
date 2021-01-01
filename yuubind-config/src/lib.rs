use std::mem;

use static_assertions::const_assert_eq;

const_assert_eq!(mem::size_of::<usize>(), mem::size_of::<u32>());

#[macro_export]
macro_rules! implement {
    () => {
        use std::alloc::{self, Layout};

        // TODO: make the caller able to specify the callbacks
        #[no_mangle]
        unsafe extern "C" fn allocate(size: usize) -> usize {
            // Note: always allocate with alignment of 8 so that capnp's
            // read_message_from_flat_slice is happy
            unsafe { alloc::alloc(Layout::from_size_align_unchecked(size, 8)) as usize }
        }

        #[no_mangle]
        unsafe extern "C" fn deallocate(ptr: usize, size: usize) {
            unsafe { alloc::dealloc(ptr as *mut u8, Layout::from_size_align_unchecked(size, 8)) }
        }

        // TODO: use the same boilerplate for all callbacks
        #[no_mangle]
        unsafe extern "C" fn client_config_must_do_tls(ptr: usize, size: usize) -> u64 {
            // We know that usize is u32 thanks to the above const_assert
            todo!()
        }
    };
}
