fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .format(true)
        .compile(&["../../proto/functionbench_pmem_local.proto"], &["../../proto"])?;
    Ok(())
}
