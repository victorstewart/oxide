fn main() {
    if let Err(err) = oxide_perf_runner::run_from_env() {
        eprintln!("{:#}", err);
        std::process::exit(2);
    }
}
