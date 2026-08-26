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
//!   addition.ast.snap        parse()    -> ast::ParsedProgram
//!   addition.analysed.snap   analyse()  -> TypeCheckedProgram (opt-in)
//!   addition.eval.snap       evaluate() -> Value              (opt-in)
//! ```
//!
//! Each `trsl_test!` line lists the stages that are currently expected to pass
//! for that program and generates one `#[test]` per stage (`<name>_ast`,
//! `<name>_analysed`, `<name>_eval`, `<name>_error`). Stages are opt-in because
//! `analyse` and `evaluate` are still being built out — add `analyse`/`eval` to a
//! program's list once that stage works for it. Use `error` for a program that is
//! *meant* to be rejected: it snapshots the batch of `AnalysisError`s. See the
//! macro docs below.

use miette::{Diagnostic, NamedSource, Report};
use pest::Parser as _;
use trestle::AnalysisError;
use trestle::parse::{Rule, TrestleParser, build_program};

/// Render one failure as miette's fancy diagnostic, with the program source attached so the
/// error shows its snippet + caret.
fn render_error<E: Diagnostic + Send + Sync + 'static>(path: &str, src: &str, error: E) -> String {
    let report = Report::new(error).with_source_code(NamedSource::new(path, src.to_string()));
    format!("{report:?}")
}

/// Render a phase-tagged analysis failure, one [`render_error`] per error in the batch.
fn render_analysis_error(path: &str, src: &str, error: AnalysisError) -> String {
    fn render_batch<E: Diagnostic + Send + Sync + 'static>(
        path: &str,
        src: &str,
        errors: Vec<E>,
    ) -> String {
        errors
            .into_iter()
            .map(|e| render_error(path, src, e))
            .collect::<Vec<_>>()
            .join("\n")
    }

    match error {
        AnalysisError::BindingResolution(errors) => render_batch(path, src, errors),
        AnalysisError::TypeCheck(errors) => render_batch(path, src, errors),
    }
}

/// A compiler stage to snapshot. Each maps to a public entry point and a
/// snapshot-file suffix (`.ast` / `.analysed` / `.eval` / `.error`).
enum Stage {
    Ast,
    Analyse,
    Eval,
    /// Analyse a program that is *expected to fail*, snapshotting the batch of
    /// `AnalysisError`s (suffix `.error`) rather than a success value. This is how
    /// the corpus pins down diagnostics — e.g. an out-of-scope reference.
    Error,
    /// Build a program that is expected to fail *before* analysis (suffix
    /// `.build-error`). Drives `TrestleParser` + `build_program` directly rather than
    /// `trestle::parse`, because that wrapper erases the structured `BuildError` into an
    /// opaque `Report` — the same reason `trestle-wasm` bypasses it.
    BuildError,
    /// Evaluate a program that analyses cleanly but is expected to fault at runtime
    /// (suffix `.eval-error`) — division by zero, arithmetic overflow.
    EvalError,
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
        // Handled before the shared parse below, which panics on a build failure — here the
        // failure *is* the subject.
        if let Stage::BuildError = stage {
            let pairs = TrestleParser::parse(Rule::program, src)
                .unwrap_or_else(|e| panic!("failed to parse `{path}`:\n{e}"));
            let pair = pairs
                .into_iter()
                .next()
                .expect("the program rule always yields exactly one pair");

            match build_program(pair) {
                Ok(_) => panic!("expected `{path}` to fail to build, but it succeeded"),
                Err(error) => insta::assert_debug_snapshot!(format!("{stem}.build-error"), error),
            }
            return;
        }

