//! Format templates: `{yyyy}`, `{yy}`, `{mm}`, and `{0000}`-style padding.

use crate::error::{Error, Result};

/// One parsed piece of a numbering format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Literal(String),
    Year4,
    Year2,
    Month,
    Pad(usize),
}

/// Parse `fmt` into tokens. Unknown `{...}` forms are [`Error::InvalidTemplate`].
pub(crate) fn parse_template(fmt: &str) -> Result<Vec<Token>> {
    if fmt.is_empty() {
        return Err(Error::InvalidTemplate("empty format".into()));
    }
    let mut out = Vec::new();
    let mut rest = fmt;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("{yyyy}") {
            out.push(Token::Year4);
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("{yy}") {
            out.push(Token::Year2);
            rest = stripped;
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("{mm}") {
            out.push(Token::Month);
            rest = stripped;
            continue;
        }
        if rest.starts_with('{') {
            let Some(end) = rest.find('}') else {
                return Err(Error::InvalidTemplate(format!("unclosed token in {fmt:?}")));
            };
            let inner = &rest[1..end];
            if inner.is_empty() || !inner.bytes().all(|b| b == b'0') {
                return Err(Error::InvalidTemplate(format!(
                    "unknown token {{{inner}}} in {fmt:?}"
                )));
            }
            out.push(Token::Pad(inner.len()));
            rest = &rest[end + 1..];
            continue;
        }
        let next = rest.find('{').unwrap_or(rest.len());
        out.push(Token::Literal(rest[..next].to_string()));
        rest = &rest[next..];
    }
    Ok(out)
}

/// Render `fmt` with `allocated` and a UTC calendar stamp. Never truncates padding.
pub(crate) fn render(fmt: &str, allocated: i64, year: i32, month: u32) -> Result<String> {
    if !(1..=12).contains(&month) {
        return Err(Error::InvalidTemplate(format!(
            "month {month} is not 1..=12"
        )));
    }
    let tokens = parse_template(fmt)?;
    let mut s = String::new();
    for token in tokens {
        match token {
            Token::Literal(lit) => s.push_str(&lit),
            Token::Year4 => s.push_str(&format!("{year:04}")),
            Token::Year2 => s.push_str(&format!("{:02}", year.rem_euclid(100))),
            Token::Month => s.push_str(&format!("{month:02}")),
            Token::Pad(width) => {
                if allocated < 0 {
                    return Err(Error::PaddingOverflow { allocated, width });
                }
                let digits = allocated.to_string();
                if digits.len() > width {
                    return Err(Error::PaddingOverflow { allocated, width });
                }
                s.push_str(&format!("{allocated:0width$}"));
            }
        }
    }
    Ok(s)
}

/// Expanded length if every pad token uses its full width.
pub(crate) fn max_len(fmt: &str) -> Result<usize> {
    let tokens = parse_template(fmt)?;
    Ok(tokens
        .iter()
        .map(|t| match t {
            Token::Literal(s) => s.len(),
            Token::Year4 => 4,
            Token::Year2 => 2,
            Token::Month => 2,
            Token::Pad(w) => *w,
        })
        .sum())
}

/// Literal characters of `fmt` (kernel lot/serial charset check).
pub(crate) fn literals(fmt: &str) -> Result<String> {
    let tokens = parse_template(fmt)?;
    Ok(tokens
        .into_iter()
        .filter_map(|t| match t {
            Token::Literal(s) => Some(s),
            _ => None,
        })
        .collect())
}

/// Infer reset policy from which calendar tokens the template uses.
pub(crate) fn reset_from_template(fmt: &str) -> Result<crate::ResetPolicy> {
    let tokens = parse_template(fmt)?;
    let mut month = false;
    let mut year = false;
    for t in tokens {
        match t {
            Token::Month => month = true,
            Token::Year4 | Token::Year2 => year = true,
            Token::Literal(_) | Token::Pad(_) => {}
        }
    }
    Ok(if month {
        crate::ResetPolicy::Monthly
    } else if year {
        crate::ResetPolicy::Yearly
    } else {
        crate::ResetPolicy::Never
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_templates_render_exactly() {
        type Case = (
            &'static str,
            i64,
            i32,
            u32,
            core::result::Result<&'static str, (i64, usize)>,
        );
        let cases: &[Case] = &[
            ("WO-{yyyy}-{0000}", 1, 2026, 9, Ok("WO-2026-0001")),
            ("WO-{yyyy}-{0000}", 416, 2026, 1, Ok("WO-2026-0416")),
            ("INV-{00000}", 7, 2026, 1, Ok("INV-00007")),
            ("PO-{0000}", 841, 2024, 3, Ok("PO-0841")),
            ("{yy}{mm}-{00}", 4, 2026, 9, Ok("2609-04")),
            ("{0}", 9, 2026, 1, Ok("9")),
            ("{0}", 10, 2026, 1, Err((10, 1))),
            ("{0000}", 10000, 2026, 1, Err((10000, 4))),
            ("LIT", 1, 2026, 1, Ok("LIT")),
        ];
        for (fmt, allocated, year, month, expected) in cases {
            let got = render(fmt, *allocated, *year, *month);
            match expected {
                Ok(want) => assert_eq!(got.as_deref().expect(fmt), *want, "fmt={fmt}"),
                Err((n, w)) => match got {
                    Err(Error::PaddingOverflow { allocated, width }) => {
                        assert_eq!(allocated, *n, "fmt={fmt}");
                        assert_eq!(width, *w, "fmt={fmt}");
                    }
                    other => panic!("fmt={fmt}: expected padding overflow, got {other:?}"),
                },
            }
        }
        assert!(matches!(
            render("{bogus}", 1, 2026, 1),
            Err(Error::InvalidTemplate(_))
        ));
        assert!(matches!(
            render("{yyyy", 1, 2026, 1),
            Err(Error::InvalidTemplate(_))
        ));
    }
}
