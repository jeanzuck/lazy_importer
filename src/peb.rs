use crate::hash;
use crate::types::{
    ApiSetNamespace, ApiSetNamespaceEntry, ApiSetValueEntry, LdrDataTableEntry, ListEntry, Peb,
    PebLdrData, UnicodeString,
};

const MAX_API_SET_ENTRIES: u32 = 8192;
const MAX_API_SET_VALUES: u32 = 64;
const MAX_MODULES: usize = 4096;

#[cfg(all(windows, target_arch = "x86_64"))]
#[inline]
pub(crate) unsafe fn peb() -> *const Peb {
    let peb: *const Peb;
    unsafe {
        core::arch::asm!(
            "mov {}, gs:[0x60]",
            out(reg) peb,
            options(nostack, preserves_flags, readonly)
        );
    }
    peb
}

#[cfg(all(windows, target_arch = "x86"))]
#[inline]
pub(crate) unsafe fn peb() -> *const Peb {
    let peb: *const Peb;
    unsafe {
        core::arch::asm!(
            "mov {}, fs:[0x30]",
            out(reg) peb,
            options(nostack, preserves_flags, readonly)
        );
    }
    peb
}

#[cfg(all(windows, target_arch = "aarch64"))]
#[inline]
pub(crate) unsafe fn peb() -> *const Peb {
    let teb: usize;
    unsafe {
        core::arch::asm!(
            "mov {}, x18",
            out(reg) teb,
            options(nostack, preserves_flags)
        );
    }

    unsafe { *((teb + 0x60) as *const *const Peb) }
}

#[cfg(not(windows))]
#[inline]
pub(crate) unsafe fn peb() -> *const Peb {
    core::ptr::null()
}

#[cfg(all(
    windows,
    not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"))
))]
compile_error!("lazy_importer currently supports Windows x86, x86_64, and aarch64 targets.");

#[inline]
unsafe fn ldr() -> Option<*const PebLdrData> {
    let peb = unsafe { peb() };
    if peb.is_null() {
        return None;
    }

    let ldr = unsafe { (*peb).ldr };
    if ldr.is_null() { None } else { Some(ldr) }
}

#[inline]
unsafe fn api_set_map() -> Option<*const ApiSetNamespace> {
    let peb = unsafe { peb() };
    if peb.is_null() {
        return None;
    }

    let api_set_map = unsafe { (*peb).api_set_map };
    if api_set_map.is_null() {
        None
    } else {
        Some(api_set_map)
    }
}

pub(crate) struct LoadedModules {
    head: *const ListEntry,
    current: *const ListEntry,
    remaining: usize,
}

impl LoadedModules {
    #[inline]
    pub(crate) unsafe fn new() -> Option<Self> {
        let ldr = unsafe { ldr()? };
        let head = unsafe { core::ptr::addr_of!((*ldr).in_load_order_module_list) };
        let current = unsafe { (*head).flink };

        if current.is_null() {
            return None;
        }

        Some(Self {
            head,
            current,
            remaining: MAX_MODULES,
        })
    }
}

impl Iterator for LoadedModules {
    type Item = *const LdrDataTableEntry;

    fn next(&mut self) -> Option<Self::Item> {
        while self.remaining > 0 {
            self.remaining -= 1;

            if self.current.is_null() || self.current == self.head {
                return None;
            }

            let entry = self.current.cast::<LdrDataTableEntry>();
            self.current = unsafe { (*self.current).flink };

            if !entry.is_null() && !unsafe { (*entry).dll_base }.is_null() {
                return Some(entry);
            }
        }

        None
    }
}

#[inline]
pub(crate) unsafe fn hash_unicode_string(name: UnicodeString, offset: u32) -> Option<u32> {
    if name.buffer.is_null() {
        return None;
    }

    Some(unsafe { hash::hash_wide(name.buffer, name.len_u16(), offset) })
}

#[inline]
pub(crate) unsafe fn hash_unicode_string_without_dll(
    name: UnicodeString,
    offset: u32,
) -> Option<u32> {
    if name.buffer.is_null() {
        return None;
    }

    Some(unsafe { hash::hash_wide_without_dll(name.buffer, name.len_u16(), offset) })
}

#[inline]
pub(crate) unsafe fn hash_unicode_string_without_dll_case_insensitive(
    name: UnicodeString,
    offset: u32,
) -> Option<u32> {
    if name.buffer.is_null() {
        return None;
    }

    Some(unsafe {
        hash::hash_wide_without_dll_case_insensitive(name.buffer, name.len_u16(), offset)
    })
}

pub(crate) unsafe fn api_set_host_hash_by_name(
    contract_name: *const u8,
    contract_name_len: usize,
    parent_name: Option<UnicodeString>,
    offset: u32,
) -> Option<u32> {
    if contract_name.is_null() || contract_name_len == 0 {
        return None;
    }

    let namespace = unsafe { api_set_map()? };
    let count = unsafe { (*namespace).count };

    if count == 0 || count > MAX_API_SET_ENTRIES {
        return None;
    }

    let base = namespace.cast::<u8>();
    let entries = unsafe {
        base.add((*namespace).entry_offset as usize)
            .cast::<ApiSetNamespaceEntry>()
    };

    let mut index = 0;
    while index < count {
        let entry = unsafe { &*entries.add(index as usize) };
        let name = unsafe { base.add(entry.name_offset as usize).cast::<u16>() };
        let name_len = (entry.name_length as usize) / core::mem::size_of::<u16>();
        let hashed_len = (entry.hashed_length as usize) / core::mem::size_of::<u16>();

        if !name.is_null()
            && unsafe {
                api_set_name_matches(name, name_len, hashed_len, contract_name, contract_name_len)
            }
        {
            return unsafe { host_hash_from_api_set_entry(base, entry, parent_name, offset) };
        }

        index += 1;
    }

    None
}

