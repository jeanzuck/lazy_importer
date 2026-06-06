#[macro_export]
macro_rules! li_module {
    ($name:literal) => {{
        static CACHE: $crate::__private::Cache = $crate::__private::Cache::new();
        const OFFSET: u32 = $crate::__private::const_random!(u32);
        const HASH: u64 = $crate::__private::khash($name, OFFSET);

        $crate::LazyModule::<HASH>::with_cache(&CACHE)
    }};
}

#[macro_export]
macro_rules! li_fn {
    ($name:ident) => {{
        static CACHE: $crate::__private::Cache = $crate::__private::Cache::new();
        const OFFSET: u32 = $crate::__private::const_random!(u32);
        const HASH: u64 = $crate::__private::khash(stringify!($name), OFFSET);

        $crate::LazyFunction::<HASH>::with_cache(&CACHE)
    }};
    ($name:literal) => {{
        static CACHE: $crate::__private::Cache = $crate::__private::Cache::new();
        const OFFSET: u32 = $crate::__private::const_random!(u32);
        const HASH: u64 = $crate::__private::khash($name, OFFSET);

        $crate::LazyFunction::<HASH>::with_cache(&CACHE)
    }};
}
