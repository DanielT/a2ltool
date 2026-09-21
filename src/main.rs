fn main() {
    match a2ltool::core(std::env::args_os()) {
        Ok(()) => {}
        Err(err) => {
            println!("{err}");
            std::process::exit(1);
        }
    }
}
