use devx_cmd::run;
use khonsu_tools::{anyhow, code_coverage::CodeCoverage};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
enum Args {
    BuildWebApp,
    GenerateCodeCoverageReport {
        #[structopt(long = "install-dependencies")]
        install_dependencies: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::from_args();
    match args {
        Args::BuildWebApp => build_web_app()?,
        Args::GenerateCodeCoverageReport {
            install_dependencies,
        } => CodeCoverage::<CodeCoverageConfig>::execute(install_dependencies)?,
    };
    Ok(())
}

fn build_web_app() -> Result<(), anyhow::Error> {
    run!(
        "cargo",
        "build",
        "--package",
        "minority-game-client",
        "--target",
        "wasm32-unknown-unknown",
        "--target-dir",
        "target/wasm",
    )?;

    execute_wasm_bindgen(
        "target/wasm/wasm32-unknown-unknown/debug/minority-game-client.wasm",
        "client/pkg/",
    )?;

    Ok(())
}

fn execute_wasm_bindgen(wasm_path: &str, out_path: &str) -> Result<(), devx_cmd::Error> {
    println!("Executing wasm-bindgen (cargo install wasm-bindgen if you don't have this)");
    run!(
        "wasm-bindgen",
        wasm_path,
        "--target",
        "web",
        "--out-dir",
        out_path,
        "--remove-producers-section"
    )
}

struct CodeCoverageConfig;

impl khonsu_tools::code_coverage::Config for CodeCoverageConfig {}
