use core::ffi::c_void;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use crate::hash;
use crate::pe::ExportsDirectory;
use crate::peb::{self, LoadedModules};
use crate::types::UnicodeString;

const MAX_FORWARD_DEPTH: usize = 32;
const GLOBAL_CACHE_SLOTS: usize = 256;

static MODULE_CACHE: GlobalCache = GlobalCache::new();
static FUNCTION_CACHE: GlobalCache = GlobalCache::new();

struct GlobalCache {
    entries: [GlobalCacheEntry; GLOBAL_CACHE_SLOTS],
}

impl GlobalCache {
    #[inline]
    pub const fn new() -> Self {
        Self {
            entries: [const { GlobalCacheEntry::new() }; GLOBAL_CACHE_SLOTS],
        }
    }

    #[inline]
    fn load(&self, key: usize) -> Option<NonNull<c_void>> {
        if key == 0 {
            return None;
        }

        let mut probe = 0;
        while probe < GLOBAL_CACHE_SLOTS {
            let entry = &self.entries[index_for_key(key, probe)];
            let entry_key = entry.key.load(Ordering::Acquire);

            if entry_key == key {
                return entry.load_value();
            }
            if entry_key == 0 {
                return None;
            }

            probe += 1;
        }

        None
    }

    #[inline]
    fn store_if_empty(&self, key: usize, ptr: NonNull<c_void>) {
        if key == 0 {
            return;
        }

        let mut probe = 0;
        while probe < GLOBAL_CACHE_SLOTS {
            let entry = &self.entries[index_for_key(key, probe)];
            let entry_key = entry.key.load(Ordering::Acquire);

            if entry_key == key {
                entry.store_value_if_empty(ptr);
                return;
            }

            if entry_key == 0 {
                match entry
                    .key
                    .compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire)
                {
                    Ok(_) => {
                        entry.store_value_if_empty(ptr);
                        return;
                    }
                    Err(actual) if actual == key => {
                        entry.store_value_if_empty(ptr);
                        return;
                    }
                    Err(_) => {}
                }
            }

            probe += 1;
        }
    }
}

struct GlobalCacheEntry {
    key: AtomicUsize,
    value: AtomicPtr<c_void>,
}

