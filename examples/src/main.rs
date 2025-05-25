mod compute;
mod font_render;
mod hello_world;
mod textures;
mod workload_test;

#[derive(Debug)]
struct Comp {
    foo: u32,
    bar: u32,
    baz: f32,
    default_1: bool,
    default_2: Option<Vec<u32>>,
}

impl Comp {
    fn builder() -> CompBuilder<Unset, Unset, Unset> {
        CompBuilder {
            foo: None,
            bar: None,
            baz: None,
            default_1: false,
            default_2: None,
            _marker: std::marker::PhantomData,
        }
    }
}

struct Set;
struct Unset;

trait HasFoo {}
trait HasBar {}
trait HasBaz {}

impl HasFoo for Set {}
impl HasBar for Set {}
impl HasBaz for Set {}

struct CompBuilder<Foo, Bar, Baz> {
    foo: Option<u32>,
    bar: Option<u32>,
    baz: Option<f32>,
    default_1: bool,
    default_2: Option<Vec<u32>>,
    _marker: std::marker::PhantomData<(Foo, Bar, Baz)>,
}

impl<Bar, Baz> CompBuilder<Unset, Bar, Baz> {
    fn foo(self, foo: u32) -> CompBuilder<Set, Bar, Baz> {
        CompBuilder::<Set, Bar, Baz> {
            foo: Some(foo),
            bar: self.bar,
            baz: self.baz,
            default_1: self.default_1,
            default_2: self.default_2,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Foo, Baz> CompBuilder<Foo, Unset, Baz> {
    fn bar(self, bar: u32) -> CompBuilder<Foo, Set, Baz> {
        CompBuilder::<Foo, Set, Baz> {
            foo: self.foo,
            bar: Some(bar),
            baz: self.baz,
            default_1: self.default_1,
            default_2: self.default_2,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Foo, Bar> CompBuilder<Foo, Bar, Unset> {
    fn baz(self, baz: f32) -> CompBuilder<Foo, Bar, Set> {
        CompBuilder::<Foo, Bar, Set> {
            foo: self.foo,
            bar: self.bar,
            baz: Some(baz),
            default_1: self.default_1,
            default_2: self.default_2,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Foo, Bar, Baz> CompBuilder<Foo, Bar, Baz> {
    fn default_1(self, default_1: bool) -> Self {
        Self { default_1, ..self }
    }

    fn default_2(self, default_2: Option<Vec<u32>>) -> Self {
        Self { default_2, ..self }
    }
}

impl<Foo, Bar, Baz> CompBuilder<Foo, Bar, Baz>
where
    Foo: HasFoo,
    Bar: HasBar,
    Baz: HasBaz,
{
    fn build(self) -> Comp {
        Comp {
            foo: self.foo.unwrap(),
            bar: self.bar.unwrap(),
            baz: self.baz.unwrap(),
            default_1: self.default_1,
            default_2: self.default_2,
        }
    }
}

fn main() {
    let test = Comp::builder().foo(2).bar(2).baz(0.0).build();
    dbg!(test);
    match std::env::args().nth(1) {
        Some(args) => match args.as_str() {
            "hello_world" => {
                hello_world::main();
            }
            "font_render" => {
                font_render::main();
            }
            "workload_test" => {
                workload_test::main();
            }
            "compute" => {
                compute::main();
            }
            "textures" => {
                textures::main();
            }
            _ => {
                println!("Please select a valid example")
            }
        },
        None => println!("Please select a example."),
    }
}

#[derive(Debug)]
pub struct CameraComponent {
    field_of_view: f32,
    aspect_ratio: f32,
    near_plane: f32,
    far_plane: f32,
    id: v4::ecs::component::ComponentId,
    parent_entity_id: v4::ecs::entity::EntityId,
    is_initialized: bool,
    is_enabled: bool,
}
pub struct CameraComponentBuilder<FieldOfView, AspectRatio, NearPlane, FarPlane> {
    field_of_view: Option<f32>,
    aspect_ratio: Option<f32>,
    near_plane: Option<f32>,
    far_plane: Option<f32>,
    id: v4::ecs::component::ComponentId,
    parent_entity_id: v4::ecs::entity::EntityId,
    is_initialized: bool,
    is_enabled: bool,
    _marker: std::marker::PhantomData<(FieldOfView, AspectRatio, NearPlane, FarPlane)>,
}
struct Set;
struct Unset;
trait HasFieldOfView {}
trait HasAspectRatio {}
trait HasNearPlane {}
trait HasFarPlane {}
impl HasFieldOfView for Set {}
impl HasAspectRatio for Set {}
impl HasNearPlane for Set {}
impl HasFarPlane for Set {}
impl<AspectRatio, NearPlane, FarPlane>
    CameraComponentBuilder<Unset, AspectRatio, NearPlane, FarPlane>
{
    fn field_of_view(
        self,
        field_of_view: Option<f32>,
    ) -> CameraComponentBuilder<Set, AspectRatio, NearPlane, FarPlane> {
        CameraComponentBuilder::<Unset, AspectRatio, NearPlane, FarPlane> {
            field_of_view: Some(field_of_view),
            ..self
        }
    }
}
impl<FieldOfView, NearPlane, FarPlane>
    CameraComponentBuilder<FieldOfView, Unset, NearPlane, FarPlane>
{
    fn aspect_ratio(
        self,
        aspect_ratio: f32,
    ) -> CameraComponentBuilder<FieldOfView, Set, NearPlane, FarPlane> {
        CameraComponentBuilder::<FieldOfView, Set, NearPlane, FarPlane> {
            aspect_ratio: Some(aspect_ratio),
            ..self
        }
    }
}
impl<FieldOfView, AspectRatio, FarPlane>
    CameraComponentBuilder<FieldOfView, AspectRatio, Unset, FarPlane>
{
    fn near_plane(
        self,
        near_plane: Option<f32>,
    ) -> CameraComponentBuilder<FieldOfView, AspectRatio, Set, FarPlane> {
        CameraComponentBuilder::<FieldOfView, AspectRatio, Set, FarPlane> {
            near_plane: Some(near_plane),
            ..self
        }
    }
}
impl<FieldOfView, AspectRatio, NearPlane>
    CameraComponentBuilder<FieldOfView, AspectRatio, NearPlane, Unset>
{
    fn far_plane(
        self,
        far_plane: Option<f32>,
    ) -> CameraComponentBuilder<FieldOfView, AspectRatio, NearPlane, Set> {
        CameraComponentBuilder::<FieldOfView, AspectRatio, NearPlane, Set> {
            far_plane: Some(far_plane),
            ..self
        }
    }
}
impl v4::ecs::component::ComponentDetails for CameraComponent {
    fn id(&self) -> v4::ecs::component::ComponentId {
        self.id
    }
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
    fn set_initialized(&mut self) {
        self.is_initialized = true;
    }
    fn parent_entity_id(&self) -> v4::ecs::entity::EntityId {
        self.parent_entity_id
    }
    fn set_parent_entity(&mut self, parent_id: v4::ecs::entity::EntityId) {
        self.parent_entity_id = parent_id;
    }
    fn is_enabled(&self) -> bool {
        self.is_enabled
    }
    fn set_enabled_state(&mut self, enabled_state: bool) {
        self.is_enabled = enabled_state;
    }
    fn rendering_order(&self) -> i32 {
        0
    }
}
