mod hello_world;
mod font_render;

fn main() {
    match std::env::args().nth(1) {
        Some(args) => {
            match args.as_str() {
                "hello_world" => {
                    hello_world::main();
                }
                "font_render" => {
                    font_render::main();
                }
                _ => {println!("Please select a valid example")}
            }
        },
        None => println!("Please select a example."),
    }
}