impl GlobalCacheEntry {
    #[inline]
    const fn new() -> Self {
        Self {
            key: AtomicUsize::new(0),
            value: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    #[inline]
    fn load_value(&self) -> Option<NonNull<c_void>> {
        NonNull::new(self.value.load(Ordering::Acquire))
    }

    #[inline]
    fn store_value_if_empty(&self, ptr: NonNull<c_void>) {
        let _ = self.value.compare_exchange(
            core::ptr::null_mut(),
            ptr.as_ptr(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

#[inline]
fn index_for_key(key: usize, probe: usize) -> usize {
    key.wrapping_add(probe) % GLOBAL_CACHE_SLOTS
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
    cache_key: usize,
    cache_enabled: bool,
}

impl<const OHP: u64> LazyModule<OHP> {
    #[inline]
    #[doc(hidden)]
    pub const fn with_cache_key(cache_key: usize) -> Self {
        Self {
            cache_key,
            cache_enabled: false,
        }
    }

    #[inline]
    fn enabled_cache_key(self) -> Option<usize> {
        if self.cache_enabled {
            Some(self.cache_key)
        } else {
            None
        }
    }

    /// Caches the first successful module resolution in the global module cache.
    #[inline]
    #[must_use]
    pub const fn cached(self) -> Self {
        Self {
            cache_key: self.cache_key,
            cache_enabled: true,
        }
    }

    #[cfg(test)]
    pub(crate) const fn cache_key_for_tests(self) -> usize {
        self.cache_key
    }

    #[cfg(test)]
    pub(crate) const fn cache_enabled_for_tests(self) -> bool {
        self.cache_enabled
    }

    #[inline]
    pub fn get(self) -> Option<ModuleHandle> {
        let cache_key = self.enabled_cache_key();

        if let Some(key) = cache_key
            && let Some(ptr) = MODULE_CACHE.load(key)
        {
            return Some(ModuleHandle::from_non_null(ptr));
        }

        let resolved = unsafe { resolve_module_by_hash(OHP) };
        if let (Some(key), Some(module)) = (cache_key, resolved) {
            MODULE_CACHE.store_if_empty(key, module.as_non_null());
        }

        resolved
    }
}

#[derive(Clone, Copy)]
pub struct LazyFunction<const OHP: u64> {
    cache_key: usize,
    cache_enabled: bool,
}

impl<const OHP: u64> LazyFunction<OHP> {
    #[inline]
    #[doc(hidden)]
    pub const fn with_cache_key(cache_key: usize) -> Self {
        Self {
            cache_key,
            cache_enabled: false,
        }
    }

    #[inline]
    fn enabled_cache_key(self) -> Option<usize> {
        if self.cache_enabled {
            Some(self.cache_key)
        } else {
            None
        }
    }

    /// Caches the first successful global function resolution in the global function cache.
    #[inline]
    #[must_use]
    pub const fn cached(self) -> Self {
        Self {
            cache_key: self.cache_key,
            cache_enabled: true,
        }
    }

    #[cfg(test)]
    pub(crate) const fn cache_key_for_tests(self) -> usize {
        self.cache_key
    }

    #[cfg(test)]
    pub(crate) const fn cache_enabled_for_tests(self) -> bool {
        self.cache_enabled
    }

    #[inline]
    pub fn address(self) -> Option<NonNull<c_void>> {
        let cache_key = self.enabled_cache_key();

        if let Some(key) = cache_key
            && let Some(ptr) = FUNCTION_CACHE.load(key)
        {
            return Some(ptr);
        }

        let resolved = unsafe { resolve_function_by_hash(OHP, true) };
        if let (Some(key), Some(ptr)) = (cache_key, resolved) {
            FUNCTION_CACHE.store_if_empty(key, ptr);
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

#[cfg(test)]
mod tests {
    use super::*;

    const KERNEL32_HASH: u64 = hash::khash("KERNEL32.DLL", 0);
    const GET_CURRENT_PROCESS_ID_HASH: u64 = hash::khash("GetCurrentProcessId", 0);
    const MISSING_MODULE_HASH: u64 = hash::khash("__lazy_importer_missing_module__.dll", 0);
    const MISSING_FUNCTION_HASH: u64 = hash::khash("__lazy_importer_missing_export__", 0);
    const TEST_MODULE_CACHE_KEY: usize = hash::cache_key("__lazy_importer_test_module_cache__");
    const TEST_FUNCTION_CACHE_KEY: usize = hash::cache_key("__lazy_importer_test_function_cache__");
    const SHARED_FUNCTION_CACHE_KEY: usize =
        hash::cache_key("__lazy_importer_shared_function_cache__");
    const SHARED_MODULE_CACHE_KEY: usize = hash::cache_key("__lazy_importer_shared_module_cache__");

    fn fake_ptr() -> NonNull<c_void> {
        NonNull::new(1usize as *mut c_void).expect("fake pointer should be non-null")
    }

    #[test]
    fn global_cache_stores_values_by_key() {
        let cache = GlobalCache::new();

        cache.store_if_empty(TEST_FUNCTION_CACHE_KEY, fake_ptr());

        assert_eq!(
            cache
                .load(TEST_FUNCTION_CACHE_KEY)
                .expect("cached value should load")
                .as_ptr(),
            fake_ptr().as_ptr()
        );
        assert!(cache.load(TEST_MODULE_CACHE_KEY).is_none());
    }

    #[test]
    fn module_resolution_ignores_global_cache_until_opted_in() {
        MODULE_CACHE.store_if_empty(TEST_MODULE_CACHE_KEY, fake_ptr());

        let module = LazyModule::<MISSING_MODULE_HASH>::with_cache_key(TEST_MODULE_CACHE_KEY);

        assert!(module.get().is_none());
        assert_eq!(
            module
                .cached()
                .get()
                .expect("cached module should resolve from global cache")
                .as_ptr(),
            fake_ptr().as_ptr()
        );
    }

    #[test]
    fn function_resolution_ignores_global_cache_until_opted_in() {
        FUNCTION_CACHE.store_if_empty(TEST_FUNCTION_CACHE_KEY, fake_ptr());

        let function =
            LazyFunction::<MISSING_FUNCTION_HASH>::with_cache_key(TEST_FUNCTION_CACHE_KEY);

        assert!(function.address().is_none());
        assert_eq!(
            function
                .cached()
                .address()
                .expect("cached function should resolve from global cache")
                .as_ptr(),
            fake_ptr().as_ptr()
        );
    }

    #[test]
    fn function_cache_is_shared_globally_by_key() {
        let function =
            LazyFunction::<GET_CURRENT_PROCESS_ID_HASH>::with_cache_key(SHARED_FUNCTION_CACHE_KEY)
                .cached();
        let resolved = function
            .address()
            .expect("GetCurrentProcessId should resolve");
        let same_cache_key =
            LazyFunction::<MISSING_FUNCTION_HASH>::with_cache_key(SHARED_FUNCTION_CACHE_KEY);

        assert_eq!(
            same_cache_key
                .cached()
                .address()
                .expect("global cache should be shared by key")
                .as_ptr(),
            resolved.as_ptr()
        );
    }

    #[test]
    fn module_cache_is_shared_globally_by_key() {
        let module = LazyModule::<KERNEL32_HASH>::with_cache_key(SHARED_MODULE_CACHE_KEY).cached();
        let resolved = module.get().expect("kernel32 should resolve");
        let same_cache_key =
            LazyModule::<MISSING_MODULE_HASH>::with_cache_key(SHARED_MODULE_CACHE_KEY);

        assert_eq!(
            same_cache_key
                .cached()
                .get()
                .expect("global cache should be shared by key")
                .as_ptr(),
            resolved.as_ptr()
        );
    }
}
