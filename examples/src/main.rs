mod compute;
mod font_render;
mod hello_world;
mod textures;
mod workload_test;

fn main() {
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
