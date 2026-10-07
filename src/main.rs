fn main() -> color_eyre::Result<std::process::ExitCode> {
    color_eyre::install()?;
    shtodo::run()
}
