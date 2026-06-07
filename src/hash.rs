//! Hashing primitives shared by the resolver and public macros.

const FNV_PRIME: u32 = 16_777_619;
const CACHE_KEY_OFFSET_A: u32 = 2_166_136_261;
const CACHE_KEY_OFFSET_B: u32 = 0x811c_9dc5 ^ 0xa5a5_5a5a;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ForwardedHashes {
    pub(crate) module_hash: u32,
    pub(crate) module_name: *const u8,
    pub(crate) module_name_len: usize,
    pub(crate) function_hash: u32,
}

#[inline(always)]
pub(crate) const fn get_hash(pair: u64) -> u32 {
    pair as u32
}

#[inline(always)]
pub(crate) const fn get_offset(pair: u64) -> u32 {
    (pair >> 32) as u32
}

#[inline(always)]
const fn default_case_sensitive() -> bool {
    !cfg!(feature = "case-insensitive")
}

#[inline(always)]
const fn normalize_ascii(byte: u8, case_sensitive: bool) -> u8 {
    if !case_sensitive && byte >= b'A' && byte <= b'Z' {
        byte | (1 << 5)
    } else {
        byte
    }
}

#[inline(always)]
const fn hash_single_with(value: u32, byte: u8, case_sensitive: bool) -> u32 {
    (value ^ (normalize_ascii(byte, case_sensitive) as u32)).wrapping_mul(FNV_PRIME)
}

#[inline(always)]
const fn hash_single(value: u32, byte: u8) -> u32 {
    hash_single_with(value, byte, default_case_sensitive())
}

#[inline(always)]
const fn khash_impl(input: &str, offset: u32) -> u32 {
    let mut value = offset;
    let bytes = input.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        value = hash_single(value, bytes[index]);
        index += 1;
    }

    value
}

#[inline(always)]
pub const fn khash(input: &str, offset: u32) -> u64 {
    ((offset as u64) << 32) | (khash_impl(input, offset) as u64)
}

#[inline(always)]
pub const fn cache_key(input: &str) -> usize {
    let key = if core::mem::size_of::<usize>() >= core::mem::size_of::<u64>() {
        let upper = khash_impl(input, CACHE_KEY_OFFSET_A) as u64;
        let lower = khash_impl(input, CACHE_KEY_OFFSET_B) as u64;

        ((upper << 32) | lower) as usize
    } else {
        khash_impl(input, CACHE_KEY_OFFSET_A) as usize
    };

    if key == 0 { 1 } else { key }
}

#[inline]
pub(crate) unsafe fn hash_c_str(mut ptr: *const u8, offset: u32) -> u32 {
    let mut value = offset;

    loop {
        let byte = unsafe { *ptr };
        if byte == 0 {
            return value;
        }

        value = hash_single(value, byte);
        ptr = unsafe { ptr.add(1) };
    }
}

#[inline]
pub(crate) unsafe fn hash_wide(ptr: *const u16, len_u16: usize, offset: u32) -> u32 {
    unsafe { hash_wide_with(ptr, len_u16, offset, default_case_sensitive()) }
}

#[inline]
unsafe fn hash_wide_with(
    ptr: *const u16,
    len_u16: usize,
    offset: u32,
    case_sensitive: bool,
) -> u32 {
    let mut value = offset;
    let mut index = 0;

    while index < len_u16 {
        let code_unit = unsafe { *ptr.add(index) };
        value = hash_single_with(value, code_unit as u8, case_sensitive);
        index += 1;
    }

    value
}

#[inline]
pub(crate) unsafe fn hash_wide_without_dll(ptr: *const u16, len_u16: usize, offset: u32) -> u32 {
    unsafe { hash_wide_without_dll_with(ptr, len_u16, offset, default_case_sensitive()) }
}

#[inline]
pub(crate) unsafe fn hash_wide_without_dll_case_insensitive(
    ptr: *const u16,
    len_u16: usize,
    offset: u32,
) -> u32 {
    unsafe { hash_wide_without_dll_with(ptr, len_u16, offset, false) }
}

#[inline]
unsafe fn hash_wide_without_dll_with(
    ptr: *const u16,
    len_u16: usize,
    offset: u32,
    case_sensitive: bool,
) -> u32 {
    let mut len_u16 = len_u16;
    if len_u16 >= 4 {
        let suffix = len_u16 - 4;
        let dot = unsafe { *ptr.add(suffix) } as u8;
        let d = unsafe { *ptr.add(suffix + 1) } as u8;
        let l1 = unsafe { *ptr.add(suffix + 2) } as u8;
        let l2 = unsafe { *ptr.add(suffix + 3) } as u8;

        if dot == b'.'
            && normalize_ascii(d, false) == b'd'
            && normalize_ascii(l1, false) == b'l'
            && normalize_ascii(l2, false) == b'l'
        {
            len_u16 -= 4;
        }
    }

    unsafe { hash_wide_with(ptr, len_u16, offset, case_sensitive) }
}

#[inline]
pub(crate) unsafe fn hash_forwarded(mut ptr: *const u8, offset: u32) -> Option<ForwardedHashes> {
    let mut module_hash = offset;
    let mut function_hash = offset;
    let module_name = ptr;
    let mut module_name_len = 0;

    loop {
        let byte = unsafe { *ptr };
        if byte == 0 {
            return None;
        }
        if byte == b'.' {
            break;
        }

        module_hash = hash_single(module_hash, byte);
        ptr = unsafe { ptr.add(1) };
        module_name_len += 1;
    }

    ptr = unsafe { ptr.add(1) };

    loop {
        let byte = unsafe { *ptr };
        if byte == 0 {
            break;
        }

        function_hash = hash_single(function_hash, byte);
        ptr = unsafe { ptr.add(1) };
    }

    Some(ForwardedHashes {
        module_hash,
        module_name,
        module_name_len,
        function_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FNV_OFFSET_BASIS: u32 = 2_166_136_261;

    #[test]
    fn fnv1a_matches_known_value() {
        assert_eq!(khash_impl("hello", FNV_OFFSET_BASIS), 0x4f9f_2cab);
    }

    #[test]
    fn khash_stores_offset_and_hash() {
        const OFFSET: u32 = 0x1234_5678;
        const PAIR: u64 = khash("GetCurrentProcessId", OFFSET);

        assert_eq!(get_offset(PAIR), OFFSET);
        assert_eq!(get_hash(PAIR), khash_impl("GetCurrentProcessId", OFFSET));
    }

    #[test]
    fn cache_key_is_stable_and_non_zero() {
        assert_ne!(cache_key("GetCurrentProcessId"), 0);
        assert_eq!(
            cache_key("GetCurrentProcessId"),
            cache_key("GetCurrentProcessId")
        );
    }

    #[test]
    fn case_sensitivity_follows_feature_flag() {
        let upper = khash_impl("KERNEL32.DLL", FNV_OFFSET_BASIS);
        let lower = khash_impl("kernel32.dll", FNV_OFFSET_BASIS);

        if cfg!(feature = "case-insensitive") {
            assert_eq!(upper, lower);
        } else {
            assert_ne!(upper, lower);
        }
    }

    #[test]
    fn cache_key_case_sensitivity_follows_feature_flag() {
        let upper = cache_key("KERNEL32.DLL");
        let lower = cache_key("kernel32.dll");

        if cfg!(feature = "case-insensitive") {
            assert_eq!(upper, lower);
        } else {
            assert_ne!(upper, lower);
        }
    }
}
