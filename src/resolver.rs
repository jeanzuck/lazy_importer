use core::ffi::c_void;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::hash;
use crate::pe::ExportsDirectory;
use crate::peb::{self, LoadedModules};
use crate::types::UnicodeString;

const MAX_FORWARD_DEPTH: usize = 32;

#[derive(Debug)]
pub struct Cache {
    value: AtomicPtr<c_void>,
}

impl Cache {
    #[inline]
    #[doc(hidden)]
    pub const fn new() -> Self {
        Self {
            value: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    #[inline]
    fn load(&self) -> Option<NonNull<c_void>> {
        NonNull::new(self.value.load(Ordering::Acquire))
    }

    #[inline]
    fn store_if_empty(&self, ptr: NonNull<c_void>) {
        let _ = self.value.compare_exchange(
            core::ptr::null_mut(),
            ptr.as_ptr(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ModuleHandle(NonNull<c_void>);

impl ModuleHandle {
    #[inline]
    const fn from_non_null(ptr: NonNull<c_void>) -> Self {
        Self(ptr)
    }

    #[inline]
    pub fn from_ptr(ptr: *mut c_void) -> Option<Self> {
        NonNull::new(ptr).map(Self)
    }

    #[inline]
    pub(crate) const fn as_non_null(self) -> NonNull<c_void> {
        self.0
    }

    #[inline]
    pub const fn as_ptr(self) -> *mut c_void {
        self.0.as_ptr()
    }
}

unsafe impl Send for ModuleHandle {}
unsafe impl Sync for ModuleHandle {}

#[derive(Clone, Copy)]
pub struct LazyModule<const OHP: u64> {
    cache: Option<&'static Cache>,
}

impl<const OHP: u64> LazyModule<OHP> {
    #[inline]
    #[doc(hidden)]
    pub const fn with_cache(cache: &'static Cache) -> Self {
        Self { cache: Some(cache) }
    }

    #[inline]
    pub fn get(self) -> Option<ModuleHandle> {
        if let Some(cache) = self.cache
            && let Some(ptr) = cache.load()
        {
            return Some(ModuleHandle::from_non_null(ptr));
        }

        let resolved = unsafe { resolve_module_by_hash(OHP) };
        if let (Some(cache), Some(module)) = (self.cache, resolved) {
            cache.store_if_empty(module.as_non_null());
        }

        resolved
    }
}

#[derive(Clone, Copy)]
pub struct LazyFunction<const OHP: u64> {
    cache: Option<&'static Cache>,
}

impl<const OHP: u64> LazyFunction<OHP> {
    #[inline]
    #[doc(hidden)]
    pub const fn with_cache(cache: &'static Cache) -> Self {
        Self { cache: Some(cache) }
    }

    #[inline]
    pub fn address(self) -> Option<NonNull<c_void>> {
        if let Some(cache) = self.cache
            && let Some(ptr) = cache.load()
        {
            return Some(ptr);
        }

        let resolved = unsafe { resolve_function_by_hash(OHP, true) };
        if let (Some(cache), Some(ptr)) = (self.cache, resolved) {
            cache.store_if_empty(ptr);
        }

        resolved
    }

    #[inline]
    pub fn raw_address(self) -> Option<NonNull<c_void>> {
        unsafe { resolve_function_by_hash(OHP, false) }
    }

    #[inline]
    pub fn address_in(self, module: ModuleHandle) -> Option<NonNull<c_void>> {
        unsafe { resolve_function_in_module(OHP, module, true) }
    }

    #[inline]
    pub fn raw_address_in(self, module: ModuleHandle) -> Option<NonNull<c_void>> {
        unsafe { resolve_function_in_module(OHP, module, false) }
    }

    /// Casts the resolved export address to `F`.
    ///
    /// # Safety
    ///
    /// `F` must be a function pointer type with the exact ABI and signature of the export.
    #[inline]
    pub unsafe fn get<F: Copy>(self) -> Option<F> {
        self.address()
            .map(|address| unsafe { assume_function_type(address) })
    }

    /// Casts an export resolved inside `module` to `F`.
    ///
    /// # Safety
    ///
    /// `F` must be a function pointer type with the exact ABI and signature of the export.
    #[inline]
    pub unsafe fn get_in<F: Copy>(self, module: ModuleHandle) -> Option<F> {
        self.address_in(module)
            .map(|address| unsafe { assume_function_type(address) })
    }
}

unsafe fn assume_function_type<F: Copy>(address: NonNull<c_void>) -> F {
    debug_assert_eq!(
        core::mem::size_of::<F>(),
        core::mem::size_of::<*mut c_void>()
    );

    unsafe { core::mem::transmute_copy(&address) }
}

unsafe fn resolve_module_by_hash(pair: u64) -> Option<ModuleHandle> {
    let offset = hash::get_offset(pair);
    let target_hash = hash::get_hash(pair);
    let modules = unsafe { LoadedModules::new()? };

    for entry in modules {
        let base_name = unsafe { (*entry).base_dll_name };
        let name_hash = unsafe { peb::hash_unicode_string(base_name, offset)? };

        if name_hash == target_hash {
            return ModuleHandle::from_ptr(unsafe { (*entry).dll_base });
        }
    }

    None
}

unsafe fn resolve_function_by_hash(pair: u64, resolve_forwarded: bool) -> Option<NonNull<c_void>> {
    let offset = hash::get_offset(pair);
    let target_hash = hash::get_hash(pair);
    let modules = unsafe { LoadedModules::new()? };

    for entry in modules {
        let base_name = unsafe { (*entry).base_dll_name };
        let module = ModuleHandle::from_ptr(unsafe { (*entry).dll_base })?;
        let exports = unsafe { ExportsDirectory::new(module.as_non_null()) };
        let Some(exports) = exports else {
            continue;
        };

        let address = unsafe { exports.find_by_hash(target_hash, offset) };
        let Some(address) = address else {
            continue;
        };

        if resolve_forwarded && exports.is_forwarded(address) {
            return unsafe {
                resolve_forwarded_export(address.cast::<u8>().as_ptr(), offset, Some(base_name))
            };
        }

        return Some(address);
    }

    None
}

unsafe fn resolve_function_in_module(
    pair: u64,
    module: ModuleHandle,
    resolve_forwarded: bool,
) -> Option<NonNull<c_void>> {
    let offset = hash::get_offset(pair);
    let target_hash = hash::get_hash(pair);
    let exports = unsafe { ExportsDirectory::new(module.as_non_null())? };
    let address = unsafe { exports.find_by_hash(target_hash, offset)? };

    if resolve_forwarded && exports.is_forwarded(address) {
        let parent_name = unsafe { peb::base_name_for_module(module.as_ptr()) };
        return unsafe {
            resolve_forwarded_export(address.cast::<u8>().as_ptr(), offset, parent_name)
        };
    }

    Some(address)
}

unsafe fn resolve_forwarded_export(
    forwarder: *const u8,
    offset: u32,
    parent_name: Option<UnicodeString>,
) -> Option<NonNull<c_void>> {
    let mut hashes = unsafe { hash::hash_forwarded(forwarder, offset)? };
    let mut parent_name = parent_name;
    let mut depth = 0;

    while depth < MAX_FORWARD_DEPTH {
        depth += 1;

        match unsafe { resolve_forwarded_hashes_in_loaded_module(hashes, offset, false) } {
            Some(ForwardedResolution::Address(address)) => return Some(address),
            Some(ForwardedResolution::Forwarded(next)) => {
                hashes = next.hashes;
                parent_name = Some(next.parent_name);
                continue;
            }
            None => {}
        }

        let host_hash = unsafe {
            peb::api_set_host_hash_by_name(
                hashes.module_name,
                hashes.module_name_len,
                parent_name,
                offset,
            )
        }?;

        match unsafe {
            resolve_forwarded_hashes_in_loaded_module(
                hash::ForwardedHashes {
                    module_hash: host_hash,
                    module_name: core::ptr::null(),
                    module_name_len: 0,
                    function_hash: hashes.function_hash,
                },
                offset,
                true,
            )
        } {
            Some(ForwardedResolution::Address(address)) => return Some(address),
            Some(ForwardedResolution::Forwarded(next)) => {
                hashes = next.hashes;
                parent_name = Some(next.parent_name);
            }
            None => return None,
        }
    }

    None
}

enum ForwardedResolution {
    Address(NonNull<c_void>),
    Forwarded(ForwardedExport),
}

struct ForwardedExport {
    hashes: hash::ForwardedHashes,
    parent_name: UnicodeString,
}

unsafe fn resolve_forwarded_hashes_in_loaded_module(
    hashes: hash::ForwardedHashes,
    offset: u32,
    case_insensitive_module_hash: bool,
) -> Option<ForwardedResolution> {
    let modules = unsafe { LoadedModules::new()? };

    for entry in modules {
        let base_name = unsafe { (*entry).base_dll_name };
        let module_hash = if case_insensitive_module_hash {
            unsafe { peb::hash_unicode_string_without_dll_case_insensitive(base_name, offset)? }
        } else {
            unsafe { peb::hash_unicode_string_without_dll(base_name, offset)? }
        };

        if module_hash != hashes.module_hash {
            continue;
        }

        let module = ModuleHandle::from_ptr(unsafe { (*entry).dll_base })?;
        let exports = unsafe { ExportsDirectory::new(module.as_non_null())? };
        let address = unsafe { exports.find_by_hash(hashes.function_hash, offset)? };

        if exports.is_forwarded(address) {
            return Some(ForwardedResolution::Forwarded(ForwardedExport {
                hashes: unsafe { hash::hash_forwarded(address.cast::<u8>().as_ptr(), offset)? },
                parent_name: base_name,
            }));
        }

        return Some(ForwardedResolution::Address(address));
    }

    None
}
