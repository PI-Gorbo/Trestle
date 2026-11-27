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
    type_check::{self, TypeCheckError, TypeCheckedProgram},
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
            let file_content = match read_trsl_file(&path) {
                Ok(value) => value,
                Err(value) => return Err(value),
            };

            let src = NamedSource::new(path.display().to_string(), file_content);

            let type_checked_program = type_check(path, &src)?;

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
        Command::Typecheck { path } => {
            let file_content = match read_trsl_file(&path) {
                Ok(value) => value,
                Err(value) => return Err(value),
            };

            let src = NamedSource::new(path.display().to_string(), file_content);

            type_check(path, &src)?;
        }
    }
    Ok(())
}

fn type_check(path: PathBuf, src: &NamedSource<String>) -> Result<TypeCheckedProgram, Report> {
    let parsed_file = match parse::parse(src.inner()) {
        Ok(program) => program,
        Err(error) => {
            return Err(error
                .with_source_code(src.clone())
                .wrap_err(format!("while parsing {}", path.display())));
        }
    };
    let resolved_program = match binding_resolution::resolve(parsed_file) {
        Ok(program) => program,
        Err(errors) => {
            return Err(CliError::BindingResolution {
                src: src.clone(),
                errors,
            }
            .into());
        }
    };
    let type_checked_program = match type_check::type_check(resolved_program) {
        Ok(program) => program,
        Err(errors) => {
            return Err(CliError::TypeError {
                src: src.clone(),
                errors,
            }
            .into());
        }
    };
    Ok(type_checked_program)
}

fn read_trsl_file(path: &PathBuf) -> Result<String, Report> {
    if path.extension() != Some("trsl".as_ref()) {
        return Err(CliError::BadExtension(path.to_path_buf()).into());
    }

    let file_content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(source) => {
            return Err(CliError::FailedToRead {
                path: path.to_path_buf(),
                source,
            }
            .into());
        }
    };

    Ok(file_content)
}
