#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![doc = include_str!("../README.md")]

#[cfg(all(not(windows), not(docsrs)))]
compile_error!("lazy_importer supports Windows targets only.");

#[cfg(test)]
extern crate std;

mod hash;
mod pe;
mod peb;
mod resolver;
mod types;

#[doc(hidden)]
pub mod __private {
    pub use const_random::const_random;

    pub use crate::hash::cache_key;
    pub use crate::hash::khash;
}

pub use resolver::{LazyFunction, LazyModule, ModuleHandle};

mod macros;

#[cfg(test)]
mod windows_tests {
    use core::ffi::{c_char, c_int, c_void};

    #[allow(non_snake_case)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn GetModuleHandleA(module_name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, proc_name: *const c_char) -> *mut c_void;
    }

    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxA(
            window: *mut c_void,
            text: *const c_char,
            caption: *const c_char,
            typ: u32,
        ) -> c_int;
    }

    #[test]
    fn resolves_loaded_module() {
        let module = crate::li_module!("KERNEL32.DLL").get();
        assert!(module.is_some());
    }

    #[test]
    fn module_macro_call_sites_share_global_cache_key() {
        let first = crate::li_module!("KERNEL32.DLL");
        let second = crate::li_module!("KERNEL32.DLL");

        assert!(!first.cache_enabled_for_tests());
        assert!(first.cached().cache_enabled_for_tests());
        assert_eq!(
            first.cache_key_for_tests(),
            second.cache_key_for_tests(),
            "same module name should map to one global cache slot"
        );
    }

    #[cfg(feature = "case-insensitive")]
    #[test]
    fn resolves_loaded_module_case_insensitively() {
        let module = crate::li_module!("kernel32.dll").get();
        assert!(module.is_some());
    }

    #[test]
    fn resolves_and_invokes_export() {
        type GetCurrentProcessIdFn = unsafe extern "system" fn() -> u32;

        let function = unsafe {
            crate::li_fn!("GetCurrentProcessId")
                .get::<GetCurrentProcessIdFn>()
                .expect("GetCurrentProcessId should resolve")
        };

        let direct = unsafe { GetCurrentProcessId() };
        let resolved = unsafe { function() };

        assert_eq!(resolved, direct);
    }

    #[test]
    fn function_macro_call_sites_share_global_cache_key() {
        let first = crate::li_fn!("GetCurrentProcessId");
        let second = crate::li_fn!("GetCurrentProcessId");

        assert!(!first.cache_enabled_for_tests());
        assert!(first.cached().cache_enabled_for_tests());
        assert_eq!(
            first.cache_key_for_tests(),
            second.cache_key_for_tests(),
            "same export name should map to one global cache slot"
        );
    }

    #[test]
    fn resolves_export_in_module() {
        type GetCurrentProcessIdFn = unsafe extern "system" fn() -> u32;

        let kernel32 = crate::li_module!("KERNEL32.DLL")
            .get()
            .expect("kernel32 should resolve");
        let function = unsafe {
            crate::li_fn!("GetCurrentProcessId")
                .get_in::<GetCurrentProcessIdFn>(kernel32)
                .expect("GetCurrentProcessId should resolve in kernel32")
        };

        assert_eq!(unsafe { function() }, unsafe { GetCurrentProcessId() });
    }

    #[test]
    fn resolves_message_box_a() {
        type MessageBoxAFn =
            unsafe extern "system" fn(*mut c_void, *const c_char, *const c_char, u32) -> c_int;

        core::hint::black_box(MessageBoxA as *const ());

        let lazy_message_box = crate::li_fn!("MessageBoxA");
        let address = lazy_message_box
            .address()
            .expect("MessageBoxA address should resolve");
        let function = unsafe {
            lazy_message_box
                .get::<MessageBoxAFn>()
                .expect("MessageBoxA should resolve")
        };

        assert_eq!(address.as_ptr(), function as *const () as *mut c_void);
    }

    #[test]
    fn resolves_api_set_forwarded_export() {
        let kernel32 = crate::li_module!("KERNEL32.DLL")
            .get()
            .expect("kernel32 should resolve");
        let export = crate::li_fn!("SetProcessMitigationPolicy");
        let raw = export
            .raw_address_in(kernel32)
            .expect("kernel32!SetProcessMitigationPolicy should exist");
        let exports = unsafe {
            crate::pe::ExportsDirectory::new(kernel32.as_non_null())
                .expect("kernel32 export directory should parse")
        };

        assert!(
            exports.is_forwarded(raw),
            "SetProcessMitigationPolicy should be a forwarded export"
        );

        let resolved_in_module = export
            .address_in(kernel32)
            .expect("API-set forwarded export should resolve to its host DLL");
        let resolved_global = export
            .address()
            .expect("API-set forwarded export should resolve globally");
        let kernel32_direct = unsafe { GetModuleHandleA(c"kernel32.dll".as_ptr()) };
        assert!(!kernel32_direct.is_null());

        let expected =
            unsafe { GetProcAddress(kernel32_direct, c"SetProcessMitigationPolicy".as_ptr()) };
        assert!(!expected.is_null());
        assert_eq!(resolved_in_module.as_ptr(), expected);
        assert_eq!(resolved_global.as_ptr(), expected);
    }
}
