#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_vtable {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            abi_version = $abi_version:expr;
            $(@min_size($min_size:expr);)?
            $($field:ident: $field_ty:ty),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[repr(C)]
        $vis struct $name {
            pub size: usize,
            pub abi_version: u32,
            pub capabilities: u64,
            pub instance: *mut std::ffi::c_void,
            $(pub $field: $field_ty,)+
        }

        impl $name {
            pub const ABI_VERSION: u32 = $abi_version;
            pub const MIN_SIZE: usize = $crate::__xabi_select_min_size!(
                std::mem::size_of::<Self>()
                $(, $min_size)?
            );

            pub fn validate(&self) -> $crate::Result<()> {
                $crate::validate_size(
                    self.size,
                    Self::MIN_SIZE,
                    stringify!($name),
                )?;
                $crate::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!($name),
                )?;
                Ok(())
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_select_min_size {
    ($default:expr) => {
        $default
    };
    ($default:expr, $min_size:expr) => {
        $min_size
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_field_available {
    ($vtable:expr, $vtable_ty:ty, $field:tt) => {{
        let size = ($vtable).size;
        let field_end =
            std::mem::offset_of!($vtable_ty, $field) + std::mem::size_of_val(&($vtable).$field);
        size >= field_end
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_manifest {
    (
        exports: [
            $(
                {
                    abi_id: $abi_id:expr,
                    name: $name:expr,
                    version: $version:expr,
                    make: $make:expr $(,)?
                }
            ),+ $(,)?
        ]
    ) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn xabi_manifest() -> *const $crate::XabiManifest {
            &XABI_MANIFEST
        }

        static XABI_EXPORTS: [$crate::XabiExport; $crate::__xabi_count_exprs!($($abi_id),+)] = [
            $(
                $crate::XabiExport {
                    abi_id: $crate::XabiStr::from_static($abi_id),
                    name: $crate::XabiStr::from_static($name),
                    version: $version,
                    make: $make,
                },
            )+
        ];

        static XABI_MANIFEST: $crate::XabiManifest =
            $crate::XabiManifest::new(&XABI_EXPORTS);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_ffi_code {
    (
        $(#[$meta:meta])*
        $vis:vis unsafe extern "C" fn $name:ident(
            $($arg:ident: $arg_ty:ty),* $(,)?
        ) -> i32 $body:block
    ) => {
        $(#[$meta])*
        $vis unsafe extern "C" fn $name($($arg: $arg_ty),*) -> i32 {
            $crate::catch_unwind_code(|| $body)
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_ffi_owned {
    (
        $(#[$meta:meta])*
        $vis:vis unsafe extern "C" fn $name:ident(
            $($arg:ident: $arg_ty:ty),* $(,)?
        ) -> $ret:ty $body:block
    ) => {
        $(#[$meta])*
        $vis unsafe extern "C" fn $name($($arg: $arg_ty),*) -> $ret {
            $crate::catch_unwind_owned(|| $body)
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_ffi_void {
    (
        $(#[$meta:meta])*
        $vis:vis unsafe extern "C" fn $name:ident(
            $($arg:ident: $arg_ty:ty),* $(,)?
        ) $body:block
    ) => {
        $(#[$meta])*
        $vis unsafe extern "C" fn $name($($arg: $arg_ty),*) {
            let _ = $crate::catch_unwind_code(|| {
                $body
                $crate::OK
            });
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_handle {
    (
        $vis:vis struct $name:ident for $vtable:ty {
            error = $error:ty;
        }
    ) => {
        $vis struct $name {
            vtable: std::ptr::NonNull<$vtable>,
            _library: std::sync::Arc<$crate::ModuleHandle>,
        }

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl $name {
            /// # Safety
            ///
            /// `vtable` must be a valid owned vtable produced by the plugin, and `library` must
            /// keep the code backing all function pointers loaded.
            pub unsafe fn from_vtable(
                vtable: *mut $vtable,
                library: std::sync::Arc<$crate::ModuleHandle>,
            ) -> std::result::Result<Self, $error> {
                let vtable = std::ptr::NonNull::new(vtable)
                    .ok_or_else(|| <$error>::new(concat!(stringify!($vtable), " pointer is null")))?;
                unsafe { vtable.as_ref() }.validate().map_err(<$error>::from)?;
                Ok(Self {
                    vtable,
                    _library: library,
                })
            }

            fn vtable(&self) -> &$vtable {
                unsafe { self.vtable.as_ref() }
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                unsafe {
                    (self.vtable().release)(self.vtable.as_ptr());
                }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_raw_export_handle {
    (
        $vis:vis struct $name:ident for $vtable:ty {
            error = $error:ty;
            abi_id = $abi_id:expr;
        }
    ) => {
        $crate::__xabi_raw_handle! {
            $vis struct $name for $vtable {
                error = $error;
            }
        }

        impl $name {
            /// # Safety
            ///
            /// `entry.make` must return a valid owned vtable that follows this trait ABI, and
            /// `library` must keep the code backing all function pointers loaded.
            pub unsafe fn from_export(
                export: &$crate::XabiExport,
                library: std::sync::Arc<$crate::ModuleHandle>,
            ) -> std::result::Result<Self, $error> {
                let abi_id = unsafe { export.abi_id.as_str() }.map_err(<$error>::from)?;
                if abi_id != $abi_id {
                    return Err(<$error>::new(format!(
                        "module export has abi_id {abi_id}, expected {}",
                        $abi_id
                    )));
                }

                let raw = unsafe { (export.make)() } as *mut $vtable;
                unsafe { Self::from_vtable(raw, library) }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_count_exprs {
    ($($value:expr),* $(,)?) => {
        <[()]>::len(&[$($crate::__xabi_replace_expr!(($value) ())),*])
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_replace_expr {
    (($value:expr) $replacement:expr) => {
        $replacement
    };
}
