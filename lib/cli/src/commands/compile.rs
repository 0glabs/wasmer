use crate::store::{EngineType, StoreOptions};
use crate::warning;
use anyhow::{Context, Result};
use std::path::PathBuf;
use structopt::StructOpt;
use wasmer::*;

#[derive(Debug, StructOpt)]
/// The options for the `wasmer compile` subcommand
pub struct Compile {
    /// Input file
    #[structopt(name = "FILE", parse(from_os_str))]
    path: PathBuf,

    /// Output file
    #[structopt(name = "OUTPUT", short = "o", parse(from_os_str))]
    output: PathBuf,

    /// Compilation Target triple
    #[structopt(long = "target")]
    target_triple: Option<Triple>,

    #[structopt(flatten)]
    store: StoreOptions,

    #[structopt(short = "m", multiple = true)]
    cpu_features: Vec<CpuFeature>,
}

impl Compile {
    /// Runs logic for the `compile` subcommand
    pub fn execute(&self) -> Result<()> {
        self.inner_execute()
            .context(format!("failed to compile `{}`", self.path.display()))
    }

    fn get_recommend_extension(
        &self,
        engine_type: &EngineType,
        target_triple: &Triple,
    ) -> &'static str {
        match engine_type {
            #[cfg(feature = "native")]
            EngineType::Native => {
                wasmer_engine_native::NativeArtifact::get_default_extension(target_triple)
            }
            #[cfg(feature = "jit")]
            EngineType::JIT => wasmer_engine_jit::JITArtifact::get_default_extension(target_triple),
            #[cfg(feature = "object-file")]
            EngineType::ObjectFile => {
                wasmer_engine_object_file::ObjectFileArtifact::get_default_extension(target_triple)
            }
            #[cfg(not(all(feature = "native", feature = "jit", feature = "object-file")))]
            _ => bail!("selected engine type is not compiled in"),
        }
    }

    fn inner_execute(&self) -> Result<()> {
        let target = self
            .target_triple
            .as_ref()
            .map(|target_triple| {
                let mut features = self
                    .cpu_features
                    .clone()
                    .into_iter()
                    .fold(CpuFeature::set(), |a, b| a | b);
                // Cranelift requires SSE2, so we have this "hack" for now to facilitate
                // usage
                features |= CpuFeature::SSE2;
                Target::new(target_triple.clone(), features)
            })
            .unwrap_or_default();
        let (store, engine_type, compiler_type) =
            self.store.get_store_for_target(target.clone())?;
        let output_filename = self
            .output
            .file_stem()
            .map(|osstr| osstr.to_string_lossy().to_string())
            .unwrap_or_default();
        let recommended_extension = self.get_recommend_extension(&engine_type, target.triple());
        match self.output.extension() {
            Some(ext) => {
                if ext != recommended_extension {
                    warning!("the output file has a wrong extension. We recommend using `{}.{}` for the chosen target", &output_filename, &recommended_extension)
                }
            },
            None => {
                warning!("the output file has no extension. We recommend using `{}.{}` for the chosen target", &output_filename, &recommended_extension)
            }
        }
        println!("Engine: {}", engine_type.to_string());
        println!("Compiler: {}", compiler_type.to_string());
        println!("Target: {}", target.triple());
        let module = Module::from_file(&store, &self.path)?;
        let _ = module.serialize_to_file(&self.output)?;
        // for C code
        let module_bytes = module.serialize()?;
        let mut header = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open("test.h")?;
        use std::io::Write;
        header
            .write(format!("const int module_bytes_len = {};\n", module_bytes.len()).as_bytes())?;
        header.write(b"extern const char WASMER_METADATA[];\n")?;
        // end c gen
        eprintln!(
            "✔ File compiled successfully to `{}`.",
            self.output.display(),
        );
        Ok(())
    }
}
