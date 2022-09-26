/// Creates an id type, which is similar to an integer
#[macro_export]
macro_rules! make_id_type {
    ($type:ident, $int_type:ident) => {
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $type($int_type);

        impl $type {
            pub const fn from(id: $int_type) -> Self {
                Self(id)
            }

            pub const fn into(self) -> $int_type {
                self.0
            }
        }

        impl core::fmt::Display for $type {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };

    ($type:tt) => {
        $crate::make_id_type!($type, usize);
    };
}

/// Creates an id type, which is similar to an integer
///
/// This version does not generate Default or From implementations
/// Use this if additional validation is needed for the id
#[macro_export]
macro_rules! make_id_type_no_from {
    ($type:ident, $int_type:ident) => {
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $type($int_type);

        impl $type {
            pub const fn into(self) -> $int_type {
                self.0
            }
        }

        impl core::fmt::Display for $type {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };

    ($type:tt) => {
        $crate::make_id_type_no_from!($type, usize);
    };
}
