//! Offline scientific calculator expression engine.
#![warn(clippy::all, rust_2018_idioms)]

use astro_float::{BigFloat, Consts, Radix, RoundingMode};
use num_bigint::{BigInt, BigUint};
use num_complex::Complex;
use num_rational::BigRational;
use num_traits::{One as _, Signed as _, Zero as _};
use std::{collections::BTreeMap, ops::Range};

pub const DEFAULT_PRECISION_BITS: usize = 256;
const LIMIT: usize = 4096;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    Lex,
    Parse,
    UnknownIdentifier,
    Domain,
    Dimension,
    Limit,
    Unsupported,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub span: Range<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Constant,
    Unit,
    Variable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub replacement: Range<usize>,
    pub insert: String,
    pub display: String,
    pub kind: CompletionKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub primary: String,
    pub approximation: Option<String>,
    pub assignment: Option<String>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DefinitionSnapshot {
    pub name: String,
    pub source: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub schema_version: u16,
    pub definitions: Vec<DefinitionSnapshot>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub restored: usize,
    pub discarded: Vec<Diagnostic>,
}

#[derive(Clone)]
enum Value {
    Rational(BigRational),
    Approx(BigFloat),
    Complex(Complex<BigRational>),
    Quantity(BigRational, [i8; 8], String, BigRational, bool),
}
pub struct Calculator {
    vars: BTreeMap<String, (Value, String)>,
}
impl Default for Calculator {
    fn default() -> Self {
        Self::new()
    }
}
impl Calculator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            vars: BTreeMap::new(),
        }
    }
    /// # Errors
    /// Returns a diagnostic for invalid syntax, domain, dimensions, or resource limits.
    pub fn evaluate(&mut self, input: &str) -> Result<Evaluation, Diagnostic> {
        self.eval(input, true)
    }
    /// # Errors
    /// Returns a diagnostic for invalid syntax, domain, dimensions, or resource limits.
    pub fn preview(&self, input: &str) -> Result<Option<Evaluation>, Diagnostic> {
        if input.trim().is_empty() {
            return Ok(None);
        }
        let mut c = Self::new();
        let _restore_report = c.restore(&self.snapshot());
        c.eval(input, true).map(Some)
    }
    #[must_use]
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            schema_version: 1,
            definitions: self
                .vars
                .iter()
                .map(|(name, (_, source))| DefinitionSnapshot {
                    name: name.clone(),
                    source: source.clone(),
                })
                .collect(),
        }
    }
    #[must_use]
    pub fn restore(&mut self, s: &SessionSnapshot) -> RestoreReport {
        self.vars.clear();
        if s.schema_version != 1 {
            return RestoreReport {
                restored: 0,
                discarded: s
                    .definitions
                    .iter()
                    .map(|d| {
                        err(
                            DiagnosticKind::Unsupported,
                            "Unsupported calculator session format.",
                            0..d.source.len(),
                        )
                    })
                    .collect(),
            };
        }
        let mut restored = 0;
        let mut discarded = Vec::new();
        for d in &s.definitions {
            match self.evaluate(&d.source) {
                Ok(_) => restored += 1,
                Err(e) => discarded.push(e),
            }
        }
        RestoreReport {
            restored,
            discarded,
        }
    }
    #[must_use]
    pub fn complete(&self, input: &str, cursor: usize) -> Vec<Completion> {
        if cursor > input.len() || !input.is_char_boundary(cursor) {
            return Vec::new();
        }
        let start = input[..cursor]
            .char_indices()
            .rev()
            .find_map(|(i, c)| (!c.is_ascii_alphanumeric() && c != '_').then_some(i + c.len_utf8()))
            .unwrap_or(0);
        let prefix = &input[start..cursor];
        let mut all: Vec<(String, CompletionKind)> = [
            "abs", "cbrt", "cos", "exp", "ln", "log", "log10", "log2", "sin", "sqrt", "tan",
        ]
        .into_iter()
        .map(|x| (x.to_owned(), CompletionKind::Function))
        .collect();
        all.extend(
            ["pi", "e", "tau", "i"]
                .into_iter()
                .map(|x| (x.to_owned(), CompletionKind::Constant)),
        );
        all.extend(
            unit_names()
                .into_iter()
                .map(|x| (x.to_owned(), CompletionKind::Unit)),
        );
        all.extend(
            self.vars
                .keys()
                .map(|x| (x.clone(), CompletionKind::Variable)),
        );
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all.into_iter()
            .filter(|(x, _)| x.starts_with(prefix))
            .map(|(insert, kind)| Completion {
                replacement: start..cursor,
                display: insert.clone(),
                insert,
                kind,
            })
            .collect()
    }
    fn eval(&mut self, input: &str, commit: bool) -> Result<Evaluation, Diagnostic> {
        if input.len() > LIMIT {
            return Err(err(
                DiagnosticKind::Limit,
                "Input exceeds 4096 bytes.",
                0..input.len(),
            ));
        }
        let (assignment, value) = {
            let parser = Parser::new(input, self)?;
            let assignment = if let Some((name, rest)) = input.split_once('=') {
                let name = name.trim();
                if !valid_name(name) || reserved(name) {
                    return Err(err(
                        DiagnosticKind::Parse,
                        "Invalid assignment target.",
                        0..name.len(),
                    ));
                }
                Some((name.to_owned(), parser.parse(rest.trim())?))
            } else {
                None
            };
            let value = assignment
                .as_ref()
                .map_or_else(|| parser.parse(input), |(_, value)| Ok(value.clone()))?;
            (assignment, value)
        };
        if commit && let Some((name, value)) = &assignment {
            self.vars
                .insert(name.clone(), (value.clone(), input.to_owned()));
        }
        let (primary, approximation) = display(&value)?;
        Ok(Evaluation {
            primary,
            approximation,
            assignment: assignment.map(|x| x.0),
        })
    }
}
struct Parser<'a> {
    calc: &'a Calculator,
}
impl<'a> Parser<'a> {
    fn new(input: &str, calc: &'a Calculator) -> Result<Self, Diagnostic> {
        if input.is_empty() {
            return Err(err(DiagnosticKind::Parse, "Expected an expression.", 0..0));
        }
        Ok(Self { calc })
    }
    fn parse(&self, s: &str) -> Result<Value, Diagnostic> {
        let s = strip_outer(s);
        if let Some(rest) = s.strip_prefix('-') {
            return negate(self.parse(rest.trim())?);
        }
        if let Some((a, b)) = s.split_once(" to ") {
            return convert(self.parse(a.trim())?, b.trim());
        }
        if let Some((number, unit)) = s.split_once(' ')
            && let Some((dim, scale, affine)) = unit_expression(unit.trim())
        {
            let Value::Rational(n) = self.parse(number.trim())? else {
                return Err(err(
                    DiagnosticKind::Dimension,
                    "A unit needs a real scalar.",
                    0..s.len(),
                ));
            };
            return Ok(Value::Quantity(
                n,
                dim,
                unit.trim().to_owned(),
                scale,
                affine,
            ));
        }
        if let Some((a, b)) = implicit_groups(s) {
            return binary(self.parse(a)?, self.parse(b)?, '*');
        }
        if let Some(prefix) = s.strip_suffix("pi").filter(|x| !x.is_empty()) {
            return binary(self.parse(prefix)?, approx_const("pi")?, '*');
        }
        if let Some((a, b, op)) = split_operator(s, &['+', '-']) {
            return binary(self.parse(a)?, self.parse(b)?, op);
        }
        if let Some((a, b, op)) = split_operator(s, &['*', '/']) {
            return binary(self.parse(a)?, self.parse(b)?, op);
        }
        if let Some((a, b)) = split_power(s) {
            return power(self.parse(a)?, self.parse(b)?);
        }
        if let Some(arg) = s.strip_suffix('!') {
            let value = self.parse(arg.trim())?;
            return factorial(&value);
        }
        if let Some((name, args)) = call(s) {
            let arguments = args
                .split(',')
                .map(|x| self.parse(x.trim()))
                .collect::<Result<Vec<_>, _>>()?;
            return function(name, &arguments);
        }
        if s == "pi" {
            return approx_const("pi");
        }
        if s == "e" {
            return approx_const("e");
        }
        if s == "tau" {
            return approx_const("tau");
        }
        if s == "i" {
            return Ok(Value::Complex(Complex::new(
                BigRational::zero(),
                BigRational::one(),
            )));
        }
        if let Some((v, _)) = self.calc.vars.get(s) {
            return Ok(v.clone());
        }
        parse_rational(s)
    }
}
#[expect(
    clippy::too_many_lines,
    reason = "binary dispatch keeps all Value combinations together"
)]
fn binary(a: Value, b: Value, op: char) -> Result<Value, Diagnostic> {
    match (a, b) {
        (Value::Rational(a), Value::Rational(b)) => match op {
            '+' => Ok(Value::Rational(a + b)),
            '-' => Ok(Value::Rational(a - b)),
            '*' => Ok(Value::Rational(a * b)),
            '/' => {
                if b.is_zero() {
                    Err(err(DiagnosticKind::Domain, "Division by zero.", 0..0))
                } else {
                    Ok(Value::Rational(a / b))
                }
            }
            _ => Err(err(DiagnosticKind::Parse, "Invalid operator.", 0..0)),
        },
        (Value::Approx(a), Value::Rational(b)) => approx_binary(&a, &rational_float(&b)?, op),
        (Value::Rational(a), Value::Approx(b)) => approx_binary(&rational_float(&a)?, &b, op),
        (Value::Approx(a), Value::Approx(b)) => approx_binary(&a, &b, op),
        (Value::Complex(a), Value::Complex(b)) => match op {
            '+' => Ok(Value::Complex(a + b)),
            '-' => Ok(Value::Complex(a - b)),
            '*' => Ok(Value::Complex(a * b)),
            _ => Err(err(
                DiagnosticKind::Unsupported,
                "Unsupported complex operation.",
                0..0,
            )),
        },
        (Value::Complex(a), Value::Rational(b)) => binary(
            Value::Complex(a),
            Value::Complex(Complex::new(b, BigRational::zero())),
            op,
        ),
        (Value::Rational(a), Value::Complex(b)) => binary(
            Value::Complex(Complex::new(a, BigRational::zero())),
            Value::Complex(b),
            op,
        ),
        (Value::Quantity(a, d, u, scale, affine), Value::Quantity(b, bd, bu, bscale, baffine)) => {
            if affine || baffine {
                return Err(err(
                    DiagnosticKind::Domain,
                    "Affine temperature units can only be converted.",
                    0..0,
                ));
            }
            match op {
                '+' | '-' => {
                    if d != bd {
                        return Err(err(
                            DiagnosticKind::Dimension,
                            "Incompatible dimensions.",
                            0..0,
                        ));
                    }
                    let converted_b = b * bscale / scale.clone();
                    let scalar = if op == '+' {
                        a + converted_b
                    } else {
                        a - converted_b
                    };
                    Ok(Value::Quantity(scalar, d, u, scale, false))
                }
                '*' | '/' => {
                    if op == '/' && b.is_zero() {
                        return Err(err(DiagnosticKind::Domain, "Division by zero.", 0..0));
                    }
                    let mut dim = d;
                    for (index, exponent) in bd.iter().enumerate() {
                        dim[index] += if op == '*' { *exponent } else { -*exponent };
                    }
                    let scalar = if op == '*' {
                        a * scale * b * bscale
                    } else {
                        a * scale / (b * bscale)
                    };
                    Ok(Value::Quantity(
                        scalar,
                        dim,
                        canonical_unit(dim, &u, &bu, op),
                        BigRational::one(),
                        false,
                    ))
                }
                _ => Err(err(DiagnosticKind::Parse, "Invalid operator.", 0..0)),
            }
        }
        (Value::Quantity(a, d, u, scale, affine), Value::Rational(b)) => {
            if affine {
                return Err(err(
                    DiagnosticKind::Domain,
                    "Affine temperature units can only be converted.",
                    0..0,
                ));
            }
            match op {
                '*' => Ok(Value::Quantity(a * b, d, u, scale, false)),
                '/' => {
                    if b.is_zero() {
                        Err(err(DiagnosticKind::Domain, "Division by zero.", 0..0))
                    } else {
                        Ok(Value::Quantity(a / b, d, u, scale, false))
                    }
                }
                _ => Err(err(
                    DiagnosticKind::Dimension,
                    "Quantities require compatible units.",
                    0..0,
                )),
            }
        }
        (Value::Rational(a), Value::Quantity(b, d, u, scale, affine)) => {
            if affine {
                return Err(err(
                    DiagnosticKind::Domain,
                    "Affine temperature units can only be converted.",
                    0..0,
                ));
            }
            match op {
                '*' => Ok(Value::Quantity(a * b, d, u, scale, false)),
                '/' => {
                    let mut dim = d;
                    for exponent in &mut dim {
                        *exponent = -*exponent;
                    }
                    Ok(Value::Quantity(
                        a / (b * scale),
                        dim,
                        format!("1/{u}"),
                        BigRational::one(),
                        false,
                    ))
                }
                _ => Err(err(
                    DiagnosticKind::Dimension,
                    "Quantities require compatible units.",
                    0..0,
                )),
            }
        }
        _ => Err(err(
            DiagnosticKind::Unsupported,
            "Unsupported operation.",
            0..0,
        )),
    }
}

