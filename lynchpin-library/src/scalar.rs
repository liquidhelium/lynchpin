//! Terminal scalar: an integer quantity with optional infinity.
//!
//! Mirrors typst's `Scalar(f64)` / `Abs(Scalar)`, but for terminal grid
//! coordinates.  Internally stored as `f64` so that IEEE 754 infinity
//! propagates correctly through all arithmetic operators without any
//! manual checks.
//!
//! The public API enforces integer semantics: construction from `i32`,
//! extraction as `i32`.  The `f64` internals are an implementation detail.

use std::cmp::Ordering;
use std::fmt::{self, Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::iter::Sum;
use std::ops::{
    Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign,
};

/// A terminal scalar value — a signed integer or infinity.
///
/// This is the terminal equivalent of `Abs` (which wraps `Scalar(f64)`).
/// It is used for both **coordinates** (column / row indices) and
/// **constraints** (region sizes that may be unbounded).
///
/// # Infinity
///
/// `TermScalar::INFINITY` represents "unbounded".  It behaves like
/// mathematical infinity under all arithmetic operations:
///
/// ```ignore
/// INFINITY + 5  == INFINITY
/// INFINITY - 5  == INFINITY
/// min(5, INFINITY) == 5
/// max(5, INFINITY) == INFINITY
/// ```
///
/// Use [`is_finite`](Self::is_finite) to check.
#[derive(Default, Copy, Clone)]
pub struct TermScalar(f64);

// ── Constants ────────────────────────────────────────────────────────────────

impl TermScalar {
    /// The additive identity.
    pub const ZERO: Self = Self(0.0);

    /// The smallest positive value (1 terminal column / row).
    pub const ONE: Self = Self(1.0);

    /// Positive infinity — represents "unbounded".
    pub const INFINITY: Self = Self(f64::INFINITY);

    /// Negative infinity.  Rarely used; mainly for `Neg` symmetry.
    pub const NEG_INFINITY: Self = Self(f64::NEG_INFINITY);
}

// ── Construction / extraction ────────────────────────────────────────────────

impl TermScalar {
    /// Create a finite scalar from an `i32`.
    #[inline]
    pub const fn new(n: i32) -> Self {
        Self(n as f64)
    }

    /// Create a scalar from a raw `f64`.  Prefer [`new`](Self::new) for
    /// integer values; this is available for unit-conversion intermediates.
    #[inline]
    pub fn from_f64(x: f64) -> Self {
        if x.is_nan() {
            Self(0.0)
        } else {
            Self(x)
        }
    }

    /// Extract the value as `i32`.  Saturates at `i32::MAX` / `i32::MIN`
    /// for non-finite inputs.
    #[inline]
    pub fn get(self) -> i32 {
        if self.0.is_infinite() {
            if self.0.is_sign_positive() {
                i32::MAX
            } else {
                i32::MIN
            }
        } else {
            self.0 as i32
        }
    }

    /// Round to the nearest integer, away from zero for half-way cases.
    #[inline]
    pub fn round(self) -> Self {
        if self.0.is_finite() {
            Self(self.0.round())
        } else {
            self
        }
    }

    /// Round toward positive infinity.
    #[inline]
    pub fn ceil(self) -> Self {
        if self.0.is_finite() {
            Self(self.0.ceil())
        } else {
            self
        }
    }

    /// Round toward negative infinity.
    #[inline]
    pub fn floor(self) -> Self {
        if self.0.is_finite() {
            Self(self.0.floor())
        } else {
            self
        }
    }

    /// Absolute value.
    #[inline]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

// ── Predicates ───────────────────────────────────────────────────────────────

impl TermScalar {
    /// Whether this is a finite value (not infinity).
    ///
    /// This is the terminal equivalent of `Abs::is_finite()`.
    #[inline]
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    /// Whether this is exactly zero.
    #[inline]
    pub fn is_zero(self) -> bool {
        self.0 == 0.0
    }

    /// Whether this value is positive (> 0).
    #[inline]
    pub fn is_positive(self) -> bool {
        self.0 > 0.0
    }

    /// Whether this value is negative (< 0).
    #[inline]
    pub fn is_negative(self) -> bool {
        self.0 < 0.0
    }

    /// Whether `self` approximately equals `other` within `eps`.
    #[inline]
    pub fn approx_eq(self, other: Self, eps: f64) -> bool {
        (self.0 - other.0).abs() < eps
    }

    /// Signum: -1, 0, or 1.
    #[inline]
    pub fn signum(self) -> f64 {
        self.0.signum()
    }
}

// ── min / max ────────────────────────────────────────────────────────────────

impl TermScalar {
    /// Element-wise minimum.
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// Element-wise maximum.
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    /// Clamp to `[lo, hi]`.
    #[inline]
    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        Self(self.0.clamp(lo.0, hi.0))
    }
}

// ── Eq / Ord / Hash ─────────────────────────────────────────────────────────

// Panic on NaN — consistent with typst's `Scalar`.
impl Eq for TermScalar {}

impl PartialEq for TermScalar {
    fn eq(&self, other: &Self) -> bool {
        assert!(!self.0.is_nan() && !other.0.is_nan(), "TermScalar is NaN");
        self.0 == other.0
    }
}

impl Ord for TermScalar {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .partial_cmp(&other.0)
            .expect("TermScalar is NaN")
    }
}

impl PartialOrd for TermScalar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for TermScalar {
    fn hash<H: Hasher>(&self, state: &mut H) {
        debug_assert!(!self.0.is_nan(), "TermScalar is NaN");
        self.0.to_bits().hash(state);
    }
}

// ── From conversions ─────────────────────────────────────────────────────────

impl From<i32> for TermScalar {
    fn from(n: i32) -> Self {
        Self::new(n)
    }
}

impl From<TermScalar> for i32 {
    fn from(s: TermScalar) -> Self {
        s.get()
    }
}

// ── Arithmetic operators ─────────────────────────────────────────────────────

impl Neg for TermScalar {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Add for TermScalar {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for TermScalar {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for TermScalar {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for TermScalar {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul for TermScalar {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl MulAssign for TermScalar {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Div for TermScalar {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        Self(self.0 / rhs.0)
    }
}

impl DivAssign for TermScalar {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl Rem for TermScalar {
    type Output = Self;
    fn rem(self, rhs: Self) -> Self {
        Self(self.0 % rhs.0)
    }
}

impl RemAssign for TermScalar {
    fn rem_assign(&mut self, rhs: Self) {
        *self = *self % rhs;
    }
}

// ── Scalar * f64 / f64 * Scalar (for unit conversion) ────────────────────────

impl Mul<f64> for TermScalar {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self(self.0 * rhs)
    }
}

impl Mul<TermScalar> for f64 {
    type Output = TermScalar;
    fn mul(self, rhs: TermScalar) -> TermScalar {
        TermScalar(self * rhs.0)
    }
}

impl Div<f64> for TermScalar {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self(self.0 / rhs)
    }
}

// ── Sum ──────────────────────────────────────────────────────────────────────

impl Sum for TermScalar {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|s| s.0).sum())
    }
}

impl<'a> Sum<&'a Self> for TermScalar {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        Self(iter.map(|s| s.0).sum())
    }
}

// ── Debug / Display ──────────────────────────────────────────────────────────

impl Debug for TermScalar {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.0.is_infinite() {
            if self.0.is_sign_positive() {
                write!(f, "∞")
            } else {
                write!(f, "-∞")
            }
        } else {
            write!(f, "{}", self.0 as i32)
        }
    }
}
