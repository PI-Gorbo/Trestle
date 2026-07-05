//! Conformance corpus: one test per `.trsl` program under `tests/programs/`.
//! Each test parses the program and snapshots its AST (via `insta`).
//! Add a program by adding one `trsl_test!` line below.

/// Generate a `#[test]` that parses a program and snapshots its AST.
///
/// - `trsl_test!(name, "path.trsl")` — active test, must parse.
/// - `trsl_test!(name, "path.trsl", ignore = "reason")` — work-in-progress
///   feature; the test is registered but reported as *ignored* until the
///   `ignore = "…"` argument is removed.
macro_rules! trsl_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let src = include_str!(concat!("programs/", $path));
            let program = trestle::parse(src)
                .unwrap_or_else(|e| panic!("failed to parse `{}`:\n{e:?}", $path));
            insta::assert_debug_snapshot!(program);
        }
    };
    ($name:ident, $path:literal, ignore = $reason:literal) => {
        #[test]
        #[ignore = $reason]
        fn $name() {
            let src = include_str!(concat!("programs/", $path));
            let program = trestle::parse(src)
                .unwrap_or_else(|e| panic!("failed to parse `{}`:\n{e:?}", $path));
            insta::assert_debug_snapshot!(program);
        }
    };
}

// ── 00 primitives ─────────────────────────────────────────

trsl_test!(primitives_int, "00-primitives/int.trsl");
trsl_test!(
    primitives_let_declaration,
    "00-primitives/let-declaration.trsl"
);
trsl_test!(primitives_addition, "00-primitives/addition.trsl");
trsl_test!(
    primitives_multiplication,
    "00-primitives/multiplication.trsl"
);
trsl_test!(primitives_lambda, "00-primitives/lambda.trsl");
trsl_test!(
    primitives_function_invocation,
    "00-primitives/function-invocation.trsl"
);
trsl_test!(primitives_typed_lambda, "00-primitives/typed-lambda.trsl");

// ── 01 basics ─────────────────────────────────────────────
trsl_test!(basics_arithmetic, "01-basics/arithmetic.trsl");
trsl_test!(basics_basics, "01-basics/basics.trsl");
trsl_test!(
    basics_precedence_and_grouping,
    "01-basics/precedence-and-grouping.trsl"
);

// ── 02 functions ──────────────────────────────────────────
trsl_test!(functions_currying, "02-functions/currying.trsl");
trsl_test!(functions_lambdas, "02-functions/lambdas.trsl");
trsl_test!(
    functions_arrow_functions,
    "02-functions/arrow-functions.trsl"
);
trsl_test!(
    functions_partial_application,
    "02-functions/partial-application.trsl"
);

// ── 03 pipelines ──────────────────────────────────────────
trsl_test!(
    pipelines_builder_as_pipeline,
    "03-pipelines/builder-as-pipeline.trsl",
    ignore = "needs the |> operator + partial application (tier 03)"
);
trsl_test!(
    pipelines_pipeline,
    "03-pipelines/pipeline.trsl",
    ignore = "needs the |> operator + leading-pipe continuation (tier 03)"
);
trsl_test!(
    pipelines_single_line_pipe,
    "03-pipelines/single-line-pipe.trsl",
    ignore = "needs the |> operator (tier 03)"
);

// ── 04 values and types ───────────────────────────────────
trsl_test!(
    values_booleans_and_comparison,
    "04-values-and-types/booleans-and-comparison.trsl",
    ignore = "needs booleans + comparison/boolean operators (tier 04)"
);
trsl_test!(
    values_floats_and_negatives,
    "04-values-and-types/floats-and-negatives.trsl",
    ignore = "needs float literals + negative numbers (tier 04)"
);
trsl_test!(
    values_strings,
    "04-values-and-types/strings.trsl",
    ignore = "needs string literals (tier 04)"
);

// ── 05 control flow ───────────────────────────────────────
trsl_test!(
    control_if_else_expression,
    "05-control-flow/if-else-expression.trsl",
    ignore = "needs if/else expressions (tier 05) — proposed syntax"
);
trsl_test!(
    control_match_expression,
    "05-control-flow/match-expression.trsl",
    ignore = "needs match / pattern matching (tier 05) — proposed syntax"
);

// ── 06 records and ADTs ───────────────────────────────────
trsl_test!(
    records_algebraic_data_types,
    "06-records-and-adts/algebraic-data-types.trsl",
    ignore = "needs ADTs + constructors + match (tiers 05/06)"
);
trsl_test!(
    records_field_access,
    "06-records-and-adts/field-access.trsl",
    ignore = "needs record field access via `.` (tier 06)"
);
trsl_test!(
    records_records,
    "06-records-and-adts/records.trsl",
    ignore = "needs record types + literals (tier 06)"
);

// ── 07 generics ───────────────────────────────────────────
trsl_test!(
    generics_generic_functions,
    "07-generics/generic-functions.trsl",
    ignore = "needs type parameters (tier 07)"
);
trsl_test!(
    generics_higher_order_data_types,
    "07-generics/higher-order-data-types.trsl",
    ignore = "needs generic data types (tier 07)"
);

// ── 08 effects ────────────────────────────────────────────
trsl_test!(
    effects_effect_block,
    "08-effects/effect-block.trsl",
    ignore = "needs the effect system (tier 08)"
);
trsl_test!(
    effects_main_as_effect,
    "08-effects/main-as-effect.trsl",
    ignore = "needs the effect system (tier 08)"
);
trsl_test!(
    effects_railway_errors,
    "08-effects/railway-errors.trsl",
    ignore = "needs the effect system (tier 08)"
);