fn canonical_unit(dim: [i8; 8], left: &str, right: &str, op: char) -> String {
    if dim == [2, 0, 0, 0, 0, 0, 0, 0] {
        "m²".to_owned()
    } else {
        format!("{left}{op}{right}")
    }
}
fn approx_binary(a: &BigFloat, b: &BigFloat, op: char) -> Result<Value, Diagnostic> {
    let p = DEFAULT_PRECISION_BITS;
    let r = match op {
        '+' => a.add(b, p, RoundingMode::ToEven),
        '-' => a.sub(b, p, RoundingMode::ToEven),
        '*' => a.mul(b, p, RoundingMode::ToEven),
        '/' => a.div(b, p, RoundingMode::ToEven),
        _ => return Err(err(DiagnosticKind::Parse, "Invalid operator.", 0..0)),
    };
    Ok(Value::Approx(r))
}
fn power(a: Value, b: Value) -> Result<Value, Diagnostic> {
    let (Value::Rational(a), Value::Rational(b)) = (a, b) else {
        return Err(err(
            DiagnosticKind::Unsupported,
            "Powers require real scalars.",
            0..0,
        ));
    };
    if !b.is_integer() {
        return Err(err(
            DiagnosticKind::Domain,
            "Exponent must be an integer.",
            0..0,
        ));
    }
    let n = num_traits::ToPrimitive::to_i32(&b.to_integer())
        .ok_or_else(|| err(DiagnosticKind::Limit, "Exponent exceeds 4096.", 0..0))?;
    if n.unsigned_abs() > 4096 {
        return Err(err(DiagnosticKind::Limit, "Exponent exceeds 4096.", 0..0));
    }
    if n >= 0 {
        Ok(Value::Rational(a.pow(n)))
    } else if a.is_zero() {
        Err(err(DiagnosticKind::Domain, "Division by zero.", 0..0))
    } else {
        Ok(Value::Rational(a.recip().pow(-n)))
    }
}
fn factorial(v: &Value) -> Result<Value, Diagnostic> {
    let value = nonnegative_integer(v)?;
    Ok(Value::Rational(BigRational::from_integer(
        factorial_integer(&value)?,
    )))
}

