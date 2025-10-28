use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for membership tests on containers with non-trivial elements that
/// prevent Python's `LOAD_CONST` bytecode optimization.
///
/// ## Why is this bad?
/// Python's bytecode compiler can optimize membership tests against simple
/// literal containers (like `x in (1, 2, 3)`) by converting them to a single
/// `LOAD_CONST` operation. However, when containers contain non-trivial values
/// (like dictionary literals, list literals, function calls, or comprehensions),
/// Python must reconstruct the container on every membership test, leading to
/// significant performance degradation.
///
/// This is particularly problematic in hot code paths, as the container is
/// rebuilt every time the membership test is evaluated.
///
/// ## Example
///
/// ```python
/// # Dictionary literal forces BUILD_CONST_KEY_MAP bytecode operation
/// if key in {"foo": 1, "bar": 2, "baz": 3}:
///     ...
///
/// # List of lists forces BUILD_LIST operations
/// if item in [[1, 2], [3, 4]]:
///     ...
/// ```
///
/// Use instead:
///
/// ```python
/// # Direct equality checks
/// if key == "foo" or key == "bar" or key == "baz":
///     ...
///
/// # Or pre-compute at module level
/// VALID_KEYS = {"foo": 1, "bar": 2, "baz": 3}
/// if key in VALID_KEYS:
///     ...
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe because:
/// - For dictionary tests, it converts to checking keys only, which changes
///   semantics if the original intent was to check in `dict.keys()` vs `dict.items()`.
/// - Converting `x in container` to `x == a or x == b` changes behavior for
///   custom `__eq__` implementations that may have side effects.
/// - The transformation may change short-circuit evaluation order.
/// - Comments within the expression might be lost during the transformation.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.13.0")]
pub(crate) struct InefficientMembershipTest {
    container_type: ContainerType,
}

impl Violation for InefficientMembershipTest {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let InefficientMembershipTest { container_type } = self;
        match container_type {
            ContainerType::Dict => {
                "Membership test against dict literal requires reconstructing the dict on each test; use equality comparison or pre-compute the dict".to_string()
            }
            ContainerType::ListWithComplexElements => {
                "Membership test against list/tuple with complex elements requires reconstructing elements on each test; use equality comparison or pre-compute the container".to_string()
            }
            ContainerType::SetWithComplexElements => {
                "Membership test against set with complex elements requires reconstructing elements on each test; use equality comparison or pre-compute the set".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to equality comparison".to_string())
    }
}

/// RUF066
pub(crate) fn inefficient_membership_test(checker: &Checker, compare: &ast::ExprCompare) {
    let [op] = &*compare.ops else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = &*compare.comparators else {
        return;
    };

    let Some(container_type) = has_non_trivial_elements(right) else {
        return;
    };

    let mut diagnostic = checker.report_diagnostic(
        InefficientMembershipTest { container_type },
        compare.range(),
    );

    let applicability = if checker.comment_ranges().intersects(compare.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Unsafe // Always unsafe due to semantic changes
    };

    if let Some(fix) = generate_fix(compare, right, op, container_type, checker) {
        diagnostic.set_fix(Fix::applicable_edit(fix, applicability));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContainerType {
    Dict,
    ListWithComplexElements,
    SetWithComplexElements,
}

/// Check if the expression contains non-trivial elements that prevent LOAD_CONST optimization.
fn has_non_trivial_elements(expr: &Expr) -> Option<ContainerType> {
    match expr {
        // Dict literals always require BUILD_CONST_KEY_MAP or BUILD_MAP (except empty ones)
        Expr::Dict(ast::ExprDict { items, .. }) => {
            if items.is_empty() {
                None
            } else {
                Some(ContainerType::Dict)
            }
        }

        // Check list/tuple elements
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            if elts.is_empty() {
                return None;
            }
            if elts.iter().any(is_complex_element) {
                Some(ContainerType::ListWithComplexElements)
            } else {
                None
            }
        }

        // Check set elements
        Expr::Set(ast::ExprSet { elts, .. }) => {
            if elts.is_empty() {
                return None;
            }
            if elts.iter().any(is_complex_element) {
                Some(ContainerType::SetWithComplexElements)
            } else {
                None
            }
        }

        _ => None,
    }
}

/// Check if an element is complex (requires runtime construction).
fn is_complex_element(expr: &Expr) -> bool {
    match expr {
        // Literals and simple values are trivial
        Expr::NumberLiteral(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_) => false,

        // Names are trivial (they're just LOAD_NAME)
        Expr::Name(_) => false,

        // Simple tuples of trivial elements are OK (they become single constants)
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(is_complex_element),

        // Everything else is complex:
        // - Dict literals (BUILD_MAP)
        // - List literals (BUILD_LIST)
        // - Set literals (BUILD_SET)
        // - Function calls
        // - Comprehensions
        // - Lambda
        // - Operations
        _ => true,
    }
}

/// Generate a fix by converting to equality comparisons.
fn generate_fix(
    compare: &ast::ExprCompare,
    container: &Expr,
    op: &CmpOp,
    container_type: ContainerType,
    checker: &Checker,
) -> Option<Edit> {
    let elements = extract_comparable_elements(container, container_type, checker)?;

    // Don't generate fix for empty containers or too many elements
    if elements.is_empty() || elements.len() > 10 {
        return None;
    }

    let left = checker.locator().slice(compare.left.as_ref());
    let is_not_in = matches!(op, CmpOp::NotIn);

    let replacement = if elements.len() == 1 {
        // Single element: x in [a] -> x == a (or x != a for not in)
        let element = &elements[0];
        let eq_op = if is_not_in { "!=" } else { "==" };
        format!("{left} {eq_op} {element}")
    } else {
        // Multiple elements: x in [a, b, c] -> x == a or x == b or x == c
        let logical_op = if is_not_in { " and " } else { " or " };
        let eq_op = if is_not_in { "!=" } else { "==" };

        let comparisons: Vec<String> = elements
            .iter()
            .map(|element| format!("{left} {eq_op} {element}"))
            .collect();

        comparisons.join(logical_op)
    };

    Some(Edit::range_replacement(replacement, compare.range()))
}

/// Extract elements that can be compared for equality.
fn extract_comparable_elements(
    container: &Expr,
    container_type: ContainerType,
    checker: &Checker,
) -> Option<Vec<String>> {
    match container_type {
        ContainerType::Dict => {
            // For dicts, extract keys
            if let Expr::Dict(ast::ExprDict { items, .. }) = container {
                let keys: Option<Vec<String>> = items
                    .iter()
                    .map(|item| {
                        item.key.as_ref().and_then(|key| {
                            // Only handle simple literal keys
                            match key {
                                Expr::StringLiteral(_)
                                | Expr::NumberLiteral(_)
                                | Expr::BooleanLiteral(_)
                                | Expr::NoneLiteral(_) => {
                                    Some(checker.locator().slice(key).to_string())
                                }
                                _ => None,
                            }
                        })
                    })
                    .collect();
                keys
            } else {
                None
            }
        }
        ContainerType::ListWithComplexElements | ContainerType::SetWithComplexElements => {
            // For now, don't generate fix for lists/sets with complex elements
            // as the transformation is less straightforward
            None
        }
    }
}
