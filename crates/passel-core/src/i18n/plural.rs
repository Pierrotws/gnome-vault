//! The `plural=` expression of a catalog's `Plural-Forms` header: C
//! syntax over one variable, `n`, giving the index of the plural form.
//!
//! ```text
//! expr    := or ('?' expr ':' expr)?
//! or      := and ('||' and)*
//! and     := eq ('&&' eq)*
//! eq      := rel (('==' | '!=') rel)*
//! rel     := add (('<' | '>' | '<=' | '>=') add)*
//! add     := mul (('+' | '-') mul)*
//! mul     := unary (('*' | '/' | '%') unary)*
//! unary   := '!' unary | primary
//! primary := 'n' | number | '(' expr ')'
//! ```

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Expr {
    N,
    Number(u64),
    Not(Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// Deeper nesting than any real plural rule; bounds the recursion on a
/// malicious catalog.
const MAX_DEPTH: usize = 64;

impl Expr {
    pub(super) fn parse(src: &str) -> Option<Expr> {
        let mut parser = Parser {
            src: src.as_bytes(),
            pos: 0,
            depth: 0,
        };
        let expr = parser.ternary()?;
        parser.skip_space();
        (parser.pos == parser.src.len()).then_some(expr)
    }

    pub(super) fn eval(&self, n: u64) -> u64 {
        match self {
            Expr::N => n,
            Expr::Number(v) => *v,
            Expr::Not(e) => u64::from(e.eval(n) == 0),
            Expr::Ternary(c, a, b) => {
                if c.eval(n) != 0 {
                    a.eval(n)
                } else {
                    b.eval(n)
                }
            }
            Expr::Binary(op, a, b) => {
                let a = a.eval(n);
                // Short-circuit like C.
                match op {
                    Op::Or if a != 0 => return 1,
                    Op::And if a == 0 => return 0,
                    _ => {}
                }
                let b = b.eval(n);
                match op {
                    Op::Or | Op::And => u64::from(b != 0),
                    Op::Eq => u64::from(a == b),
                    Op::Ne => u64::from(a != b),
                    Op::Lt => u64::from(a < b),
                    Op::Gt => u64::from(a > b),
                    Op::Le => u64::from(a <= b),
                    Op::Ge => u64::from(a >= b),
                    Op::Add => a.wrapping_add(b),
                    Op::Sub => a.wrapping_sub(b),
                    Op::Mul => a.wrapping_mul(b),
                    Op::Div => a.checked_div(b).unwrap_or(0),
                    Op::Rem => a.checked_rem(b).unwrap_or(0),
                }
            }
        }
    }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        while self.src.get(self.pos).is_some_and(u8::is_ascii_whitespace) {
            self.pos += 1;
        }
    }

    /// Consumes `token` if it comes next (and, for `<`/`>`/`!`, is not
    /// the start of `<=`/`>=`/`!=`).
    fn eat(&mut self, token: &str) -> bool {
        self.skip_space();
        let rest = &self.src[self.pos..];
        if !rest.starts_with(token.as_bytes()) {
            return false;
        }
        if token.len() == 1 && matches!(token, "<" | ">" | "!") && rest.get(1) == Some(&b'=') {
            return false;
        }
        self.pos += token.len();
        true
    }

    fn ternary(&mut self) -> Option<Expr> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return None;
        }
        let cond = self.binary(0)?;
        let expr = if self.eat("?") {
            let a = self.ternary()?;
            if !self.eat(":") {
                return None;
            }
            let b = self.ternary()?;
            Expr::Ternary(Box::new(cond), Box::new(a), Box::new(b))
        } else {
            cond
        };
        self.depth -= 1;
        Some(expr)
    }

    /// One precedence level per entry, loosest first.
    const LEVELS: &'static [&'static [(&'static str, Op)]] = &[
        &[("||", Op::Or)],
        &[("&&", Op::And)],
        &[("==", Op::Eq), ("!=", Op::Ne)],
        &[("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)],
        &[("+", Op::Add), ("-", Op::Sub)],
        &[("*", Op::Mul), ("/", Op::Div), ("%", Op::Rem)],
    ];

    fn binary(&mut self, level: usize) -> Option<Expr> {
        let Some(ops) = Self::LEVELS.get(level) else {
            return self.unary();
        };
        let mut lhs = self.binary(level + 1)?;
        'next: loop {
            for (token, op) in *ops {
                if self.eat(token) {
                    let rhs = self.binary(level + 1)?;
                    lhs = Expr::Binary(*op, Box::new(lhs), Box::new(rhs));
                    continue 'next;
                }
            }
            return Some(lhs);
        }
    }

    fn unary(&mut self) -> Option<Expr> {
        if self.eat("!") {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return None;
            }
            let inner = self.unary()?;
            self.depth -= 1;
            return Some(Expr::Not(Box::new(inner)));
        }
        if self.eat("(") {
            let inner = self.ternary()?;
            return self.eat(")").then_some(inner);
        }
        if self.eat("n") {
            return Some(Expr::N);
        }
        self.skip_space();
        let start = self.pos;
        while self.src.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .ok()?
            .parse()
            .ok()
            .map(Expr::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(src: &str, ns: &[u64]) -> Vec<u64> {
        let expr = Expr::parse(src).expect(src);
        ns.iter().map(|&n| expr.eval(n)).collect()
    }

    #[test]
    fn french_and_english() {
        assert_eq!(forms("(n > 1)", &[0, 1, 2, 5]), [0, 0, 1, 1]);
        assert_eq!(forms("n != 1", &[0, 1, 2]), [1, 0, 1]);
    }

    #[test]
    fn slavic_rules_with_ternaries() {
        // Russian.
        let ru =
            "(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2)";
        assert_eq!(
            forms(ru, &[1, 2, 5, 11, 12, 21, 22, 25, 111]),
            [0, 1, 2, 2, 2, 0, 1, 2, 2]
        );
        // Arabic, six forms.
        let ar = "n==0 ? 0 : n==1 ? 1 : n==2 ? 2 : n%100>=3 && n%100<=10 ? 3 : n%100>=11 ? 4 : 5";
        assert_eq!(forms(ar, &[0, 1, 2, 3, 11, 100]), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn rejects_garbage() {
        for src in ["", "n >", "(n", "n ? 1", "m", "n 1", &"(".repeat(1000)] {
            assert_eq!(Expr::parse(src), None, "{src}");
        }
        assert_eq!(forms("!n", &[0, 3]), [1, 0]);
        assert_eq!(forms("n / 0 + n % 0", &[7]), [0]);
    }
}