fn function(name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    let first = args
        .first()
        .ok_or_else(|| err(DiagnosticKind::Parse, "Expected an argument.", 0..0))?;
    match name {
        "sqrt" => sqrt_value(first),
        "abs" => match first {
            Value::Rational(value) => Ok(Value::Rational(value.abs())),
            _ => Err(err(
                DiagnosticKind::Unsupported,
                "abs requires a real scalar.",
                0..0,
            )),
        },
        "gcd" | "lcm" => integer_pair_function(name, args),
        "nCr" | "nPr" => combinatoric_function(name, args),
        "min" | "max" => min_max_function(name, args),
        "sin" | "cos" | "tan" | "ln" | "log" | "log10" | "log2" | "exp" | "cbrt" => {
            transcendental(name, args)
        }
        _ => Err(err(
            DiagnosticKind::UnknownIdentifier,
            format!("Unknown function: {name}."),
            0..0,
        )),
    }
}

fn integer_pair_function(name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    require_arity(name, args, 2)?;
    let a = nonnegative_integer(&args[0])?;
    let b = nonnegative_integer(&args[1])?;
    let gcd = bigint_gcd(a.clone(), b.clone());
    let result = if name == "gcd" {
        gcd
    } else if gcd.is_zero() {
        BigInt::zero()
    } else {
        a * b / gcd
    };
    Ok(Value::Rational(BigRational::from_integer(result)))
}

