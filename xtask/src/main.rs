use devx_cmd::{run, Cmd};
use khonsu_tools::universal::{
    anyhow,
    clap::{self, Parser},
    code_coverage::CodeCoverage,
};

#[derive(Debug, Parser)]
enum Args {
    BuildWebApp {
        #[clap(long = "release")]
        release: bool,
    },
    GenerateCodeCoverageReport {
        #[clap(long = "install-dependencies")]
        install_dependencies: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args {
        Args::BuildWebApp { release } => build_web_app(release)?,
        Args::GenerateCodeCoverageReport {
            install_dependencies,
        } => CodeCoverage::<CodeCoverageConfig>::execute(install_dependencies)?,
    };
    Ok(())
}

fn build_web_app(release: bool) -> Result<(), anyhow::Error> {
    let mut cmd = Cmd::new("cargo");
    let cmd = cmd.args([
        "build",
        "--package",
        "minority-game-client",
        "--target",
        "wasm32-unknown-unknown",
        "--target-dir",
        "target/wasm",
    ]);
    if release {
        cmd.arg("--release");
    }
    cmd.run()?;

    execute_wasm_bindgen(
        if release {
            "target/wasm/wasm32-unknown-unknown/release/minority-game-client.wasm"
        } else {
            "target/wasm/wasm32-unknown-unknown/debug/minority-game-client.wasm"
        },
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

impl khonsu_tools::universal::code_coverage::Config for CodeCoverageConfig {}