        let program =
            trestle::parse(src).unwrap_or_else(|e| panic!("failed to parse `{path}`:\n{e:?}"));
        match stage {
            Stage::Ast => {
                insta::assert_debug_snapshot!(format!("{stem}.ast"), program);
            }
            Stage::Analyse => {
                let analysed = trestle::analyse(program).unwrap_or_else(|e| {
                    panic!(
                        "failed to analyse `{path}`:\n{}",
                        render_analysis_error(path, src, e)
                    )
                });
                insta::assert_debug_snapshot!(format!("{stem}.analysed"), analysed);
            }
            Stage::Eval => {
                let analysed = trestle::analyse(program).unwrap_or_else(|e| {
                    panic!(
                        "failed to analyse `{path}`:\n{}",
                        render_analysis_error(path, src, e)
                    )
                });
                let value = trestle::evaluate::evaluate(analysed).unwrap_or_else(|e| {
                    panic!("failed to eval `{path}`:\n{}", render_error(path, src, e))
                });
                insta::assert_debug_snapshot!(format!("{stem}.eval"), value);
            }
            // Inverse of `Analyse`: the program is *meant* to be rejected, so a success is
            // the failure mode. Snapshot the errors' structured `Debug` (matching the other
            // stages' style) rather than the miette-rendered text, keeping the snapshot stable.
            Stage::Error => match trestle::analyse(program) {
                Ok(_) => panic!("expected `{path}` to fail analysis, but it succeeded"),
                Err(errors) => {
                    insta::assert_debug_snapshot!(format!("{stem}.error"), errors);
                }
            },
            // Inverse of `Eval`: the program is well-typed, so analysis must succeed; it is
            // *evaluation* that is expected to fault.
            Stage::EvalError => {
                let analysed = trestle::analyse(program).unwrap_or_else(|e| {
                    panic!(
                        "failed to analyse `{path}`:\n{}",
                        render_analysis_error(path, src, e)
                    )
                });
                match trestle::evaluate::evaluate(analysed) {
                    Ok(value) => {
                        panic!("expected `{path}` to fault at runtime, but it produced {value:?}")
                    }
                    Err(error) => {
                        insta::assert_debug_snapshot!(format!("{stem}.eval-error"), error)
                    }
                }
            }
            Stage::BuildError => unreachable!("handled above, before the shared parse"),
        }
    });
}