fn combinatoric_function(name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    require_arity(name, args, 2)?;
    let n = nonnegative_integer(&args[0])?;
    let r = nonnegative_integer(&args[1])?;
    if r > n {
        return Err(err(DiagnosticKind::Domain, "r must not exceed n.", 0..0));
    }
    let count = num_traits::ToPrimitive::to_u64(&r).ok_or_else(|| {
        err(
            DiagnosticKind::Limit,
            "Combinatoric argument is too large.",
            0..0,
        )
    })?;
    let mut result = BigInt::one();
    for index in 0..count {
        result *= &n - BigInt::from(index);
    }
    if name == "nCr" {
        result /= factorial_integer(&BigInt::from(count))?;
    }
    Ok(Value::Rational(BigRational::from_integer(result)))
}

fn require_arity(name: &str, args: &[Value], expected: usize) -> Result<(), Diagnostic> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(err(
            DiagnosticKind::Parse,
            format!("{name} expects {expected} arguments."),
            0..0,
        ))
    }
}

fn nonnegative_integer(value: &Value) -> Result<BigInt, Diagnostic> {
    let Value::Rational(value) = value else {
        return Err(err(
            DiagnosticKind::Domain,
            "Expected a non-negative integer.",
            0..0,
        ));
    };
    if !value.is_integer() || value.is_negative() {
        return Err(err(
            DiagnosticKind::Domain,
            "Expected a non-negative integer.",
            0..0,
        ));
    }
    Ok(value.to_integer())
}

fn factorial_integer(value: &BigInt) -> Result<BigInt, Diagnostic> {
    let count = num_traits::ToPrimitive::to_u64(value)
        .ok_or_else(|| err(DiagnosticKind::Limit, "Factorial exceeds 1024.", 0..0))?;
    if count > 1024 {
        return Err(err(DiagnosticKind::Limit, "Factorial exceeds 1024.", 0..0));
    }
    Ok((1..=count).fold(BigInt::one(), |result, index| result * BigInt::from(index)))
}

fn bigint_gcd(mut a: BigInt, mut b: BigInt) -> BigInt {
    while !b.is_zero() {
        let remainder = &a % &b;
        a = b;
        b = remainder;
    }
    a.abs()
}

fn min_max_function(name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(err(
            DiagnosticKind::Parse,
            "Expected at least two arguments.",
            0..0,
        ));
    }
    let Value::Rational(result) = &args[0] else {
        return Err(err(
            DiagnosticKind::Unsupported,
            "min and max require real scalars.",
            0..0,
        ));
    };
    let mut result = result.clone();
    for value in &args[1..] {
        let Value::Rational(value) = value else {
            return Err(err(
                DiagnosticKind::Unsupported,
                "min and max require real scalars.",
                0..0,
            ));
        };
        if (name == "min" && value < &result) || (name == "max" && value > &result) {
            result = value.clone();
        }
    }
    Ok(Value::Rational(result))
}

fn sqrt_value(value: &Value) -> Result<Value, Diagnostic> {
    let Value::Rational(r) = value else {
        return Err(err(
            DiagnosticKind::Unsupported,
            "Transcendental complex functions are unsupported.",
            0..0,
        ));
    };
    if r.is_integer() {
        let n = r.to_integer();
        if n.is_negative() {
            let root = isqrt(&(-n.clone()));
            if root.clone() * root.clone() == -n {
                return Ok(Value::Complex(Complex::new(
                    BigRational::zero(),
                    BigRational::from_integer(root),
                )));
            }
        } else {
            let root = isqrt(&n);
            if root.clone() * root.clone() == n {
                return Ok(Value::Rational(BigRational::from_integer(root)));
            }
        }
    }
    Ok(Value::Approx(
        rational_float(r)?.sqrt(DEFAULT_PRECISION_BITS, RoundingMode::ToEven),
    ))
}

fn transcendental(name: &str, args: &[Value]) -> Result<Value, Diagnostic> {
    if matches!(name, "ln" | "log" | "log10" | "log2")
        && matches!(args.first(), Some(Value::Rational(value)) if value <= &BigRational::zero())
    {
        return Err(err(
            DiagnosticKind::Domain,
            "Logarithm requires a positive value.",
            0..0,
        ));
    }
    if name == "log" && args.len() == 2 {
        let value = scalar_float(&args[0])?;
        let base = scalar_float(&args[1])?;
        let mut cc = Consts::new()
            .map_err(|_e| err(DiagnosticKind::Limit, "Constants unavailable.", 0..0))?;
        return Ok(Value::Approx(value.log(
            &base,
            DEFAULT_PRECISION_BITS,
            RoundingMode::ToEven,
            &mut cc,
        )));
    }
    require_arity(name, args, 1)?;
    let value = scalar_float(&args[0])?;
    let mut cc =
        Consts::new().map_err(|_e| err(DiagnosticKind::Limit, "Constants unavailable.", 0..0))?;
    let result = match name {
        "sin" => value.sin(DEFAULT_PRECISION_BITS, RoundingMode::ToEven, &mut cc),
        "cos" => value.cos(DEFAULT_PRECISION_BITS, RoundingMode::ToEven, &mut cc),
        "tan" => value.tan(DEFAULT_PRECISION_BITS, RoundingMode::ToEven, &mut cc),
        "ln" => value.ln(DEFAULT_PRECISION_BITS, RoundingMode::ToEven, &mut cc),
        "log" | "log10" => value.log10(DEFAULT_PRECISION_BITS, RoundingMode::ToEven, &mut cc),
        "log2" => value.log2(DEFAULT_PRECISION_BITS, RoundingMode::ToEven, &mut cc),
        "exp" => value.exp(DEFAULT_PRECISION_BITS, RoundingMode::ToEven, &mut cc),
        "cbrt" => value.cbrt(DEFAULT_PRECISION_BITS, RoundingMode::ToEven),
        _ => unreachable!(),
    };
    Ok(Value::Approx(result))
}

