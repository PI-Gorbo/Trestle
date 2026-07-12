//! Conformance corpus: one program per directory under `tests/programs/`, each
//! snapshotted through up to three compiler stages via `insta`.
//!
//! Programs are tiered by complexity and dependency. Tier `00-basics` is the
//! foundation every later tier builds on; it is split into "houses" (literals,
//! operators, bindings, functions, conditionals, blocks) with one concern per program.
//!
//! Layout — every program lives in its own directory alongside its snapshots:
//!
//! ```text
//! programs/00-basics/operators/addition/
//!   addition.trsl            the source
//!   addition.ast.snap        parse()    -> ast::Program
//!   addition.analysed.snap   analyse()  -> AnalysedProgram (opt-in)
//!   addition.eval.snap       evaluate() -> Value           (opt-in)
//! ```
//!
//! Each `trsl_test!` line lists the stages that are currently expected to pass
//! for that program and generates one `#[test]` per stage (`<name>_ast`,
//! `<name>_analysed`, `<name>_eval`). Stages are opt-in because `analyse` and
//! `evaluate` are still being built out — add `analyse`/`eval` to a program's list
//! once that stage works for it. See the macro docs below.

use miette::{NamedSource, Report};
use trestle::analyse::AnalysisError;
use trestle::evaluate::Environment;