unsafe fn api_set_name_matches(
    name: *const u16,
    name_len: usize,
    hashed_len: usize,
    contract_name: *const u8,
    contract_name_len: usize,
) -> bool {
    if unsafe {
        wide_equals_ascii_case_insensitive(name, name_len, contract_name, contract_name_len)
    } {
        return true;
    }

    hashed_len != 0
        && hashed_len <= name_len
        && hashed_len <= contract_name_len
        && unsafe {
            wide_equals_ascii_case_insensitive(name, hashed_len, contract_name, hashed_len)
        }
}

unsafe fn wide_equals_ascii_case_insensitive(
    wide: *const u16,
    wide_len: usize,
    ascii: *const u8,
    ascii_len: usize,
) -> bool {
    if wide_len != ascii_len {
        return false;
    }

    let mut index = 0;
    while index < wide_len {
        let wide_byte = unsafe { *wide.add(index) } as u8;
        let ascii_byte = unsafe { *ascii.add(index) };

        if fold_ascii(wide_byte) != fold_ascii(ascii_byte) {
            return false;
        }

        index += 1;
    }

    true
}

unsafe fn wide_equals_wide_case_insensitive(
    left: *const u16,
    left_len: usize,
    right: *const u16,
    right_len: usize,
) -> bool {
    if left_len != right_len {
        return false;
    }

    let mut index = 0;
    while index < left_len {
        let left_byte = unsafe { *left.add(index) } as u8;
        let right_byte = unsafe { *right.add(index) } as u8;

        if fold_ascii(left_byte) != fold_ascii(right_byte) {
            return false;
        }

        index += 1;
    }

    true
}

unsafe fn wide_equals_wide_case_insensitive_without_dll(
    left: *const u16,
    left_len: usize,
    right: *const u16,
    right_len: usize,
) -> bool {
    unsafe {
        wide_equals_wide_case_insensitive(
            left,
            len_without_dll(left, left_len),
            right,
            len_without_dll(right, right_len),
        )
    }
}

unsafe fn len_without_dll(ptr: *const u16, len: usize) -> usize {
    if len < 4 {
        return len;
    }

    let suffix = len - 4;
    let dot = unsafe { *ptr.add(suffix) } as u8;
    let d = unsafe { *ptr.add(suffix + 1) } as u8;
    let l1 = unsafe { *ptr.add(suffix + 2) } as u8;
    let l2 = unsafe { *ptr.add(suffix + 3) } as u8;

    if dot == b'.' && fold_ascii(d) == b'd' && fold_ascii(l1) == b'l' && fold_ascii(l2) == b'l' {
        suffix
    } else {
        len
    }
}

#[inline]
const fn fold_ascii(byte: u8) -> u8 {
    if byte >= b'A' && byte <= b'Z' {
        byte | (1 << 5)
    } else {
        byte
    }
}

unsafe fn host_hash_from_api_set_entry(
    base: *const u8,
    entry: &ApiSetNamespaceEntry,
    parent_name: Option<UnicodeString>,
    offset: u32,
) -> Option<u32> {
    if entry.value_count == 0 || entry.value_count > MAX_API_SET_VALUES {
        return None;
    }

    let values = unsafe {
        base.add(entry.value_offset as usize)
            .cast::<ApiSetValueEntry>()
    };
    let mut default_host = None;
    let mut first_host = None;
    let mut index = 0;

    while index < entry.value_count {
        let value = unsafe { &*values.add(index as usize) };

        if value.value_length != 0 {
            let host = unsafe { base.add(value.value_offset as usize).cast::<u16>() };
            let host_len = (value.value_length as usize) / core::mem::size_of::<u16>();
            let host_hash =
                unsafe { hash::hash_wide_without_dll_case_insensitive(host, host_len, offset) };

            first_host = first_host.or(Some(host_hash));

            if value.name_length == 0 {
                default_host = Some(host_hash);
                index += 1;
                continue;
            }

            if let Some(parent_name) = parent_name {
                let name = unsafe { base.add(value.name_offset as usize).cast::<u16>() };
                let name_len = (value.name_length as usize) / core::mem::size_of::<u16>();

                if !name.is_null()
                    && !parent_name.buffer.is_null()
                    && unsafe {
                        wide_equals_wide_case_insensitive_without_dll(
                            name,
                            name_len,
                            parent_name.buffer,
                            parent_name.len_u16(),
                        )
                    }
                {
                    return Some(host_hash);
                }
            }
        }

        index += 1;
    }

    default_host.or(first_host)
}

pub(crate) unsafe fn base_name_for_module(
    module_base: *mut core::ffi::c_void,
) -> Option<UnicodeString> {
    let modules = unsafe { LoadedModules::new()? };

    for entry in modules {
        if unsafe { (*entry).dll_base } == module_base {
            return Some(unsafe { (*entry).base_dll_name });
        }
    }

    None
}