fn scalar_float(value: &Value) -> Result<BigFloat, Diagnostic> {
    match value {
        Value::Rational(value) => rational_float(value),
        Value::Approx(value) => Ok(value.clone()),
        _ => Err(err(
            DiagnosticKind::Unsupported,
            "Transcendental complex functions are unsupported.",
            0..0,
        )),
    }
}
fn rational_float(r: &BigRational) -> Result<BigFloat, Diagnostic> {
    let mut cc =
        Consts::new().map_err(|_e| err(DiagnosticKind::Limit, "Constants unavailable.", 0..0))?;
    let n = BigFloat::parse(
        &r.numer().to_string(),
        Radix::Dec,
        DEFAULT_PRECISION_BITS,
        RoundingMode::ToEven,
        &mut cc,
    );
    let d = BigFloat::parse(
        &r.denom().to_string(),
        Radix::Dec,
        DEFAULT_PRECISION_BITS,
        RoundingMode::ToEven,
        &mut cc,
    );
    Ok(n.div(&d, DEFAULT_PRECISION_BITS, RoundingMode::ToEven))
}
fn approx_const(name: &str) -> Result<Value, Diagnostic> {
    let cc =
        Consts::new().map_err(|_e| err(DiagnosticKind::Limit, "Constants unavailable.", 0..0))?;
    let mut ctx = astro_float::ctx::Context::new(
        DEFAULT_PRECISION_BITS,
        RoundingMode::ToEven,
        cc,
        -100_000,
        100_000,
    );
    let x = match name {
        "pi" => ctx.const_pi(),
        "e" => ctx.const_e(),
        "tau" => ctx.const_pi().mul(
            &BigFloat::from_u64(2, DEFAULT_PRECISION_BITS),
            DEFAULT_PRECISION_BITS,
            RoundingMode::ToEven,
        ),
        _ => {
            return Err(err(
                DiagnosticKind::UnknownIdentifier,
                "Unknown constant.",
                0..0,
            ));
        }
    };
    Ok(Value::Approx(x))
}
fn display(v: &Value) -> Result<(String, Option<String>), Diagnostic> {
    match v {
        Value::Rational(r) => {
            if r.is_integer() {
                Ok((r.to_integer().to_string(), None))
            } else {
                Ok((
                    format!("{}/{}", r.numer(), r.denom()),
                    Some(format!("≈ {}", format_float(&rational_float(r)?)?)),
                ))
            }
        }
        Value::Approx(x) => Ok((format!("≈ {}", format_float(x)?), None)),
        Value::Complex(c) => {
            if c.im.is_zero() {
                Ok((c.re.to_string(), None))
            } else if c.re.is_zero() {
                if c.im == BigRational::one() {
                    Ok(("i".to_owned(), None))
                } else {
                    Ok((format!("{}i", c.im), None))
                }
            } else if c.im.is_negative() {
                Ok((format!("{} - {}i", c.re, -c.im.clone()), None))
            } else {
                Ok((format!("{} + {}i", c.re, c.im), None))
            }
        }
        Value::Quantity(n, _, u, ..) => {
            let (p, a) = display(&Value::Rational(n.clone()))?;
            Ok((format!("{p} {u}"), a.map(|x| format!("{x} {u}"))))
        }
    }
}
fn format_float(x: &BigFloat) -> Result<String, Diagnostic> {
    let mut cc =
        Consts::new().map_err(|_e| err(DiagnosticKind::Limit, "Constants unavailable.", 0..0))?;
    let text = x
        .format(Radix::Dec, RoundingMode::ToEven, &mut cc)
        .map_err(|_e| err(DiagnosticKind::Domain, "Could not format result.", 0..0))?;
    Ok(scientific_to_decimal(&text))
}
fn scientific_to_decimal(s: &str) -> String {
    let (m, e) = if let Some((m, e)) = s.split_once("e+") {
        (m, e.parse().unwrap_or(0_i32))
    } else if let Some((m, e)) = s.split_once("e-") {
        (m, -e.parse::<i32>().unwrap_or(0))
    } else {
        (s, 0)
    };
    let negative = m.starts_with('-');
    let mut digits = m
        .trim_start_matches('-')
        .replace('.', "")
        .chars()
        .collect::<Vec<_>>();
    if digits.len() > 30 {
        let round = digits[30] >= '5';
        digits.truncate(30);
        if round {
            for i in (0..digits.len()).rev() {
                if digits[i] < '9' {
                    digits[i] = char::from_u32(digits[i] as u32 + 1).unwrap_or('9');
                    break;
                }
                digits[i] = '0';
                if i == 0 {
                    digits.insert(0, '1');
                }
            }
        }
    }
    let point =
        (i32::try_from(m.trim_start_matches('-').find('.').unwrap_or(1)).unwrap_or(i32::MAX) + e)
            .max(0) as usize;
    let mut result = if point >= digits.len() {
        format!(
            "{}{}",
            digits.iter().collect::<String>(),
            "0".repeat(point - digits.len())
        )
    } else if point == 0 {
        format!("0.{}", digits.iter().collect::<String>())
    } else {
        format!(
            "{}.{}",
            digits[..point].iter().collect::<String>(),
            digits[point..].iter().collect::<String>()
        )
    };
    if negative {
        result.insert(0, '-');
    }
    result
}
fn isqrt(n: &BigInt) -> BigInt {
    let n = n.to_biguint().unwrap_or_else(BigUint::zero);
    if n.is_zero() {
        return BigInt::zero();
    }
    let mut x = n.clone();
    let mut y: BigUint = (&x + BigUint::one()) >> 1;
    while y < x {
        x = y.clone();
        y = (&x + &n / &x) >> 1;
    }
    BigInt::from(x)
}
fn unit_names() -> Vec<&'static str> {
    vec![
        "m", "kg", "s", "A", "K", "mol", "cd", "g", "L", "min", "h", "in", "ft", "yd", "mi", "lb",
        "oz", "gal", "Hz", "N", "Pa", "J", "W", "C", "V", "Ohm", "F", "B", "bit", "kmh", "mph",
        "rad", "degC", "degF",
    ]
}
fn unit_def(name: &str) -> Option<([i8; 8], BigRational, bool)> {
    let one = BigRational::one();
    let unit = match name {
        "m" => ([1, 0, 0, 0, 0, 0, 0, 0], one, false),
        "kg" => ([0, 1, 0, 0, 0, 0, 0, 0], one, false),
        "s" => ([0, 0, 1, 0, 0, 0, 0, 0], one, false),
        "A" => ([0, 0, 0, 1, 0, 0, 0, 0], one, false),
        "K" => ([0, 0, 0, 0, 1, 0, 0, 0], one, false),
        "mol" => ([0, 0, 0, 0, 0, 1, 0, 0], one, false),
        "cd" => ([0, 0, 0, 0, 0, 0, 1, 0], one, false),
        "g" => (
            [0, 1, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::one(), BigInt::from(1000)),
            false,
        ),
        "L" => (
            [3, 0, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::one(), BigInt::from(1000)),
            false,
        ),
        "min" => (
            [0, 0, 1, 0, 0, 0, 0, 0],
            BigRational::from_integer(BigInt::from(60)),
            false,
        ),
        "h" => (
            [0, 0, 1, 0, 0, 0, 0, 0],
            BigRational::from_integer(BigInt::from(3600)),
            false,
        ),
        "in" => (
            [1, 0, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(127), BigInt::from(5000)),
            false,
        ),
        "ft" => (
            [1, 0, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(381), BigInt::from(1250)),
            false,
        ),
        "yd" => (
            [1, 0, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(1143), BigInt::from(1250)),
            false,
        ),
        "mi" => (
            [1, 0, 0, 0, 0, 0, 0, 0],
            BigRational::from_integer(BigInt::from(1609344))
                / BigRational::from_integer(BigInt::from(1000)),
            false,
        ),
        "lb" => (
            [0, 1, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(45359237), BigInt::from(100000000)),
            false,
        ),
        "oz" => (
            [0, 1, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(45359237), BigInt::from(1600000000)),
            false,
        ),
        "gal" => (
            [3, 0, 0, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(473176473), BigInt::from(125000000000_i64)),
            false,
        ),
        "Hz" => ([0, 0, -1, 0, 0, 0, 0, 0], one, false),
        "N" => ([1, 1, -2, 0, 0, 0, 0, 0], one, false),
        "Pa" => ([-1, 1, -2, 0, 0, 0, 0, 0], one, false),
        "J" => ([2, 1, -2, 0, 0, 0, 0, 0], one, false),
        "W" => ([2, 1, -3, 0, 0, 0, 0, 0], one, false),
        "C" => ([0, 0, 1, 1, 0, 0, 0, 0], one, false),
        "V" => ([2, 1, -3, -1, 0, 0, 0, 0], one, false),
        "Ohm" => ([2, 1, -3, -2, 0, 0, 0, 0], one, false),
        "F" => ([-2, -1, 4, 2, 0, 0, 0, 0], one, false),
        "B" | "bit" => ([0, 0, 0, 0, 0, 0, 0, 0], one, false),
        "kmh" => (
            [1, 0, -1, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(5), BigInt::from(18)),
            false,
        ),
        "mph" => (
            [1, 0, -1, 0, 0, 0, 0, 0],
            BigRational::new(BigInt::from(1609344), BigInt::from(3600000)),
            false,
        ),
        "rad" => ([0, 0, 0, 0, 0, 0, 0, 1], one, false),
        "degC" => ([0, 0, 0, 0, 1, 0, 0, 0], one, true),
        "degF" => (
            [0, 0, 0, 0, 1, 0, 0, 0],
            BigRational::new(BigInt::from(5), BigInt::from(9)),
            true,
        ),
        _ => return prefixed_unit(name),
    };
    Some(unit)
}