/// Render a batch of analysis errors as miette's fancy diagnostics, with the
/// program source attached so each error shows its snippet + caret.
fn render_analysis_errors(path: &str, src: &str, errors: Vec<AnalysisError>) -> String {
    errors
        .into_iter()
        .map(|e| {
            let report = Report::new(e).with_source_code(NamedSource::new(path, src.to_string()));
            format!("{report:?}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A compiler stage to snapshot. Each maps to a public entry point and a
/// snapshot-file suffix (`.ast` / `.analysed` / `.eval`).
enum Stage {
    Ast,
    Analyse,
    Eval,
}

/// Run one stage of one program and snapshot its `Debug` output *next to the
/// program*.
///
/// `path` is the program's path relative to `programs/`, e.g.
/// `"00-basics/operators/addition/addition.trsl"`. The snapshot is written to
/// that same directory with the program's stem plus the stage suffix, so a file
/// named exactly `addition.ast.snap` lands beside `addition.trsl`.
///
/// `analyse` and `evaluate` are `todo!()` today, so only `ast` is wired up corpus-wide.
fn run_stage(path: &str, src: &str, stage: Stage) {
    // Split the relative path into its directory and file stem:
    //   dir  = "00-basics/operators/addition"   (where the .snap is written)
    //   stem = "addition"                        (the snapshot name prefix)
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let stem = path
        .rsplit('/')
        .next()
        .unwrap()
        .strip_suffix(".trsl")
        .unwrap_or(path);

    // `set_snapshot_path` is resolved relative to this file's directory
    // (`crates/trestle/tests/`), so this co-locates the snapshot with the
    // program. Dropping the module prefix + naming the snapshot explicitly makes
    // the file exactly `<stem>.<stage>.snap` (no `corpus__` prefix).
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(format!("programs/{dir}"));
    settings.set_prepend_module_to_snapshot(false);

    settings.bind(|| {
        let program =
            trestle::parse(src).unwrap_or_else(|e| panic!("failed to parse `{path}`:\n{e:?}"));
        match stage {
            Stage::Ast => {
                insta::assert_debug_snapshot!(format!("{stem}.ast"), program);
            }
            Stage::Analyse => {
                let analysed = trestle::analyse::analyse(program).unwrap_or_else(|e| {
                    panic!(
                        "failed to analyse `{path}`:\n{}",
                        render_analysis_errors(path, src, e)
                    )
                });
                insta::assert_debug_snapshot!(format!("{stem}.analysed"), analysed);
            }
            Stage::Eval => {
                let analysed = trestle::analyse::analyse(program).unwrap_or_else(|e| {
                    panic!(
                        "failed to analyse `{path}`:\n{}",
                        render_analysis_errors(path, src, e)
                    )
                });
                let env = Environment::empty();
                let value = trestle::evaluate::evaluate(&env, &analysed)
                    .unwrap_or_else(|e| panic!("failed to eval `{path}`:\n{e:?}"));
                insta::assert_debug_snapshot!(format!("{stem}.eval"), value);
            }
        }
    });
}

/// Register a program's conformance tests.
///
/// The path is the program's location under `programs/`, e.g.
/// `"00-basics/operators/addition/addition.trsl"`. Each active stage becomes its
/// own `#[test]` (`<name>_ast`, `<name>_analysed`, `<name>_eval`).
///
/// - `trsl_test!(name, "path.trsl")` — default stage list `[ast]`.
/// - `trsl_test!(name, "path.trsl", [ast, analyse, eval])` — opt into more stages
///   as `analyse`/`evaluate` come online for that program. `analyse` and
///   `evaluate` are `todo!()` today, so only `ast` is wired up corpus-wide.
/// - `trsl_test!(name, "path.trsl", ignore = "reason")` — work-in-progress
///   program (e.g. syntax not implemented yet); every generated stage test is
///   reported as *ignored* until the `ignore = "…"` argument is removed. Combine
///   with a stage list as `trsl_test!(name, "path.trsl", [ast], ignore = "…")`.
macro_rules! trsl_test {
    // ── Public forms ──────────────────────────────────────────
    ($name:ident, $path:literal) => {
        trsl_test!($name, $path, [ast]);
    };
    ($name:ident, $path:literal, ignore = $reason:literal) => {
        trsl_test!($name, $path, [ast], ignore = $reason);
    };
    ($name:ident, $path:literal, [ $($stage:ident),+ $(,)? ]) => {
        $( trsl_test!(@stage $name, $path, $stage); )+
    };
    ($name:ident, $path:literal, [ $($stage:ident),+ $(,)? ], ignore = $reason:literal) => {
        $( trsl_test!(@stage $name, $path, $stage, ignore = $reason); )+
    };

    // ── Per-stage `#[test]` generators (one fn per stage) ─────
    // The optional `, ignore = "…"` tail applies `#[ignore]` to the fn.
    (@stage $name:ident, $path:literal, ast $(, ignore = $reason:literal)?) => {
        paste::paste! {
            #[test]
            $(#[ignore = $reason])?
            fn [<$name _ast>]() {
                run_stage($path, include_str!(concat!("programs/", $path)), Stage::Ast);
            }
        }
    };
    (@stage $name:ident, $path:literal, analyse $(, ignore = $reason:literal)?) => {
        paste::paste! {
            #[test]
            $(#[ignore = $reason])?
            fn [<$name _analysed>]() {
                run_stage($path, include_str!(concat!("programs/", $path)), Stage::Analyse);
            }
        }
    };
    (@stage $name:ident, $path:literal, eval $(, ignore = $reason:literal)?) => {
        paste::paste! {
            #[test]
            $(#[ignore = $reason])?
            fn [<$name _eval>]() {
                run_stage($path, include_str!(concat!("programs/", $path)), Stage::Eval);
            }
        }
    };
}

// ══ 00 basics ═════════════════════════════════════════════
// The foundation tier. Everything later builds on these. Grouped into houses;
// one concern per program, related concerns kept side by side.

// ── literals ──────────────────────────────────────────────
trsl_test!(
    basics_literals_int,
    "00-basics/literals/int/int.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_literals_string,
    "00-basics/literals/string/string.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_literals_bool,
    "00-basics/literals/bool/bool.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_literals_float,
    "00-basics/literals/float/float.trsl",
    [ast, analyse]
);

// ── operators ─────────────────────────────────────────────
trsl_test!(
    basics_operators_addition,
    "00-basics/operators/addition/addition.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_operators_multiplication,
    "00-basics/operators/multiplication/multiplication.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_operators_precedence_and_grouping,
    "00-basics/operators/precedence-and-grouping/precedence-and-grouping.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_operators_subtraction,
    "00-basics/operators/subtraction/subtraction.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_operators_division,
    "00-basics/operators/division/division.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_operators_negation,
    "00-basics/operators/negation/negation.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_operators_comparison,
    "00-basics/operators/comparison/comparison.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_operators_logical,
    "00-basics/operators/logical/logical.trsl",
    [ast, analyse]
);

// ── bindings ──────────────────────────────────────────────
trsl_test!(
    basics_bindings_let_declaration,
    "00-basics/bindings/let-declaration/let-declaration.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_bindings_arithmetic,
    "00-basics/bindings/arithmetic/arithmetic.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_bindings_typed_let_declaration,
    "00-basics/bindings/typed-let-declaration/typed-let-declaration.trsl",
    ignore = "needs Let bindings to carry a type declaration on the AST"
);

// ── functions ─────────────────────────────────────────────
trsl_test!(
    basics_functions_lambda,
    "00-basics/functions/lambda/lambda.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_functions_typed_lambda,
    "00-basics/functions/typed-lambda/typed-lambda.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_functions_nested_lambda,
    "00-basics/functions/nested-lambda/nested-lambda.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_functions_function_invocation,
    "00-basics/functions/function-invocation/function-invocation.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_functions_currying,
    "00-basics/functions/currying/currying.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_functions_partial_application,
    "00-basics/functions/partial-application/partial-application.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_functions_zero_param_lambda,
    "00-basics/functions/zero-param-lambda/zero-param-lambda.trsl",
    [ast, analyse]
);

// ── conditionals ──────────────────────────────────────────
// `if` parses into the AST, but analyse/eval reject it for now (see the
// `AnalysisError::Unsupported` stub in resolve_names). Re-add `analyse`/`eval` once
// `if` is threaded through resolve_names + type_check.
trsl_test!(
    basics_conditionals_if_expression,
    "00-basics/conditionals/if-expression/if-expression.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_conditionals_if_else_expression,
    "00-basics/conditionals/if-else-expression/if-else-expression.trsl",
    [ast, analyse]
);

// ── blocks ────────────────────────────────────────────────
trsl_test!(
    basics_blocks_block_single_expr,
    "00-basics/blocks/block-single-expr/block-single-expr.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_blocks_block_multi_expr,
    "00-basics/blocks/block-multi-expr/block-multi-expr.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_blocks_block_with_bindings,
    "00-basics/blocks/block-with-bindings/block-with-bindings.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_blocks_nested_block,
    "00-basics/blocks/nested-block/nested-block.trsl",
    [ast, analyse]
);
trsl_test!(
    basics_blocks_if_block,
    "00-basics/blocks/if-block/if-block.trsl",
    [ast, analyse],
    ignore = "needs if-expression lowering (ast::If + build/resolve/type-check arms)"
);
trsl_test!(
    basics_blocks_if_else_block,
    "00-basics/blocks/if-else-block/if-else-block.trsl",
    [ast, analyse],
    ignore = "needs if-expression lowering (ast::If + build/resolve/type-check arms)"
);

// ══ 01 pipelines ══════════════════════════════════════════
trsl_test!(
    pipelines_pipeline,
    "01-pipelines/pipeline/pipeline.trsl",
    ignore = "needs the |> operator + leading-pipe continuation"
);
trsl_test!(
    pipelines_single_line_pipe,
    "01-pipelines/single-line-pipe/single-line-pipe.trsl",
    ignore = "needs the |> operator"
);
trsl_test!(
    pipelines_builder_as_pipeline,
    "01-pipelines/builder-as-pipeline/builder-as-pipeline.trsl",
    ignore = "needs the |> operator + partial application"
);

// ══ 02 control flow ═══════════════════════════════════════
trsl_test!(
    control_match_expression,
    "02-control-flow/match-expression/match-expression.trsl",
    ignore = "needs match / pattern matching — proposed syntax"
);

// ══ 03 records and ADTs ═══════════════════════════════════
trsl_test!(
    records_records,
    "03-records-and-adts/records/records.trsl",
    ignore = "needs record types + literals"
);
trsl_test!(
    records_field_access,
    "03-records-and-adts/field-access/field-access.trsl",
    ignore = "needs record field access via `.`"
);
trsl_test!(
    records_algebraic_data_types,
    "03-records-and-adts/algebraic-data-types/algebraic-data-types.trsl",
    ignore = "needs ADTs + constructors + match"
);

// ══ 04 generics ═══════════════════════════════════════════
trsl_test!(
    generics_generic_functions,
    "04-generics/generic-functions/generic-functions.trsl",
    ignore = "needs type parameters"
);
trsl_test!(
    generics_higher_order_data_types,
    "04-generics/higher-order-data-types/higher-order-data-types.trsl",
    ignore = "needs generic data types"
);

// ══ 05 effects ════════════════════════════════════════════
trsl_test!(
    effects_effect_block,
    "05-effects/effect-block/effect-block.trsl",
    ignore = "needs the effect system"
);
trsl_test!(
    effects_main_as_effect,
    "05-effects/main-as-effect/main-as-effect.trsl",
    ignore = "needs the effect system"
);
trsl_test!(
    effects_railway_errors,
    "05-effects/railway-errors/railway-errors.trsl",
    ignore = "needs the effect system"
);
