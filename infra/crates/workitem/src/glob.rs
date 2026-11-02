//! Path globs: `/`-separated segments; `**` is any number of segments,
//! `*` any run of non-`/` characters, `?` one character, `[...]` a class.

fn segments(s: &str) -> Vec<&str> {
    s.split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .collect()
}

/// Does `pattern` match the concrete relative `path`?
pub fn matches(pattern: &str, path: &str) -> bool {
    match_segs(&segments(pattern), &segments(path))
}

fn match_segs(pat: &[&str], path: &[&str]) -> bool {
    match pat.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|i| match_segs(rest, &path[i..])),
        Some((p, rest)) => path.split_first().is_some_and(|(s, prest)| {
            match_seg(p.as_bytes(), s.as_bytes()) && match_segs(rest, prest)
        }),
    }
}

fn match_seg(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') => (0..=s.len()).any(|i| match_seg(&p[1..], &s[i..])),
        Some(b'?') => !s.is_empty() && match_seg(&p[1..], &s[1..]),
        Some(b'[') => match (class_end(p), s.first()) {
            (Some(end), Some(c)) => class_has(&p[1..end], *c) && match_seg(&p[end + 1..], &s[1..]),
            (None, Some(c)) => *c == b'[' && match_seg(&p[1..], &s[1..]),
            (_, None) => false,
        },
        Some(c) => s.first() == Some(c) && match_seg(&p[1..], &s[1..]),
    }
}

fn class_end(p: &[u8]) -> Option<usize> {
    p.iter().skip(2).position(|b| *b == b']').map(|i| i + 2)
}

fn class_has(class: &[u8], c: u8) -> bool {
    let (neg, body) = match class.first() {
        Some(b'!') | Some(b'^') => (true, &class[1..]),
        _ => (false, class),
    };
    let mut i = 0;
    let mut hit = false;
    while i < body.len() {
        if i + 2 < body.len() && body[i + 1] == b'-' {
            hit |= body[i] <= c && c <= body[i + 2];
            i += 3;
        } else {
            hit |= body[i] == c;
            i += 1;
        }
    }
    hit != neg
}

fn has_wild(seg: &str) -> bool {
    seg.bytes().any(|b| matches!(b, b'*' | b'?' | b'['))
}

/// Could some path match both patterns? Conservative for wildcard segments.
pub fn may_overlap(a: &str, b: &str) -> bool {
    overlap_segs(&segments(a), &segments(b))
}

fn overlap_segs(a: &[&str], b: &[&str]) -> bool {
    match (a.split_first(), b.split_first()) {
        (None, None) => true,
        (Some((&"**", ar)), _) => {
            overlap_segs(ar, b) || (!b.is_empty() && overlap_segs(a, &b[1..]))
        }
        (_, Some((&"**", br))) => {
            overlap_segs(a, br) || (!a.is_empty() && overlap_segs(&a[1..], b))
        }
        (None, _) | (_, None) => false,
        (Some((x, ar)), Some((y, br))) => seg_overlap(x, y) && overlap_segs(ar, br),
    }
}

fn seg_overlap(x: &str, y: &str) -> bool {
    match (has_wild(x), has_wild(y)) {
        (false, false) => x == y,
        (true, false) => match_seg(x.as_bytes(), y.as_bytes()),
        (false, true) => match_seg(y.as_bytes(), x.as_bytes()),
        (true, true) => {
            let lit = |s: &str| {
                s.bytes()
                    .take_while(|b| !matches!(b, b'*' | b'?' | b'['))
                    .collect::<Vec<u8>>()
            };
            let tail = |s: &str| {
                let mut t: Vec<u8> = s
                    .bytes()
                    .rev()
                    .take_while(|b| !matches!(b, b'*' | b'?' | b']'))
                    .collect();
                t.reverse();
                t
            };
            let (px, py) = (lit(x), lit(y));
            let (sx, sy) = (tail(x), tail(y));
            (px.starts_with(&py) || py.starts_with(&px)) && (sx.ends_with(&sy) || sy.ends_with(&sx))
        }
    }
}