fn prefixed_unit(name: &str) -> Option<([i8; 8], BigRational, bool)> {
    let prefixes = [
        ("da", 10_i32),
        ("Y", 24),
        ("Z", 21),
        ("E", 18),
        ("P", 15),
        ("T", 12),
        ("G", 9),
        ("M", 6),
        ("k", 3),
        ("h", 2),
        ("d", -1),
        ("c", -2),
        ("m", -3),
        ("u", -6),
        ("n", -9),
        ("p", -12),
        ("f", -15),
        ("a", -18),
        ("z", -21),
        ("y", -24),
        ("Ki", 10),
        ("Mi", 20),
        ("Gi", 30),
        ("Ti", 40),
    ];
    for (prefix, exponent) in prefixes {
        if let Some(base) = name.strip_prefix(prefix) {
            let (dim, scale, affine) = unit_def(base)?;
            if affine {
                return None;
            }
            let factor = if prefix.ends_with('i') {
                BigRational::from_integer(BigInt::from(2).pow(exponent as u32))
            } else if exponent >= 0 {
                BigRational::from_integer(BigInt::from(10).pow(exponent as u32))
            } else {
                BigRational::new(BigInt::one(), BigInt::from(10).pow((-exponent) as u32))
            };
            return Some((dim, scale * factor, false));
        }
    }
    None
}

