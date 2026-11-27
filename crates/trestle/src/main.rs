use clap::{Parser, Subcommand};
use miette::{Diagnostic, NamedSource, Report};
use std::{
    fs::{self},
    io::{self},
    path::PathBuf,
};
use thiserror::Error;
use trestle::{
    binding_resolution::{self, BindingResolutionError},
    evaluate::{self, Value},
    parse::{self},
    type_check::{self, TypeCheckError},
};

#[derive(Debug, Error, Diagnostic)]
enum CliError {
    #[error("expected a .trsl file, got {0}")]
    #[diagnostic(help("trestle source files end in .trsl"))]
    BadExtension(PathBuf),

    #[error("failed to read .trsl file at {path}")]
    FailedToRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("could not resolve bindings")]
    BindingResolution {
        #[source_code]
        src: NamedSource<String>,
        #[related]
        errors: Vec<BindingResolutionError>,
    },

    #[error("type error")]
    TypeError {
        #[source_code]
        src: NamedSource<String>,
        #[related]
        errors: Vec<TypeCheckError>,
    },
}

#[derive(Parser)]
#[command(
    name = "Trestle",
    version,
    about = "This is the trestle cli. Use it to typecheck and run trestle programs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// executes a trestle program
    Run {
        path: PathBuf,
    },
    Typecheck {
        path: PathBuf,
    },
}

fn main() -> miette::Result<()> {
    match Cli::parse().command {
        Command::Run { path } => {
            if path.extension() != Some("trsl".as_ref()) {
                return Err(CliError::BadExtension(path).into());
            }

            // Early returns rather than `map_err`, so each error path can move its values instead of cloning them.
            let file_content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(source) => return Err(CliError::FailedToRead { path, source }.into()),
            };

            let src = NamedSource::new(path.display().to_string(), file_content);

            let parsed_file = match parse::parse(src.inner()) {
                Ok(program) => program,
                Err(error) => {
                    return Err(error
                        .with_source_code(src)
                        .wrap_err(format!("while parsing {}", path.display())));
                }
            };

            let resolved_program = match binding_resolution::resolve(parsed_file) {
                Ok(program) => program,
                Err(errors) => return Err(CliError::BindingResolution { src, errors }.into()),
            };

            let type_checked_program = match type_check::type_check(resolved_program) {
                Ok(program) => program,
                Err(errors) => return Err(CliError::TypeError { src, errors }.into()),
            };

            let evaluated_program = match evaluate::evaluate(type_checked_program) {
                Ok(value) => value,
                Err(error) => return Err(Report::new(error).with_source_code(src)),
            };

            // The result goes to stdout; diagnostics go to stderr via miette. `unit` prints nothing.
            match evaluated_program {
                Value::String(s) => println!("{}", s),
                Value::Unit => {}
                other => println!("{other}"),
            }
        }
        Command::Typecheck { path: _ } => todo!(),
    }
    Ok(())
}
