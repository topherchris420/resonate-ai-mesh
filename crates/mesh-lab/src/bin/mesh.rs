use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = mesh_lab::cli::Cli::parse();
    std::process::exit(mesh_lab::cli::main(cli).await);
}