fn unit_expression(source: &str) -> Option<([i8; 8], BigRational, bool)> {
    let mut dim = [0_i8; 8];
    let mut scale = BigRational::one();
    let mut divide = false;
    let mut affine_seen = false;
    for symbol in source.split_inclusive(['*', '/']) {
        let (symbol, operator) = symbol.strip_suffix('*').map_or_else(
            || {
                symbol
                    .strip_suffix('/')
                    .map_or((symbol, None), |s| (s, Some('/')))
            },
            |s| (s, Some('*')),
        );
        let (name, exponent) = if let Some((name, exponent)) = symbol.trim().split_once('^') {
            (name.trim(), exponent.trim().parse().ok()?)
        } else {
            (symbol.trim(), 1_i8)
        };
        if !(-12..=12).contains(&exponent) {
            return None;
        }
        let (part_dim, part_scale, affine) = unit_def(name)?;
        if affine && (source.trim().len() != name.len() || exponent != 1) {
            return None;
        }
        affine_seen |= affine;
        let signed_exponent = if divide { -exponent } else { exponent };
        for (index, value) in part_dim.iter().enumerate() {
            dim[index] += value.checked_mul(signed_exponent)?;
            if !(-12..=12).contains(&dim[index]) {
                return None;
            }
        }
        scale = if signed_exponent >= 0 {
            scale * part_scale.pow(i32::from(signed_exponent))
        } else {
            scale / part_scale.pow(i32::from(-signed_exponent))
        };
        divide = operator == Some('/');
    }
    Some((dim, scale, affine_seen))
}
fn convert(v: Value, target: &str) -> Result<Value, Diagnostic> {
    let Some((d, scale, affine)) = unit_expression(target) else {
        return Err(err(
            DiagnosticKind::UnknownIdentifier,
            "Unknown unit.",
            0..0,
        ));
    };
    let Value::Quantity(n, sd, source, ss, sa) = v else {
        return Err(err(
            DiagnosticKind::Dimension,
            "Only quantities can be converted.",
            0..0,
        ));
    };
    if d != sd {
        return Err(err(
            DiagnosticKind::Dimension,
            "Incompatible dimensions.",
            0..0,
        ));
    }
    if sa || affine {
        let kelvin = match source.as_str() {
            "degC" => n + BigRational::new(BigInt::from(27315), BigInt::from(100)),
            "degF" => {
                n * BigRational::new(BigInt::from(5), BigInt::from(9))
                    + BigRational::new(BigInt::from(45967), BigInt::from(180))
            }
            "K" => n,
            _ => {
                return Err(err(
                    DiagnosticKind::Domain,
                    "Affine temperature units can only be converted.",
                    0..0,
                ));
            }
        };
        let result = match target {
            "degC" => kelvin - BigRational::new(BigInt::from(27315), BigInt::from(100)),
            "degF" => {
                (kelvin - BigRational::new(BigInt::from(45967), BigInt::from(180)))
                    / BigRational::new(BigInt::from(5), BigInt::from(9))
            }
            "K" => kelvin,
            _ => {
                return Err(err(
                    DiagnosticKind::Domain,
                    "Affine temperature units can only be converted.",
                    0..0,
                ));
            }
        };
        return Ok(Value::Quantity(result, d, target.to_owned(), scale, true));
    }
    Ok(Value::Quantity(
        n * ss / scale.clone(),
        d,
        target.to_owned(),
        scale,
        false,
    ))
}
fn err(kind: DiagnosticKind, message: impl Into<String>, span: Range<usize>) -> Diagnostic {
    Diagnostic {
        kind,
        message: message.into(),
        span,
    }
}
fn valid_name(s: &str) -> bool {
    s.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && s.bytes().all(|x| x.is_ascii_alphanumeric() || x == b'_')
}
fn reserved(s: &str) -> bool {
    unit_names().contains(&s)
        || [
            "pi", "e", "tau", "i", "sqrt", "sin", "cos", "tan", "ln", "log",
        ]
        .contains(&s)
}
fn strip_outer(mut s: &str) -> &str {
    while s.starts_with('(') && s.ends_with(')') {
        let mut n = 0;
        let mut closes = false;
        for (i, c) in s.char_indices() {
            if c == '(' {
                n += 1;
            } else if c == ')' {
                n -= 1;
                if n == 0 {
                    closes = i + 1 == s.len();
                    break;
                }
            }
        }
        if closes {
            s = &s[1..s.len() - 1];
        } else {
            break;
        }
    }
    s.trim()
}
fn split_power(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '^' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}
fn negate(value: Value) -> Result<Value, Diagnostic> {
    match value {
        Value::Rational(x) => Ok(Value::Rational(-x)),
        Value::Complex(x) => Ok(Value::Complex(-x)),
        _ => Err(err(
            DiagnosticKind::Unsupported,
            "Unary minus requires a scalar.",
            0..0,
        )),
    }
}
fn implicit_groups(s: &str) -> Option<(&str, &str)> {
    if !s.starts_with('(') {
        return None;
    }
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i + 1 < s.len() {
                    return Some((&s[..=i], &s[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}
fn split_operator<'a>(s: &'a str, ops: &[char]) -> Option<(&'a str, &'a str, char)> {
    let mut depth = 0;
    for (i, c) in s.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => depth -= 1,
            _ => {}
        }
        if depth == 0 && ops.contains(&c) && i > 0 {
            return Some((&s[..i], &s[i + 1..], c));
        }
    }
    None
}
fn call(s: &str) -> Option<(&str, &str)> {
    let (name, args) = s.split_once('(')?;
    args.strip_suffix(')').map(|x| (name.trim(), x))
}
fn parse_rational(s: &str) -> Result<Value, Diagnostic> {
    let s = s.replace('_', "");
    if s.len() > 1024 {
        return Err(err(
            DiagnosticKind::Limit,
            "Literal exceeds 1024 digits.",
            0..s.len(),
        ));
    }
    let n = if let Some(x) = s.strip_prefix("0x") {
        BigInt::parse_bytes(x.as_bytes(), 16)
    } else if let Some(x) = s.strip_prefix("0b") {
        BigInt::parse_bytes(x.as_bytes(), 2)
    } else if let Some(x) = s.strip_prefix("0o") {
        BigInt::parse_bytes(x.as_bytes(), 8)
    } else {
        None
    };
    if let Some(n) = n {
        return Ok(Value::Rational(BigRational::from_integer(n)));
    }
    let (mantissa, exp) = s
        .split_once(['e', 'E'])
        .map_or((s.as_str(), 0), |(a, b)| (a, b.parse().unwrap_or(0)));
    let (whole, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{whole}{frac}");
    let n = BigInt::parse_bytes(digits.as_bytes(), 10)
        .ok_or_else(|| err(DiagnosticKind::Lex, "Invalid number.", 0..s.len()))?;
    let p = exp - i32::try_from(frac.len()).unwrap_or(i32::MAX);
    let ten = BigInt::from(10);
    Ok(Value::Rational(if p >= 0 {
        BigRational::from_integer(n * ten.pow(p as u32))
    } else {
        BigRational::new(n, ten.pow((-p) as u32))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(input: &str) -> String {
        Calculator::new()
            .evaluate(input)
            .expect("expression must evaluate")
            .primary
    }

    #[test]
    fn precedence_and_exact_values() {
        assert_eq!(value("1 + 3 * 4"), "13");
        assert_eq!(value("2^3^2"), "512");
        assert_eq!(value("0xFF + 0b1"), "256");
        assert_eq!(value("2^100"), "1267650600228229401496703205376");
        assert_eq!(value("sqrt(-4)"), "2i");
    }
    #[test]
    fn implicit_multiplication_precision_and_units() {
        assert_eq!(value("2pi"), "≈ 6.28318530717958647692528676656");
        assert_eq!(value("(1+2)(3+4)"), "21");
        let third = Calculator::new().evaluate("1/3").expect("fraction");
        assert_eq!(third.primary, "1/3");
        assert_eq!(
            third.approximation.as_deref(),
            Some("≈ 0.333333333333333333333333333333")
        );
        assert_eq!(value("5 m/s to km/h"), "18 km/h");
        assert_eq!(value("i*i"), "-1");
    }

    #[test]
    fn affine_temperature_conversion() {
        assert_eq!(value("20 degC to degF"), "68 degF");
        assert!(Calculator::new().evaluate("20 degC + 1 degC").is_err());
    }

    #[test]
    fn quantity_arithmetic_and_zero_denominators() {
        assert_eq!(value("2 m * 3 m"), "6 m²");
        assert_eq!(value("10 N / 5 Pa"), "2 m²");
        assert_eq!(value("1 / 2 s"), "1/2 1/s");
        assert!(Calculator::new().evaluate("5 m / 0 s").is_err());
    }

    #[test]
    fn completion_matches_supported_functions() {
        let calculator = Calculator::new();
        let completions = calculator.complete("si", 2);
        assert!(
            completions
                .iter()
                .any(|completion| completion.insert == "sin")
        );
        assert!(
            !completions
                .iter()
                .any(|completion| completion.insert == "sinh")
        );
        assert_eq!(value("sin(0)"), "≈ 0.0");
    }

    #[test]
    fn prefixed_and_powered_units() {
        assert_eq!(value("2 km to m"), "2000 m");
        assert_eq!(value("1 m/s^2 to m/s^2"), "1 m/s^2");
        assert_eq!(value("1 ft to in"), "12 in");
    }

    #[test]
    fn integer_functions_and_arities() {
        assert_eq!(value("gcd(63,27)"), "9");
        assert_eq!(value("lcm(6,8)"), "24");
        assert_eq!(value("nCr(5,2)"), "10");
        assert_eq!(value("nPr(5,2)"), "20");
        assert_eq!(value("min(3,1,2)"), "1");
        assert_eq!(value("max(3,1,2)"), "3");
        assert!(Calculator::new().evaluate("gcd(2)").is_err());
    }

    #[test]
    fn constants_flow_into_transcendental_functions() {
        let result = Calculator::new()
            .evaluate("sin(pi/2)")
            .expect("sine must evaluate");
        assert!(result.primary.starts_with("≈ 1"));
        assert!(Calculator::new().evaluate("ln(-1)").is_err());
    }

    #[test]
    fn snapshots_restore_in_order_and_discard_invalid_definitions() {
        let mut calculator = Calculator::new();
        calculator
            .evaluate("x = 2")
            .expect("assignment must evaluate");
        calculator
            .evaluate("y = x + 3")
            .expect("dependent assignment must evaluate");
        let snapshot = calculator.snapshot();
        let mut restored = Calculator::new();
        let report = restored.restore(&snapshot);
        assert_eq!(report.restored, 2);
        assert_eq!(
            restored
                .evaluate("y^2")
                .expect("restored variable must evaluate")
                .primary,
            "25"
        );

        let mut invalid = snapshot.clone();
        invalid.definitions.push(DefinitionSnapshot {
            name: "bad".to_owned(),
            source: "bad = (".to_owned(),
        });
        let report = restored.restore(&invalid);
        assert_eq!(report.restored, 2);
        assert_eq!(report.discarded.len(), 1);
    }

    #[test]
    fn input_limit_is_reported_before_evaluation() {
        let input = "1".repeat(4097);
        let error = Calculator::new()
            .evaluate(&input)
            .expect_err("oversized input must fail");
        assert_eq!(error.kind, DiagnosticKind::Limit);
    }
}
