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
    let test = Comp::builder().foo(2).bar(2).build();
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
