//! Atomic composite parser (LexParseAst §4).
//!
//! A primary `SymbolId` plus up to four mod slots, recursively. Mods are
//! `^arg` / `_arg` where the argument is itself an atomic composite.

use qubitick_model::{AtomicComposite, Mods};

use crate::lexer::{LexError, Span, Token};

/// Parse an atomic composite starting at `pos`, returning the composite and the
/// next token position.
pub fn parse_atomic(tokens: &[Token], pos: usize) -> Result<(AtomicComposite, usize), LexError> {
    let (primary, pos) = parse_primary(tokens, pos)?;
    let mut mods = Mods::default();

    let mut pos = pos;
    while let Some(tok) = tokens.get(pos) {
        match tok {
            Token::Superscript(_) => {
                pos += 1;
                let (arg, next) = parse_atom_arg(tokens, pos)?;
                pos = next;
                if mods.r_sup.is_none() && mods.r_sub.is_none() && mods.l_sup.is_none() {
                    mods.r_sup = Some(Box::new(arg));
                } else {
                    mods.l_sup = Some(Box::new(arg));
                }
            }
            Token::Subscript(_) => {
                pos += 1;
                let (arg, next) = parse_atom_arg(tokens, pos)?;
                pos = next;
                if mods.r_sub.is_none() && mods.r_sup.is_none() && mods.l_sub.is_none() {
                    mods.r_sub = Some(Box::new(arg));
                } else {
                    mods.l_sub = Some(Box::new(arg));
                }
            }
            _ => break,
        }
    }

    Ok((
        AtomicComposite {
            primary,
            mods,
        },
        pos,
    ))
}

fn parse_primary(tokens: &[Token], pos: usize) -> Result<(u32, usize), LexError> {
    match tokens.get(pos) {
        Some(Token::Symbol(id, _)) => Ok((*id, pos + 1)),
        Some(Token::LParen(_)) => {
            // parenthesized atomic `( atomic )`
            let (inner, next) = parse_atomic(tokens, pos + 1)?;
            match tokens.get(next) {
                Some(Token::RParen(_)) => Ok((inner.primary, next + 1)),
                _ => Err(LexError {
                    span: Span::new(pos, pos + 1),
                    message: "unclosed '(' in atomic".into(),
                }),
            }
        }
        Some(Token::Var(_, span)) => Err(LexError {
            span: *span,
            message: "a variable is not a primary symbol in an atomic composite".into(),
        }),
        Some(tok) => {
            let span = span_of(tok, pos);
            Err(LexError {
                span,
                message: "expected a symbol to begin an atomic composite".into(),
            })
        }
        None => Err(LexError {
            span: Span::new(pos, pos),
            message: "unexpected end of input in atomic composite".into(),
        }),
    }
}

fn parse_atom_arg(tokens: &[Token], pos: usize) -> Result<(AtomicComposite, usize), LexError> {
    match tokens.get(pos) {
        Some(Token::LBrace(_)) => parse_atomic(tokens, pos + 1).and_then(|(ac, next)| {
            match tokens.get(next) {
                Some(Token::RBrace(_)) => Ok((ac, next + 1)),
                _ => Err(LexError {
                    span: Span::new(pos, pos + 1),
                    message: "unclosed '{' in mod argument".into(),
                }),
            }
        }),
        Some(_) => parse_atomic(tokens, pos),
        None => Err(LexError {
            span: Span::new(pos, pos),
            message: "missing mod argument".into(),
        }),
    }
}

fn span_of(tok: &Token, _pos: usize) -> Span {
    // Best-effort span; tokens that carry one are used directly.
    match tok {
        Token::Symbol(_, s) | Token::Var(_, s) | Token::LParen(s) | Token::RParen(s) => *s,
        _ => Span::new(0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(items: Vec<Token>) -> Vec<Token> {
        items
    }

    #[test]
    fn parses_bare_symbol() {
        let t = toks(vec![Token::Symbol(5, Span::new(0, 0))]);
        let (ac, next) = parse_atomic(&t, 0).unwrap();
        assert_eq!(ac.primary, 5);
        assert_eq!(next, 1);
    }

    #[test]
    fn parses_superscript_mod() {
        // \mathbb{Z}^+  => Symbol(1) Superscript LBrace Symbol(2) RBrace
        let t = toks(vec![
            Token::Symbol(1, Span::new(0, 0)),
            Token::Superscript(Span::new(0, 0)),
            Token::Symbol(2, Span::new(0, 0)),
        ]);
        let (ac, _next) = parse_atomic(&t, 0).unwrap();
        assert_eq!(ac.primary, 1);
        let rsup = ac.mods.r_sup.expect("r_sup");
        assert_eq!(rsup.primary, 2);
    }
}