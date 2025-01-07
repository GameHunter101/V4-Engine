use hello_world::Temp;

mod hello_world;
mod font_render;
mod workload_test;

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
                "workload_test" => {
                    workload_test::main();
                }
                _ => {println!("Please select a valid example")}
            }
        },
        None => println!("Please select a example."),
    }
    /* let test = Temp {
        hi: "hi".to_string(),
        ..Default::default()
    }; */
}