/// Register a program's conformance tests.
///
/// The path is the program's location under `programs/`, e.g.
/// `"00-basics/operators/addition/addition.trsl"`. Each active stage becomes its
/// own `#[test]` (`<name>_ast`, `<name>_analysed`, `<name>_eval`, `<name>_error`).
/// The `error` stage expects analysis to *fail* and snapshots the errors.
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
    (@stage $name:ident, $path:literal, build_error $(, ignore = $reason:literal)?) => {
        paste::paste! {
            #[test]
            $(#[ignore = $reason])?
            fn [<$name _build_error>]() {
                run_stage($path, include_str!(concat!("programs/", $path)), Stage::BuildError);
            }
        }
    };
    (@stage $name:ident, $path:literal, eval_error $(, ignore = $reason:literal)?) => {
        paste::paste! {
            #[test]
            $(#[ignore = $reason])?
            fn [<$name _eval_error>]() {
                run_stage($path, include_str!(concat!("programs/", $path)), Stage::EvalError);
            }
        }
    };
    (@stage $name:ident, $path:literal, error $(, ignore = $reason:literal)?) => {
        paste::paste! {
            #[test]
            $(#[ignore = $reason])?
            fn [<$name _error>]() {
                run_stage($path, include_str!(concat!("programs/", $path)), Stage::Error);
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
    [ast, analyse, eval]
);
trsl_test!(
    basics_literals_string,
    "00-basics/literals/string/string.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_literals_bool,
    "00-basics/literals/bool/bool.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_literals_float,
    "00-basics/literals/float/float.trsl",
    [ast, analyse, eval]
);

trsl_test!(
    basics_literals_unit,
    "00-basics/literals/unit/unit.trsl",
    [ast, analyse, eval]
);
// The literal forms side by side, one binding each — the program the playground uses to show
// what the Bindings panel makes of each of them.
trsl_test!(
    basics_literals_every_literal,
    "00-basics/literals/every-literal/every-literal.trsl",
    [ast, analyse, eval]
);
// Rejected while building the AST, so `ast` is not among its stages — there is no tree to
// snapshot.
trsl_test!(
    basics_literals_int_literal_out_of_range,
    "00-basics/literals/int-literal-out-of-range/int-literal-out-of-range.trsl",
    [build_error]
);

// ── operators ─────────────────────────────────────────────
trsl_test!(
    basics_operators_addition,
    "00-basics/operators/addition/addition.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_multiplication,
    "00-basics/operators/multiplication/multiplication.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_precedence_and_grouping,
    "00-basics/operators/precedence-and-grouping/precedence-and-grouping.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_subtraction,
    "00-basics/operators/subtraction/subtraction.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_division,
    "00-basics/operators/division/division.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_integer_division,
    "00-basics/operators/integer-division/integer-division.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_negation,
    "00-basics/operators/negation/negation.trsl",
    [ast, analyse, eval]
);
// Both analyse cleanly and fault at runtime, so they carry `analyse` but `eval_error` in
// place of `eval`.
trsl_test!(
    basics_operators_division_by_zero,
    "00-basics/operators/division-by-zero/division-by-zero.trsl",
    [ast, analyse, eval_error]
);
trsl_test!(
    basics_operators_arithmetic_overflow,
    "00-basics/operators/arithmetic-overflow/arithmetic-overflow.trsl",
    [ast, analyse, eval_error]
);
trsl_test!(
    basics_operators_comparison_greater_than,
    "00-basics/operators/comparison/greater-than/greater-than.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_comparison_less_than,
    "00-basics/operators/comparison/less-than/less-than.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_comparison_greater_or_equal,
    "00-basics/operators/comparison/greater-or-equal/greater-or-equal.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_comparison_less_or_equal,
    "00-basics/operators/comparison/less-or-equal/less-or-equal.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_comparison_equal,
    "00-basics/operators/comparison/equal/equal.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_comparison_not_equal,
    "00-basics/operators/comparison/not-equal/not-equal.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_logical_and,
    "00-basics/operators/logical/and/and.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_logical_or,
    "00-basics/operators/logical/or/or.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_operators_logical_negation,
    "00-basics/operators/logical/logical-negation/logical-negation.trsl",
    [ast, analyse, eval]
);
// The comparison and logical families combined into one readable program. The single-operator
// programs above pin each operator; this one pins that they compose, and is what the playground
// shows rather than a bare `5 > 3`.
trsl_test!(
    basics_operators_comparison_and_logic,
    "00-basics/operators/comparison-and-logic/comparison-and-logic.trsl",
    [ast, analyse, eval]
);

// ── bindings ──────────────────────────────────────────────
trsl_test!(
    basics_bindings_let_declaration,
    "00-basics/bindings/let-declaration/let-declaration.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_bindings_arithmetic,
    "00-basics/bindings/arithmetic/arithmetic.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_bindings_typed_let_declaration,
    "00-basics/bindings/typed-let-declaration/typed-let-declaration.trsl",
    [ast, analyse, eval]
);
// Re-declaring a name in the same scope is a `DuplicateBinding` error (shadowing
// needs a new scope). The `error` stage pins that diagnostic.
trsl_test!(
    basics_bindings_duplicate_binding,
    "00-basics/bindings/duplicate-binding/duplicate-binding.trsl",
    [ast, error]
);

// ── functions ─────────────────────────────────────────────
trsl_test!(
    basics_functions_lambda,
    "00-basics/functions/lambda/lambda.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_functions_typed_lambda,
    "00-basics/functions/typed-lambda/typed-lambda.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_functions_nested_lambda,
    "00-basics/functions/nested-lambda/nested-lambda.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_functions_function_invocation,
    "00-basics/functions/function-invocation/function-invocation.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_functions_currying,
    "00-basics/functions/currying/currying.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_functions_partial_application,
    "00-basics/functions/partial-application/partial-application.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_functions_zero_param_lambda,
    "00-basics/functions/zero-param-lambda/zero-param-lambda.trsl",
    [ast, analyse, eval]
);

trsl_test!(
    basics_functions_closures,
    "00-basics/functions/closures/closure.trsl",
    [ast, analyse, eval]
);

trsl_test!(
    basics_functions_function_typed_parameter,
    "00-basics/functions/function-typed-parameter/function-typed-parameter.trsl",
    ignore = "needs function type expressions in annotations"
);

// ── conditionals ──────────────────────────────────────────
// `if` is threaded end-to-end: it parses into the AST, resolves in
// `binding_resolution`, and type-checks in `type_check`.
trsl_test!(
    basics_conditionals_if_expression,
    "00-basics/conditionals/if-expression/if-expression.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_conditionals_if_else_expression,
    "00-basics/conditionals/if-else-expression/if-else-expression.trsl",
    [ast, analyse, eval]
);

trsl_test!(
    basics_conditionals_branches_share_a_variable,
    "00-basics/conditionals/branches-share-a-variable/branches-share-a-variable.trsl",
    [ast, analyse, eval]
);

// ── blocks ────────────────────────────────────────────────
trsl_test!(
    basics_blocks_block_single_expr,
    "00-basics/blocks/block-single-expr/block-single-expr.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_blocks_block_multi_expr,
    "00-basics/blocks/block-multi-expr/block-multi-expr.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_blocks_block_with_bindings,
    "00-basics/blocks/block-with-bindings/block-with-bindings.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_blocks_nested_block,
    "00-basics/blocks/nested-block/nested-block.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_blocks_if_block,
    "00-basics/blocks/if-block/if-block.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    basics_blocks_if_else_block,
    "00-basics/blocks/if-else-block/if-else-block.trsl",
    [ast, analyse, eval]
);
// A block-local binding referenced after its block closes is an `UnboundName`
// error — the block's scope does not leak. The `error` stage pins that diagnostic.
trsl_test!(
    basics_blocks_block_scope_leak,
    "00-basics/blocks/block-scope-leak/block-scope-leak.trsl",
    [ast, error]
);
// A block-local `let` may reuse an enclosing name: the inner binding shadows the
// outer one *within* the block, and the outer binding is restored once the block
// closes. The eval value pins that the inner binding wins and the outer is intact.
trsl_test!(
    basics_blocks_shadowing,
    "00-basics/blocks/shadowing/shadowing.trsl",
    [ast, analyse, eval]
);

// ══ 01 pipelines ══════════════════════════════════════════
trsl_test!(
    pipelines_pipeline,
    "01-pipelines/pipeline/pipeline.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    pipelines_single_line_pipe,
    "01-pipelines/single-line-pipe/single-line-pipe.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    pipelines_builder_as_pipeline,
    "01-pipelines/builder-as-pipeline/builder-as-pipeline.trsl",
    [ast, analyse, eval]
);

// == 01 unification ========================================
trsl_test!(
    unification_int_lambda_parameter,
    "01-unification/lambda-parameters/int-lambda-parameter.trsl",
    [ast, analyse, eval]
);

trsl_test!(
    unification_partially_known_function,
    "01-unification/partially-known-function/partially-known-function.trsl",
    [ast, analyse, eval]
);
// Self-application `x(x)` constrains x's variable to a function type containing itself. The
// occurs check has to reject it; the `error` stage pins the `InfiniteType` diagnostic, and the
// test terminating at all is the point — the cycle would make `subsitute` non-terminating.
trsl_test!(
    unification_infinite_type_self_application,
    "01-unification/infinite-type/self-application.trsl",
    [ast, error]
);

// Same occurs check, but the cycle runs *through a record field*: `x({ inner: x })`
// constrains `_0 := Fn(Record { inner: _0 }, _1)`. `root_occurs_in_type` has to descend
// into a record's field types the way it already does into an `Fn`'s.
trsl_test!(
    unification_infinite_type_record_self_reference,
    "01-unification/infinite-type/record-self-reference.trsl",
    [ast, error]
);

trsl_test!(
    unification_type_alias_declaration_record,
    "01-unification/type-alias-declaration/record/record.trsl",
    [ast, analyse, eval]
);
// The other half of a type declaration: naming a type that already exists, rather than
// describing a new record. An alias is structural, not nominal — `Celsius` *is* `Int`, so a
// value annotated with it unifies with plain `Int` arithmetic. Aliases chain, too.
trsl_test!(
    unification_type_alias_declaration_named_alias,
    "01-unification/type-alias-declaration/named-alias/named-alias.trsl",
    [ast, analyse, eval]
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
    [ast, analyse, eval]
);
trsl_test!(
    records_field_access,
    "03-records-and-adts/field-access/field-access.trsl",
    [ast, analyse, eval]
);
// An *unannotated* record binding: the binding gets a fresh type variable, so the literal
// is unified variable-against-record and the occurs check has to walk a `Type::Record`.
// The annotated programs above never take that path.
trsl_test!(
    records_inferred_record_let,
    "03-records-and-adts/inferred-record-let/inferred-record-let.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    records_nested_field_access,
    "03-records-and-adts/nested-field-access/nested-field-access.trsl",
    [ast, analyse, eval]
);
// The working half of `nested-record-types` below: a field's annotation is a bare identifier,
// so an inner record type has to be *named* first. Nesting the type is what makes the `.`
// chain check — `nestedValue.value` has to resolve to a record for the second `.` to land.
trsl_test!(
    records_nested_record_alias,
    "03-records-and-adts/nested-record-alias/nested-record-alias.trsl",
    [ast, analyse, eval]
);
// Tier 01's `builder-as-pipeline` over structured data: data-last steps chained with `|>`,
// each rebuilding a nested record rather than mutating it. Lives here rather than in
// `01-pipelines` because records are its highest dependency. It is also the program that will
// have to change when record update lands — every step spells out the fields it keeps only
// because `{ ...server, name: name }` does not exist yet.
trsl_test!(
    records_record_builder_pipeline,
    "03-records-and-adts/record-builder-pipeline/record-builder-pipeline.trsl",
    [ast, analyse, eval]
);
trsl_test!(
    records_nested_record_types,
    "03-records-and-adts/nested-record-types/nested-record-types.trsl",
    ignore = "needs *inline* record type expressions in field position"
);
trsl_test!(
    records_record_function_field,
    "03-records-and-adts/record-function-field/record-function-field.trsl",
    ignore = "needs function type expressions in annotations"
);
trsl_test!(
    records_field_call_chain,
    "03-records-and-adts/field-call-chain/field-call-chain.trsl",
    ignore = "needs mixed postfix chaining (a.b().c)"
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
