use cargo_testdox::get_cargo_test_output;

fn main() {
    let output = get_cargo_test_output(std::env::args().skip(2).collect());
    for result in &output.results {
        println!("{result}");
    }
    if let Some(error) = &output.error {
        if !output.results.is_empty() {
            println!();
        }
        eprintln!("{error}");
    }
    if output.failed() {
        std::process::exit(1);
    }
}
