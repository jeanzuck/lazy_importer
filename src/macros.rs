#[macro_export]
macro_rules! li_module {
    ($name:literal) => {{
        const OFFSET: u32 = $crate::__private::const_random!(u32);
        const HASH: u64 = $crate::__private::khash($name, OFFSET);
        const CACHE_KEY: usize = $crate::__private::cache_key($name);

        $crate::LazyModule::<HASH>::with_cache_key(CACHE_KEY)
    }};
}

#[macro_export]
macro_rules! li_fn {
    ($name:literal) => {{
        const OFFSET: u32 = $crate::__private::const_random!(u32);
        const HASH: u64 = $crate::__private::khash($name, OFFSET);
        const CACHE_KEY: usize = $crate::__private::cache_key($name);

        $crate::LazyFunction::<HASH>::with_cache_key(CACHE_KEY)
    }};
}
