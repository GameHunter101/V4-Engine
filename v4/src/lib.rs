pub use v4_core::V4;
pub use v4_core::EngineDetails;
pub use v4_core::V4Builder;
pub use v4_macros::component;
pub use v4_macros::scene;

#[allow(unused_imports)]
pub(crate) mod v4 {
    pub(crate) mod ecs {
        pub(crate) use v4_core::ecs::*;
    }
    pub(crate) mod engine_support {
        pub(crate) use v4_core::engine_support::*;
    }
}

pub mod builtin_actions;

pub mod ecs {
    pub use v4_core::ecs::*;
}

pub mod engine_management {
    pub use v4_core::engine_management::*;
}

pub mod engine_support {
    pub use v4_core::engine_support::*;
}

pub mod builtin_components {
    pub mod mesh_component;
    pub mod transform_component;
    pub mod camera_component;
}

