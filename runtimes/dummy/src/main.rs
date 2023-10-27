use std::env;
use std::io;

fn main() {
    let mut input = String::new();
    let args: Vec<String> = env::args().collect();

    println!("Dummy runtime. Args: {args:?}");

    loop {
        input.clear();

        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let trimmed_input = input.trim();
                if trimmed_input == "stop" {
                    println!("Stopping.");
                    break;
                }
            }
            Err(error) => {
                eprintln!("Error reading input: {}", error);
                // break;
            }
        }
    }
}
